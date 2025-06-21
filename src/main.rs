#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_assignments)]

use std::time::{Duration, SystemTime, UNIX_EPOCH};
use std::thread;
use std::io;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::sync::mpsc::{self, Sender, Receiver};
use sha2::{Sha256, Digest};
use hex;
use chrono::{TimeZone, FixedOffset, Utc};
use std::collections::HashMap;
use rand::{Rng, thread_rng, rngs::OsRng};
use std::num::NonZeroU64;
use thiserror::Error;
use log::{info, error, warn};
use simple_logger::SimpleLogger;
use once_cell::sync::Lazy;
use ed25519_dalek::{Signature, SigningKey, VerifyingKey, Signer, Verifier};
use base64::encode;

// Custom error type for the application
#[derive(Error, Debug)]
pub enum BlockchainError {
    #[error("IO error: {0}")]
    Io(#[from] io::Error),
    
    #[error("Time error: {0}")]
    Time(String),
    
    #[error("Wallet error: {0}")]
    Wallet(String),
    
    #[error("Transaction error: {0}")]
    Transaction(String),
    
    #[error("Block error: {0}")]
    Block(String),
    
    #[error("System error: {0}")]
    System(String),
}

// Type alias for Result with our custom error type
type Result<T> = std::result::Result<T, BlockchainError>;

// Constants for the application
static SUPPORTED_CURRENCIES: Lazy<Vec<&'static str>> = Lazy::new(|| vec!["ZUX", "USDZ"]);

// Constants for AMM pool
const AMM_POOL_ADDRESS: &str = "AMM_POOL_ZUX_USDZ";
const PRICE_UPDATE_INTERVAL_MS: u64 = 50; // Update price display more frequently for responsive charts

// Define the character set and constants for unique wallet address generation
const CHARSET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ";
const N: u64 = 62; // Size of the alphabet
const CODE_LEN: usize = 7; // Length of each code
const MODULUS: u64 = 3_521_614_606_208; // 62^7 = 3,521,614,606,208
const SYSTEM_WALLET_ADDRESS: &str = "SYSTEM";

/// Convert a number to a 7-character base-62 string
/// 
/// This function takes a u64 number and converts it to a base-62 string
/// representation using the defined character set.
fn to_code(x: u64) -> String {
    if x == 0 {
        return "0".repeat(CODE_LEN);
    }
    
    let mut digits = Vec::with_capacity(CODE_LEN);
    let mut num = x;
    
    while num > 0 {
        let idx = (num % N) as usize;
        digits.push(CHARSET[idx] as char);
        num /= N;
    }
    
    // Pad with leading zeros if necessary
    while digits.len() < CODE_LEN {
        digits.push('0');
    }
    
    digits.reverse();
    digits.into_iter().collect()
}

/// Struct to hold the generator state for unique wallet addresses
pub struct UniqueCodeGenerator {
    counter: u64,
    a: NonZeroU64, // Multiplier, coprime with MODULUS
    b: u64,        // Offset
    used_codes: HashMap<String, bool>, // Track used codes to prevent collisions
}

impl UniqueCodeGenerator {
    /// Initialize with a cryptographically secure random secret
    pub fn new() -> Result<Self> {
        // Use OsRng for better randomness
        let mut secret = [0u8; 32];
        OsRng.fill(&mut secret);

        // Derive 'a' from the secret, ensuring it's coprime with 62^7 (not divisible by 2 or 31)
        let a = loop {
            // Generate fresh random bytes for each attempt
            let mut candidate_bytes = [0u8; 8];
            OsRng.fill(&mut candidate_bytes);
            
            let candidate = u64::from_be_bytes(candidate_bytes);
            
            // Ensure candidate is not 0 and is coprime with MODULUS
            // We need to check that gcd(candidate, MODULUS) = 1
            // Since MODULUS = 62^7 and 62 = 2 * 31, we check that candidate is not divisible by 2 or 31
            if candidate > 0 && candidate % 2 != 0 && candidate % 31 != 0 {
                break candidate;
            }
            // No sleep needed, just continue the loop with new random values
        };
        
        let a = NonZeroU64::new(a).ok_or_else(|| {
            BlockchainError::System("Failed to generate valid multiplier for address generator".to_string())
        })?;

        // Generate a separate random value for 'b'
        let mut b_bytes = [0u8; 8];
        OsRng.fill(&mut b_bytes);
        let b = u64::from_be_bytes(b_bytes) % MODULUS;

        Ok(UniqueCodeGenerator { 
            counter: 0, 
            a, 
            b,
            used_codes: HashMap::new(),
        })
    }

    /// Generate the next unique code
    pub fn generate(&mut self) -> Result<String> {
        // Try up to 10 times to generate a unique code
        for _ in 0..10 {
            // Compute x = (a * counter + b) mod 62^7 using u128 to avoid overflow
            let x = (self.a.get() as u128 * self.counter as u128 + self.b as u128) % MODULUS as u128;
            let x = x as u64;
            self.counter = self.counter.wrapping_add(1); // Increment counter
            
            let code = to_code(x);
            
            // Check if this code has been used before
            if !self.used_codes.contains_key(&code) {
                self.used_codes.insert(code.clone(), true);
                return Ok(code);
            }
        }
        
        // If we couldn't generate a unique code after several attempts
        Err(BlockchainError::System("Failed to generate unique wallet address".to_string()))
    }
    
    /// Reserve a specific code to prevent it from being generated
    pub fn reserve_code(&mut self, code: &str) {
        self.used_codes.insert(code.to_string(), true);
    }
}

/// Wallet structure to hold wallet data in memory with multiple currencies
#[derive(Debug, Clone)]
struct TradingStrategy {
    price_history: Vec<f64>,    // Recent price history for analysis
    last_trade_time: u64,       // Timestamp of last trade
    whale_mode: bool,           // Some wallets are "whales" that make massive trades
    mega_whale_mode: bool,      // Ultra-large whales that can move markets dramatically
    fomo_threshold: f64,        // Price increase that triggers FOMO buying
    panic_threshold: f64,       // Price decrease that triggers panic selling
    manipulation_intent: i8,    // -1: bear, 0: neutral, 1: bull (for market manipulation)
}

impl TradingStrategy {
    fn new(initial_price: f64) -> Self {
        let mut rng = rand::thread_rng();
        
        // Randomly assign whale status (10% chance, up from 5%)
        let whale_mode = rng.gen_bool(0.10);
        
        // Mega whales (1% chance) - can move markets dramatically
        let mega_whale_mode = rng.gen_bool(0.01);
        
        // Random FOMO and panic thresholds (even more sensitive: 0.5-3%)
        let fomo_threshold = rng.gen_range(0.005..0.03); // 0.5-3% price increase triggers FOMO
        let panic_threshold = rng.gen_range(0.005..0.03); // 0.5-3% price decrease triggers panic
        
        // Market manipulation intent (-1: bear, 0: neutral, 1: bull)
        // This determines if the wallet tries to manipulate price direction
        let manipulation_intent = if rng.gen_bool(0.30) {
            // 30% chance to have manipulation intent
            if rng.gen_bool(0.5) { 1 } else { -1 }
        } else {
            0 // neutral
        };
        
        TradingStrategy {
            price_history: vec![initial_price],
            last_trade_time: 0,
            whale_mode,
            mega_whale_mode,
            fomo_threshold,
            panic_threshold,
            manipulation_intent,
        }
    }
    
    fn update_price_history(&mut self, current_price: f64) {
        self.price_history.push(current_price);
        
        // Keep only the last 3 price points - extremely short-term memory
        if self.price_history.len() > 3 {
            self.price_history.remove(0);
        }
    }
    
    fn decide_action(&mut self, current_price: f64, current_time: u64, wallet_zux: f64, wallet_usdz: f64) -> (TradeAction, f64) {
        // Update price history
        self.update_price_history(current_price);
        
        // 99% chance to make a trade on every opportunity (hyper-active trading)
        if rand::thread_rng().gen_bool(0.99) {
            // Get the previous price if available
            let previous_price = if self.price_history.len() > 1 {
                self.price_history[self.price_history.len() - 2]
            } else {
                current_price // Use current price if no history
            };
            
            // Calculate price change percentage
            let price_change_pct = (current_price - previous_price) / previous_price;
            
            // Mega whale market manipulation (these can move markets dramatically)
            if self.mega_whale_mode && rand::thread_rng().gen_bool(0.8) {
                // 80% chance for mega whales to act
                match self.manipulation_intent {
                    1 => { // Bullish manipulation
                        if wallet_usdz > 0.0 {
                            // Buy with 95-100% of USDZ balance to pump price
                            let position_size = wallet_usdz * rand::thread_rng().gen_range(0.95..1.0);
                            return (TradeAction::Buy, position_size);
                        }
                    },
                    -1 => { // Bearish manipulation
                        if wallet_zux > 0.0 {
                            // Sell with 95-100% of ZUX balance to dump price
                            let position_size = wallet_zux * rand::thread_rng().gen_range(0.95..1.0);
                            return (TradeAction::Sell, position_size);
                        }
                    },
                    _ => {} // Neutral, continue with normal logic
                }
            }
            
            // FOMO buying - buy more aggressively when price is rising
            if price_change_pct > self.fomo_threshold {
                // FOMO buy with 90% chance when price is rising (up from 80%)
                if rand::thread_rng().gen_bool(0.9) && wallet_usdz > 0.0 {
                    // Determine position size - extreme FOMO uses 90-100% of balance
                    let position_size = wallet_usdz * rand::thread_rng().gen_range(0.9..1.0);
                    return (TradeAction::Buy, position_size);
                }
            }
            
            // Panic selling - sell aggressively when price is falling
            if price_change_pct < -self.panic_threshold {
                // Panic sell with 90% chance when price is falling (up from 80%)
                if rand::thread_rng().gen_bool(0.9) && wallet_zux > 0.0 {
                    // Determine position size - panic selling uses 90-100% of balance
                    let position_size = wallet_zux * rand::thread_rng().gen_range(0.9..1.0);
                    return (TradeAction::Sell, position_size);
                }
            }
            
            // Regular whale manipulation - make massive trades to move the market
            if self.whale_mode && rand::thread_rng().gen_bool(0.5) {
                // 50% chance for whales to act (up from 30%)
                if rand::thread_rng().gen_bool(0.5) && wallet_usdz > 0.0 {
                    // Whale buy - use 90-100% of USDZ balance
                    let position_size = wallet_usdz * rand::thread_rng().gen_range(0.9..1.0);
                    return (TradeAction::Buy, position_size);
                } else if wallet_zux > 0.0 {
                    // Whale sell - use 90-100% of ZUX balance
                    let position_size = wallet_zux * rand::thread_rng().gen_range(0.9..1.0);
                    return (TradeAction::Sell, position_size);
                }
            }
            
            // Random trading with extreme position sizing
            if rand::thread_rng().gen_bool(0.5) && wallet_usdz > 0.0 {
                // Buy with 70-100% of available USDZ (up from 50-100%)
                let position_size = wallet_usdz * rand::thread_rng().gen_range(0.7..1.0);
                return (TradeAction::Buy, position_size);
            } else if wallet_zux > 0.0 {
                // Sell with 70-100% of available ZUX (up from 50-100%)
                let position_size = wallet_zux * rand::thread_rng().gen_range(0.7..1.0);
                return (TradeAction::Sell, position_size);
            }
        }
        
        // Default action is to hold
        (TradeAction::Hold, 0.0)
    }
}

#[derive(Debug, Clone, PartialEq)]
enum TradeAction {
    Buy,
    Sell,
    Hold,
}

#[derive(Debug, Clone)]
struct Wallet {
    private_key: Vec<u8>,      // Ed25519 private key bytes
    public_key: Vec<u8>,       // Ed25519 public key bytes
    address: String,           // Unique wallet address
    balances: HashMap<String, f64>, // Map of currency code to balance with 9 decimal points
    trading_strategy: Option<TradingStrategy>, // Optional trading strategy
}

impl Wallet {
    /// Create a new wallet with empty balances
    fn new(private_key: Vec<u8>, public_key: Vec<u8>, address: String) -> Self {
        let mut balances = HashMap::new();
        // Initialize with zero balance for all supported currencies
        for currency in SUPPORTED_CURRENCIES.iter() {
            balances.insert((*currency).to_string(), 0.0);
        }
        
        Wallet {
            private_key,
            public_key,
            address,
            balances,
            trading_strategy: None,
        }
    }
    
    /// Initialize trading strategy for this wallet
    fn initialize_trading_strategy(&mut self, initial_price: f64) {
        self.trading_strategy = Some(TradingStrategy::new(initial_price));
    }
    
    /// Get the private key as a base64 string for display purposes
    fn private_key_base64(&self) -> String {
        encode(&self.private_key)
    }
    
    /// Get the public key as a base64 string for display purposes
    fn public_key_base64(&self) -> String {
        encode(&self.public_key)
    }
    
    /// Get the Ed25519 signing key for signing operations
    fn get_signing_key(&self) -> Result<SigningKey> {
        // Convert private key bytes to a fixed-size array
        let private_key_bytes: [u8; 32] = self.private_key.as_slice().try_into().map_err(|_| {
            BlockchainError::Wallet(format!("Invalid private key length"))
        })?;
        
        // Create a SigningKey from the bytes
        let signing_key = SigningKey::from_bytes(&private_key_bytes);
        
        Ok(signing_key)
    }
    
    /// Get the Ed25519 verifying key for verification operations
    fn get_verifying_key(&self) -> Result<VerifyingKey> {
        // Convert public key bytes to a fixed-size array
        let public_key_bytes: [u8; 32] = self.public_key.as_slice().try_into().map_err(|_| {
            BlockchainError::Wallet(format!("Invalid public key length"))
        })?;
        
        // Create a VerifyingKey from the bytes
        let verifying_key = VerifyingKey::from_bytes(&public_key_bytes).map_err(|e| {
            BlockchainError::Wallet(format!("Invalid public key: {}", e))
        })?;
        
        Ok(verifying_key)
    }
    
    /// Get the balance for a specific currency
    fn get_balance(&self, currency: &str) -> f64 {
        *self.balances.get(currency).unwrap_or(&0.0)
    }
    
    /// Set the balance for a specific currency
    fn set_balance(&mut self, currency: &str, amount: f64) {
        self.balances.insert(currency.to_string(), amount);
    }
    
    /// Add to the balance for a specific currency
    fn add_balance(&mut self, currency: &str, amount: f64) -> Result<()> {
        let current = self.get_balance(currency);
        let new_balance = current + amount;
        self.set_balance(currency, new_balance);
        Ok(())
    }
    
    /// Subtract from the balance for a specific currency
    fn subtract_balance(&mut self, currency: &str, amount: f64) -> Result<()> {
        let current = self.get_balance(currency);
        if current < amount {
            return Err(BlockchainError::Wallet(
                format!("Insufficient balance for wallet {}: has {:.9} {}, needs {:.9} {}", 
                       self.address, current, currency, amount, currency)
            ));
        }
        self.set_balance(currency, current - amount);
        Ok(())
    }
}

/// Transaction structure to represent blockchain activity
#[derive(Debug, Clone)]
struct Transaction {
    sender: String,
    recipient: String,
    amount: f64,
    currency: String, // Currency code (ZUX, USDZ, etc.)
    timestamp: u64,
    signature: Vec<u8>, // Ed25519 cryptographic signature
    sender_public_key: Vec<u8>, // Sender's public key for signature verification
}

impl Transaction {
    /// Create a new transaction
    fn new(sender: String, recipient: String, amount: f64, currency: String, 
           timestamp: u64, signature: Vec<u8>, sender_public_key: Vec<u8>) -> Self {
        Transaction {
            sender,
            recipient,
            amount,
            currency,
            timestamp,
            signature,
            sender_public_key,
        }
    }
    
    /// Get the transaction data that would be signed
    fn get_signing_data(&self) -> String {
        format!("{}{}{}{}{}", 
            self.sender, self.recipient, self.amount, self.currency, self.timestamp)
    }
    
    /// Verify that the transaction is valid, including cryptographic signature
    fn verify(&self) -> Result<()> {
        // Check that amount is greater than zero
        if self.amount <= 0.0 {
            return Err(BlockchainError::Transaction("Transaction amount must be greater than zero".to_string()));
        }
        
        // Check that the currency is supported
        if !SUPPORTED_CURRENCIES.contains(&self.currency.as_str()) {
            return Err(BlockchainError::Transaction(
                format!("Unsupported currency: {}", self.currency)
            ));
        }
        
        // Verify the cryptographic signature
        let verifying_key = VerifyingKey::from_bytes(&self.sender_public_key.as_slice().try_into().map_err(|_| {
            BlockchainError::Transaction(format!("Invalid public key length"))
        })?).map_err(|e| BlockchainError::Transaction(format!("Invalid public key: {}", e)))?;
            
        // Convert signature bytes to a fixed-size array
        let signature_bytes: [u8; 64] = self.signature.as_slice().try_into().map_err(|_| {
            BlockchainError::Transaction(format!("Invalid signature length"))
        })?;
        
        // Create a Signature from the bytes
        let signature = Signature::from_bytes(&signature_bytes);
            
        let message = self.get_signing_data();
        
        verifying_key.verify(message.as_bytes(), &signature)
            .map_err(|e| BlockchainError::Transaction(format!("Signature verification failed: {}", e)))?;
        
        Ok(())
    }
    
    /// Get a hash of the transaction data
    fn hash(&self) -> String {
        let data = format!("{}{}{}{}{}", 
            self.sender, self.recipient, self.amount, self.currency, self.timestamp);
        let mut hasher = Sha256::new();
        hasher.update(data.as_bytes());
        hex::encode(hasher.finalize())
    }
}

/// Function to create a new wallet with initial balances using Ed25519 cryptography
fn create_wallet(code_generator: &mut UniqueCodeGenerator, initial_balance: f64) -> Result<Wallet> {
    // Generate a cryptographically secure Ed25519 keypair
    let mut rng = thread_rng();
    let signing_key = SigningKey::generate(&mut rng);
    let verifying_key = signing_key.verifying_key();
    
    // Extract the private and public keys
    let private_key = signing_key.to_bytes().to_vec();
    let public_key = verifying_key.to_bytes().to_vec();
    
    // Generate a guaranteed unique address using our code generator
    let address = code_generator.generate()?;
    
    // Create a new wallet with empty balances
    let mut wallet = Wallet::new(private_key, public_key, address);
    
    // Set initial balances if specified
    if initial_balance > 0.0 {
        for currency in SUPPORTED_CURRENCIES.iter() {
            wallet.set_balance(currency, initial_balance);
        }
    }
    
    Ok(wallet)
}

/// Function to create a wallet and return it without setting balances
fn create_wallet_without_balance(code_generator: &mut UniqueCodeGenerator) -> Result<Wallet> {
    create_wallet(code_generator, 0.0)
}

/// Function to create a system wallet with special address and high initial balance
fn create_system_wallet(code_generator: &mut UniqueCodeGenerator) -> Result<Wallet> {
    // Generate a cryptographically secure Ed25519 keypair
    let mut rng = thread_rng();
    let signing_key = SigningKey::generate(&mut rng);
    let verifying_key = signing_key.verifying_key();
    
    // Log a warning about system wallet creation
    warn!("Creating system wallet with high initial balance");
    
    // Extract the private and public keys
    let private_key = signing_key.to_bytes().to_vec();
    let public_key = verifying_key.to_bytes().to_vec();
    
    // Reserve the system wallet address to prevent it from being generated for other wallets
    code_generator.reserve_code(SYSTEM_WALLET_ADDRESS);
    
    // Create a new wallet with the system address
    let mut wallet = Wallet::new(private_key, public_key, SYSTEM_WALLET_ADDRESS.to_string());
    
    // Set initial balances for the system wallet: 1B ZUX and 5B USDZ
    wallet.set_balance("ZUX", 1_000_000_000.0); // 1 billion ZUX
    wallet.set_balance("USDZ", 5_000_000_000.0); // 5 billion USDZ
    
    // Log the creation of the system wallet with a warning about its special status
    warn!("Created system wallet with address '{}'. This wallet has special privileges and high initial balance.", SYSTEM_WALLET_ADDRESS);
    
    Ok(wallet)
}

// Enum to track different types of blockchain events
/// Structure to represent a price point with timestamp
#[derive(Clone, Debug)]
struct PricePoint {
    timestamp: u64,
    price: f64,
}

/// AMM Pool structure implementing Constant Product Market Maker (x * y = k)
#[derive(Clone, Debug)]
struct AmmPool {
    zux_reserve: f64,
    usd_reserve: f64,
    k_constant: f64,
    fee_percent: f64,
    price_history: Vec<PricePoint>,
}

impl AmmPool {
    /// Create a new AMM pool with initial liquidity
    fn new(initial_zux: f64, initial_usd: f64, fee_percent: f64) -> Self {
        let k_constant = initial_zux * initial_usd;
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_secs();
        
        let initial_price = initial_usd / initial_zux;
        
        AmmPool {
            zux_reserve: initial_zux,
            usd_reserve: initial_usd,
            k_constant,
            fee_percent,
            price_history: vec![PricePoint { timestamp, price: initial_price }],
        }
    }
    
    /// Get the current ZUX price in USD
    fn get_zux_price(&self) -> f64 {
        self.usd_reserve / self.zux_reserve
    }
    
    /// Calculate the output amount for a swap based on constant product formula
    fn calculate_output_amount(&self, input_amount: f64, input_is_zux: bool) -> f64 {
        let (input_reserve, output_reserve) = if input_is_zux {
            (self.zux_reserve, self.usd_reserve)
        } else {
            (self.usd_reserve, self.zux_reserve)
        };
        
        // Apply fee to input amount
        let input_with_fee = input_amount * (1.0 - self.fee_percent / 100.0);
        
        // Calculate output based on constant product formula: (x + dx) * (y - dy) = k
        // Therefore: dy = y - k / (x + dx)
        let numerator = input_with_fee * output_reserve;
        let denominator = input_reserve + input_with_fee;
        
        // Calculate result, ensuring we get at least 0.000000001 if the input is non-zero
        let result = numerator / denominator;
        if input_amount > 0.0 && result < 0.000000001 {
            0.000000001 // Ensure minimum output for non-zero input
        } else {
            result
        }
    }
    
    /// Swap ZUX for USD
    fn swap_zux_to_usd(&mut self, zux_amount: f64) -> Result<f64> {
        if zux_amount <= 0.0 {
            return Err(BlockchainError::Transaction("Swap amount must be greater than zero".to_string()));
        }
        
        let usd_output = self.calculate_output_amount(zux_amount, true);
        
        if usd_output < 0.000000001 {
            return Err(BlockchainError::Transaction("Swap would result in too small output".to_string()));
        }
        
        // Update reserves
        self.zux_reserve += zux_amount;
        self.usd_reserve -= usd_output;
        
        // Update k constant
        self.k_constant = self.zux_reserve * self.usd_reserve;
        
        // Record new price point
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_secs();
        
        let new_price = self.get_zux_price();
        self.price_history.push(PricePoint { timestamp, price: new_price });
        
        // Limit price history size to avoid memory issues
        if self.price_history.len() > 1000 {
            self.price_history.remove(0);
        }
        
        Ok(usd_output)
    }
    
    /// Swap USD for ZUX
    fn swap_usd_to_zux(&mut self, usd_amount: f64) -> Result<f64> {
        if usd_amount <= 0.0 {
            return Err(BlockchainError::Transaction("Swap amount must be greater than zero".to_string()));
        }
        
        let zux_output = self.calculate_output_amount(usd_amount, false);
        
        if zux_output < 0.000000001 {
            return Err(BlockchainError::Transaction("Swap would result in too small output".to_string()));
        }
        
        // Update reserves
        self.usd_reserve += usd_amount;
        self.zux_reserve -= zux_output;
        
        // Update k constant
        self.k_constant = self.zux_reserve * self.usd_reserve;
        
        // Record new price point
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_secs();
        
        let new_price = self.get_zux_price();
        self.price_history.push(PricePoint { timestamp, price: new_price });
        
        // Limit price history size to avoid memory issues
        if self.price_history.len() > 1000 {
            self.price_history.remove(0);
        }
        
        Ok(zux_output)
    }
    
    /// Get recent price history for display
    fn get_recent_price_history(&self, count: usize) -> Vec<PricePoint> {
        let start_idx = if self.price_history.len() > count {
            self.price_history.len() - count
        } else {
            0
        };
        
        self.price_history[start_idx..].to_vec()
    }
}

#[derive(Clone, Debug)]
enum BlockEvent {
    Genesis,
    WalletCreation(String), // Wallet address
    TokenCredit(String, String, f64), // Wallet address, currency code, amount
    AmmPoolCreation(String), // AMM Pool address
    Swap(String, bool, f64, f64), // Wallet address, is_zux_to_usd, input_amount, output_amount
}

// Function to create multiple wallets with individual blocks for each event
fn create_multiple_wallets(count: usize, current_block_id: &mut u64, parent_hash: &mut String, 
                          network_name: &str, block_ver: &str, inception_year: u16,
                          code_generator: &mut UniqueCodeGenerator) -> Result<HashMap<String, Wallet>> {
    let mut wallets = HashMap::new();
    info!("Creating {} wallets in memory...", count);
    
    // No file operations - everything stays in memory
    info!("Generated on: {}", Utc::now().format("%Y-%m-%d %H:%M:%S UTC"));
    info!("Each wallet will be credited from System Wallet with:");
    info!("  - 100 ZUX tokens");
    info!("  - 500 USDZ tokens");
    
    info!("Block generation is event-triggered based on computation completion");
    
    for i in 0..count {
        // Create wallet without balance using the unique code generator
        let wallet = create_wallet_without_balance(code_generator)?;
        let address_info = format!("Wallet #{}: Address = {} | Balances = 0 ZUX, 0 USDZ (will be credited)", i+1, wallet.address);
        
        // Print progress every 100 wallets or for the first one
        if i % 100 == 0 || i == 0 {
            info!("{}", address_info);
        }
        
        // Create a block for this wallet creation event
        *current_block_id += 1;
        let event = BlockEvent::WalletCreation(wallet.address.clone());
        let (new_block_hash, _) = create_block(
            *current_block_id,
            parent_hash,
            &[], // No transactions for wallet creation
            network_name,
            block_ver,
            inception_year,
            &event
        )?;
        *parent_hash = new_block_hash;
        
        // Store wallet in the map
        wallets.insert(wallet.address.clone(), wallet);
        
        // Print progress every 100 blocks or for the first one
        if i % 100 == 0 || i == 0 {
            info!("Block #{} created for wallet creation event.", current_block_id);
        }
    }
    
    info!("All {} wallets created successfully!", count);
    Ok(wallets)
}

// Function to display wallet information
fn display_wallet(wallet: &Wallet) {
    println!("\n________________________ZUX Wallet_________________________________________");
    println!("Address         : {}", wallet.address);
    println!("Public Key      : {}", wallet.public_key_base64());
    println!("Private Key     : {}", wallet.private_key_base64());
    println!("Balances:");
    println!("  - ZUX         : {:.9}", wallet.balances.get("ZUX").unwrap_or(&0.0));
    println!("  - USDZ        : {:.9}", wallet.balances.get("USDZ").unwrap_or(&0.0));
    println!("____________________________________________________________________________\n");
}

// Function to display AMM pool information
fn display_amm_pool(amm_pool: &AmmPool) {
    println!("\n________________________ZUX/USDZ AMM Pool_________________________________");
    println!("ZUX Reserve     : {}", amm_pool.zux_reserve);
    println!("USDZ Reserve    : {}", amm_pool.usd_reserve);
    println!("K Constant      : {}", amm_pool.k_constant);
    println!("Fee Percentage  : {}%", amm_pool.fee_percent);
    println!("Current Price   : {:.6} USDZ per ZUX", amm_pool.get_zux_price());
    println!("____________________________________________________________________________\n");
}

// Function to create a new transaction
/// Create a transaction with proper validation and error handling using Ed25519 signatures
/// Takes sender wallet reference instead of wallet info tuple
fn create_transaction(
    sender_wallet: &Wallet, 
    recipient_address: &str, 
    amount: f64, 
    currency: &str,
    _wallets: &HashMap<String, Wallet> // Keeping parameter for compatibility but not using it
) -> Result<Transaction> {
    // Validate transaction parameters
    if amount <= 0.0 {
        return Err(BlockchainError::Transaction("Transaction amount must be greater than zero".to_string()));
    }
    
    if !SUPPORTED_CURRENCIES.contains(&currency) {
        return Err(BlockchainError::Transaction(format!("Unsupported currency: {}", currency)));
    }
    
    // Check if sender has sufficient balance directly from the sender_wallet
    let sender_balance = sender_wallet.get_balance(currency);
    if sender_balance < amount {
        return Err(BlockchainError::Transaction(
            format!("Insufficient balance: {:.9} {} (needed: {:.9})", sender_balance, currency, amount)
        ));
    }
    
    // Get current timestamp with proper error handling
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| BlockchainError::Time(format!("Time error: {}", e)))?            
        .as_secs();
    
    // Create transaction data for signing
    let transaction_data = format!("{}{}{:.9}{}{}", 
        sender_wallet.address, recipient_address, amount, currency, timestamp);
    
    // Get the signing key for signing
    let signing_key = sender_wallet.get_signing_key()?;
    
    // Sign the transaction data using Ed25519
    let signature = signing_key.sign(transaction_data.as_bytes());
    
    // Create and return the transaction
    Ok(Transaction {
        sender: sender_wallet.address.clone(),
        recipient: recipient_address.to_string(),
        amount,
        currency: currency.to_string(),
        timestamp,
        signature: signature.to_bytes().to_vec(),
        sender_public_key: sender_wallet.public_key.clone(),
    })
}

/// Create a swap transaction between a wallet and the AMM pool
fn create_swap_transaction(
    wallet: &Wallet,
    is_zux_to_usd: bool,
    input_amount: f64,
    output_amount: f64
) -> Result<Transaction> {
    // Validate transaction parameters
    if input_amount <= 0.0 {
        return Err(BlockchainError::Transaction("Swap amount must be greater than zero".to_string()));
    }
    
    // Determine input and output currencies
    let (input_currency, output_currency) = if is_zux_to_usd {
        ("ZUX", "USDZ")
    } else {
        ("USDZ", "ZUX")
    };
    
    // Check if wallet has sufficient balance
    let wallet_balance = wallet.get_balance(input_currency);
    if wallet_balance < input_amount {
        return Err(BlockchainError::Transaction(
            format!("Insufficient balance: {:.9} {} (needed: {:.9})", wallet_balance, input_currency, input_amount)
        ));
    }
    
    // Get current timestamp
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| BlockchainError::Time(format!("Time error: {}", e)))?            
        .as_secs();
    
    // Create transaction data for signing
    // For swaps, we include both input and output amounts and currencies
    let transaction_data = format!("{}{}{:.9}{}{:.9}{}{}", 
        wallet.address, AMM_POOL_ADDRESS, input_amount, input_currency, 
        output_amount, output_currency, timestamp);
    
    // Get the signing key for signing
    let signing_key = wallet.get_signing_key()?;
    
    // Sign the transaction data using Ed25519
    let signature = signing_key.sign(transaction_data.as_bytes());
    
    // Create and return the transaction
    Ok(Transaction {
        sender: wallet.address.clone(),
        recipient: AMM_POOL_ADDRESS.to_string(),
        amount: input_amount,
        currency: input_currency.to_string(),
        timestamp,
        signature: signature.to_bytes().to_vec(),
        sender_public_key: wallet.public_key.clone(),
    })
}

/// Execute a swap between a wallet and the AMM pool
fn execute_swap(
    wallet: &mut Wallet,
    amm_pool: &mut AmmPool,
    is_zux_to_usd: bool,
    input_amount: f64
) -> Result<(f64, Transaction)> {
    // Determine input and output currencies
    let (input_currency, output_currency) = if is_zux_to_usd {
        ("ZUX", "USDZ")
    } else {
        ("USDZ", "ZUX")
    };
    
    // Check if wallet has sufficient balance
    let wallet_balance = wallet.get_balance(input_currency);
    if wallet_balance < input_amount {
        return Err(BlockchainError::Transaction(
            format!("Insufficient balance: {:.9} {} (needed: {:.9})", wallet_balance, input_currency, input_amount)
        ));
    }
    
    // Execute the swap in the AMM pool
    let output_amount = if is_zux_to_usd {
        amm_pool.swap_zux_to_usd(input_amount)?
    } else {
        amm_pool.swap_usd_to_zux(input_amount)?
    };
    
    // Create the swap transaction
    let transaction = create_swap_transaction(wallet, is_zux_to_usd, input_amount, output_amount)?;
    
    // Update wallet balances
    wallet.subtract_balance(input_currency, input_amount)?;
    wallet.add_balance(output_currency, output_amount)?;
    
    Ok((output_amount, transaction))
}

/// Create an intelligent swap transaction based on trading strategy
fn create_intelligent_swap(
    wallets: &mut HashMap<String, Wallet>,
    amm_pool: &mut AmmPool
) -> Result<(String, bool, f64, f64, Transaction)> {
    // Get all wallet addresses except the system wallet
    let wallet_addresses: Vec<String> = wallets.keys()
        .filter(|&addr| addr != SYSTEM_WALLET_ADDRESS)
        .cloned()
        .collect();
    
    let wallet_count = wallet_addresses.len();
    if wallet_count == 0 {
        return Err(BlockchainError::Transaction("No wallets available for swap".to_string()));
    }
    
    // Use cryptographically secure random number generator
    let mut rng = OsRng;
    
    // Select a random wallet
    let wallet_idx = rng.gen_range(0..wallet_count);
    let wallet_address = wallet_addresses[wallet_idx].clone();
    
    // Get the wallet
    let mut wallet = wallets.remove(&wallet_address)
        .ok_or_else(|| BlockchainError::Wallet(format!("Wallet not found: {}", wallet_address)))?;
    
    // Get current price and time
    let current_price = amm_pool.get_zux_price();
    let current_time = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::from_secs(0))
        .as_secs();
    
    // Initialize trading strategy if it doesn't exist
    if wallet.trading_strategy.is_none() {
        wallet.initialize_trading_strategy(current_price);
    }
    
    // Get the wallet's trading action
    let trading_action = {
        let zux_balance = wallet.get_balance("ZUX");
        let usdz_balance = wallet.get_balance("USDZ");
        let trading_strategy = wallet.trading_strategy.as_mut().unwrap();
        trading_strategy.decide_action(current_price, current_time, zux_balance, usdz_balance)
    };
    
    // Determine swap direction and amount based on trading action
    let (is_zux_to_usd, input_amount) = match trading_action {
        (TradeAction::Buy, position_size) => {
            // Buy ZUX with USDZ - ultra aggressive
            let is_zux_to_usd = false; // USDZ to ZUX
            let usdz_balance = wallet.get_balance("USDZ");
            
            // Skip if balance is too small
            if usdz_balance < 0.000001 {
                wallets.insert(wallet_address, wallet);
                return create_intelligent_swap(wallets, amm_pool);
            }
            
            let input_amount = position_size.min(usdz_balance);
            (is_zux_to_usd, input_amount)
        },
        (TradeAction::Sell, position_size) => {
            // Sell ZUX for USDZ - ultra aggressive
            let is_zux_to_usd = true; // ZUX to USDZ
            let zux_balance = wallet.get_balance("ZUX");
            
            // Skip if balance is too small
            if zux_balance < 0.000001 {
                wallets.insert(wallet_address, wallet);
                return create_intelligent_swap(wallets, amm_pool);
            }
            
            let input_amount = position_size.min(zux_balance);
            (is_zux_to_usd, input_amount)
        },
        (TradeAction::Hold, _) => {
            // Even for hold, make a smaller random trade
            let is_zux_to_usd = rng.gen_bool(0.5);
            
            let input_amount = if is_zux_to_usd {
                let zux_balance = wallet.get_balance("ZUX");
                
                // Skip if balance is too small
                if zux_balance < 0.000001 {
                    wallets.insert(wallet_address, wallet);
                    return create_intelligent_swap(wallets, amm_pool);
                }
                
                zux_balance * rng.gen_range(0.1..0.3) // Use 10-30% of ZUX balance
            } else {
                let usdz_balance = wallet.get_balance("USDZ");
                
                // Skip if balance is too small
                if usdz_balance < 0.000001 {
                    wallets.insert(wallet_address, wallet);
                    return create_intelligent_swap(wallets, amm_pool);
                }
                
                usdz_balance * rng.gen_range(0.1..0.3) // Use 10-30% of USDZ balance
            };
            
            (is_zux_to_usd, input_amount)
        },
    };
    
    // Ensure minimum trade amount and skip if too small
    if input_amount < 0.000001 {
        wallets.insert(wallet_address, wallet);
        return create_intelligent_swap(wallets, amm_pool);
    }
    
    // Execute the swap
    let result = execute_swap(&mut wallet, amm_pool, is_zux_to_usd, input_amount);
    
    // Handle errors by trying again with another wallet
    if result.is_err() {
        wallets.insert(wallet_address, wallet);
        return create_intelligent_swap(wallets, amm_pool);
    }
    
    let (output_amount, transaction) = result.unwrap();
    
    // Update last trade time
    if let Some(trading_strategy) = wallet.trading_strategy.as_mut() {
        trading_strategy.last_trade_time = current_time;
    }
    
    // Put the wallet back in the map
    wallets.insert(wallet_address.clone(), wallet);
    
    Ok((wallet_address, is_zux_to_usd, input_amount, output_amount, transaction))
}

/// Create a random swap transaction for simulation (kept for backward compatibility)
fn create_random_swap(
    wallets: &mut HashMap<String, Wallet>,
    amm_pool: &mut AmmPool
) -> Result<(String, bool, f64, f64, Transaction)> {
    // Get all wallet addresses except the system wallet
    let wallet_addresses: Vec<String> = wallets.keys()
        .filter(|&addr| addr != SYSTEM_WALLET_ADDRESS)
        .cloned()
        .collect();
    
    let wallet_count = wallet_addresses.len();
    if wallet_count == 0 {
        return Err(BlockchainError::Transaction("No wallets available for swap".to_string()));
    }
    
    // Use cryptographically secure random number generator
    let mut rng = OsRng;
    
    // Select a random wallet
    let wallet_idx = rng.gen_range(0..wallet_count);
    let wallet_address = wallet_addresses[wallet_idx].clone();
    
    // Randomly decide swap direction (ZUX to USD or USD to ZUX)
    let is_zux_to_usd = rng.gen_bool(0.5);
    
    // Get the wallet
    let mut wallet = wallets.remove(&wallet_address)
        .ok_or_else(|| BlockchainError::Wallet(format!("Wallet not found: {}", wallet_address)))?;
    
    // Determine input currency based on swap direction
    let input_currency = if is_zux_to_usd { "ZUX" } else { "USDZ" };
    
    // Get wallet balance for the input currency
    let wallet_balance = wallet.get_balance(input_currency);
    
    // Generate a random amount between 0.000000001 and wallet balance (max 100.0)
    let max_amount = f64::min(wallet_balance, 100.0);
    let input_amount = if max_amount > 0.000000001 {
        // Generate a random f64 between 0.000000001 and max_amount
        let random_factor = rng.gen_range(0.000000001..=1.0);
        (random_factor * max_amount).max(0.000000001) // Ensure minimum amount
    } else {
        // Skip this wallet if it has insufficient balance
        wallets.insert(wallet_address, wallet);
        return create_random_swap(wallets, amm_pool);
    };
    
    // Execute the swap
    let (output_amount, transaction) = execute_swap(&mut wallet, amm_pool, is_zux_to_usd, input_amount)?;
    
    // Put the wallet back in the map
    wallets.insert(wallet_address.clone(), wallet);
    
    Ok((wallet_address, is_zux_to_usd, input_amount, output_amount, transaction))
}

// Transfer functionality has been removed

/// Block structure to store all block information
#[derive(Debug, Clone)]
struct Block {
    id: u64,
    hash: String,
    parent_hash: String,
    state_root: String,
    timestamp: u64,
    block_class: String,
    block_type: String,
    version: String,
    inception_year: u16,
    network_name: String,
    transactions: Vec<Transaction>,
    event: BlockEvent,
    formatted_time: String,
    difficulty: u64,       // Mining difficulty target
    nonce: u64,            // Nonce used for mining
}

impl Block {
    /// Calculate a Merkle root hash from transactions and event data
    fn calculate_merkle_root(transactions: &[Transaction], event: &BlockEvent) -> String {
        // If there are no transactions, create a simple hash of the event
        if transactions.is_empty() {
            let event_data = match event {
                BlockEvent::Genesis => "genesis_block".to_string(),
                BlockEvent::WalletCreation(address) => format!("wallet_creation:{}", address),
                BlockEvent::TokenCredit(address, currency, amount) => 
                    format!("token_credit:{}:{}:{:.9}", address, currency, amount),
                BlockEvent::AmmPoolCreation(address) => 
                    format!("amm_pool_creation:{}", address),
                BlockEvent::Swap(address, is_zux_to_usd, input_amount, output_amount) => 
                    format!("swap:{}:{}:{:.9}:{:.9}", address, is_zux_to_usd, input_amount, output_amount),
            };
            
            let mut hasher = Sha256::new();
            hasher.update(event_data.as_bytes());
            return hex::encode(hasher.finalize());
        }
        
        // Create leaf nodes from transaction hashes
        let mut leaves: Vec<String> = transactions.iter()
            .map(|tx| {
                let data = tx.get_signing_data();
                let mut hasher = Sha256::new();
                hasher.update(data.as_bytes());
                hex::encode(hasher.finalize())
            })
            .collect();
            
        // Add event data as a leaf node
        let event_data = match event {
            BlockEvent::Genesis => "genesis_block".to_string(),
            BlockEvent::WalletCreation(address) => format!("wallet_creation:{}", address),
            BlockEvent::TokenCredit(address, currency, amount) => 
                format!("token_credit:{}:{}:{}", address, currency, amount),
            BlockEvent::AmmPoolCreation(address) => 
                format!("amm_pool_creation:{}", address),
            BlockEvent::Swap(address, is_zux_to_usd, input_amount, output_amount) => 
                format!("swap:{}:{}:{}:{}", address, is_zux_to_usd, input_amount, output_amount),
        };
        
        let mut event_hasher = Sha256::new();
        event_hasher.update(event_data.as_bytes());
        leaves.push(hex::encode(event_hasher.finalize()));
        
        // If there's only one leaf (one transaction + event), return it
        if leaves.len() == 1 {
            return leaves[0].clone();
        }
        
        // Build the Merkle tree by repeatedly hashing pairs of nodes
        while leaves.len() > 1 {
            let mut new_level = Vec::new();
            
            // Process pairs of nodes
            for i in (0..leaves.len()).step_by(2) {
                if i + 1 < leaves.len() {
                    // Hash the pair of nodes
                    let mut pair_hasher = Sha256::new();
                    pair_hasher.update(leaves[i].as_bytes());
                    pair_hasher.update(leaves[i+1].as_bytes());
                    new_level.push(hex::encode(pair_hasher.finalize()));
                } else {
                    // Odd number of nodes, promote the last one
                    new_level.push(leaves[i].clone());
                }
            }
            
            // Replace the current level with the new level
            leaves = new_level;
        }
        
        // Return the root hash
        leaves[0].clone()
    }
    
    /// Mine a block by finding a nonce that produces a hash with the required number of leading zeros
    fn mine_block(
        block_id: u64,
        parent_hash: &str,
        state_root: &str,
        timestamp: u64,
        block_class: &str,
        block_type: &str,
        block_ver: &str,
        inception_year: u16,
        network_name: &str,
        difficulty: u64
    ) -> Result<(String, u64)> {
        // For simulation purposes, we'll limit the maximum nonce to avoid infinite loops
        const MAX_NONCE: u64 = 1_000_000;
        
        // Create a difficulty target (number of leading zero bytes required)
        let target_prefix = "0".repeat(difficulty as usize);
        
        // Try different nonce values until we find a valid hash
        for nonce in 0..MAX_NONCE {
            // Create block header content for hashing
            let block_header_content = format!(
                "{}{}{}{}{}{}{}{}{}{}",
                block_id,
                parent_hash,
                state_root,
                timestamp,
                block_class,
                block_type,
                block_ver,
                inception_year,
                network_name,
                nonce
            );
            
            // Calculate block hash
            let mut block_hasher = Sha256::new();
            block_hasher.update(block_header_content.as_bytes());
            let hash = hex::encode(block_hasher.finalize());
            
            // Check if the hash meets the difficulty target
            if hash.starts_with(&target_prefix) {
                return Ok((hash, nonce));
            }
        }
        
        // If we reach here, we couldn't find a valid nonce within the limit
        Err(BlockchainError::Block(format!("Failed to mine block: could not find valid nonce within {} attempts", MAX_NONCE)))
    }
    
    /// Verify that the block hash is valid
    fn verify(&self) -> Result<()> {
        // Recreate the block header content
        let block_header_content = format!(
            "{}{}{}{}{}{}{}{}{}{}",
            self.id,
            self.parent_hash,
            self.state_root,
            self.timestamp,
            self.block_class,
            self.block_type,
            self.version,
            self.inception_year,
            self.network_name,
            self.nonce
        );
        
        // Calculate the hash
        let mut block_hasher = Sha256::new();
        block_hasher.update(block_header_content.as_bytes());
        let calculated_hash = hex::encode(block_hasher.finalize());
        
        // Verify that the calculated hash matches the stored hash
        if calculated_hash != self.hash {
            return Err(BlockchainError::Block(format!("Invalid block hash: expected {}, got {}", self.hash, calculated_hash)));
        }
        
        // Verify that the hash meets the difficulty target
        let target_prefix = "0".repeat(self.difficulty as usize);
        if !self.hash.starts_with(&target_prefix) {
            return Err(BlockchainError::Block(format!("Block hash does not meet difficulty target: {}", self.difficulty)));
        }
        
        // Verify all transactions in the block
        for tx in &self.transactions {
            tx.verify()?;
        }
        
        Ok(())
    }
    
    /// Create a new block with transaction and event information, including proof-of-work mining
    fn new(current_block_id: u64, parent_hash: &str, transactions: &[Transaction], 
           network_name: &str, block_ver: &str, inception_year: u16, event: &BlockEvent) -> Result<Self> {
        // Get current timestamp
        let creation_timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|e| BlockchainError::Time(format!("Time error: {}", e)))?            
            .as_secs();

        // Format the timestamp for display
        let utc_datetime = Utc.timestamp_opt(creation_timestamp as i64, 0)
            .single()
            .ok_or_else(|| BlockchainError::Time("Invalid timestamp".to_string()))?;

        let kuala_lumpur_tz = FixedOffset::east_opt(8 * 3600)
            .ok_or_else(|| BlockchainError::Time("Failed to create timezone offset".to_string()))?;

        let kl_datetime = utc_datetime.with_timezone(&kuala_lumpur_tz);
        let formatted_kl_time = kl_datetime.format("%Y-%m-%d %H:%M:%S %Z").to_string();

        // Determine block type based on event
        let block_type = match event {
            BlockEvent::Genesis => "Genesis",
            BlockEvent::WalletCreation(_) => "Wallet Creation",
            BlockEvent::TokenCredit(_, _, _) => "Token Credit",
            BlockEvent::AmmPoolCreation(_) => "AMM Pool Creation",
            BlockEvent::Swap(_, _, _, _) => "Token Swap",
        };
        
        let block_class = if network_name == "ZUX-Testnet" { "Private" } else { "Public" };

        // Create a merkle root from transactions using a more robust approach
        let state_root = Self::calculate_merkle_root(transactions, event);
        
        // Set mining difficulty - in a real blockchain this would adjust based on network hashrate
        // For this simulation, we'll use a fixed difficulty that requires a few leading zeros
        let difficulty = if block_type == "Genesis" { 1 } else { 2 }; // Require 1 or 2 leading zero bytes
        
        // Mine the block (find a valid nonce)
        let (hash, nonce) = Self::mine_block(
            current_block_id,
            parent_hash,
            &state_root,
            creation_timestamp,
            block_class,
            block_type,
            block_ver,
            inception_year,
            network_name,
            difficulty
        )?;

        // Create and return the block
        let block = Block {
            id: current_block_id,
            hash,
            parent_hash: parent_hash.to_string(),
            state_root,
            timestamp: creation_timestamp,
            difficulty,
            nonce,
            block_class: block_class.to_string(),
            block_type: block_type.to_string(),
            version: block_ver.to_string(),
            inception_year,
            network_name: network_name.to_string(),
            transactions: transactions.to_vec(),
            event: event.clone(),
            formatted_time: formatted_kl_time,
        };

        Ok(block)
    }

    /// Print block information to console
    fn print(&self) {
        println!("\nThis is a private simulation of a local blockchain that runs on a single deterministic node.\n");
        println!("________________________ZUX Block ({})_________________________________________", self.block_type);
        println!("Block ID         : {:08}", self.id);
        println!("Block Hash       : {}", self.hash);
        println!("Parent Hash      : {}", self.parent_hash);
        println!("State Root       : {}", self.state_root);
        println!("Creation Timestamp: {} (UNIX Epoch Seconds) ({})\n", self.timestamp, self.formatted_time);
        println!("Difficulty       : {}", self.difficulty);
        println!("Nonce            : {}", self.nonce);
        println!("Block Class      : {}", self.block_class);
        println!("Block Type       : {}", self.block_type);
        println!("Block Version    : {} // The very first version", self.version);
        println!("Inception Year   : {}", self.inception_year);
        println!("Network Name     : {} // Since its running on a private testnet", self.network_name);
        
        // Print event details
        match &self.event {
            BlockEvent::Genesis => {
                println!("Event           : Genesis Block Creation");
            },
            BlockEvent::WalletCreation(address) => {
                println!("Event           : Wallet Creation");
                println!("Wallet Address  : {}", address);
            },
            BlockEvent::TokenCredit(address, currency, amount) => {
                println!("Event           : Token Credit");
                println!("Wallet Address  : {}", address);
                println!("Currency        : {}", currency);
                println!("Credit Amount   : {}", amount);
            },
            BlockEvent::AmmPoolCreation(address) => {
                println!("Event           : AMM Pool Creation");
                println!("Pool Address    : {}", address);
            },
            BlockEvent::Swap(address, is_zux_to_usd, input_amount, output_amount) => {
                println!("Event           : Token Swap");
                println!("Wallet Address  : {}", address);
                
                if *is_zux_to_usd {
                    println!("Swap Direction  : ZUX → USDZ");
                    println!("Input Amount    : {} ZUX", input_amount);
                    println!("Output Amount   : {} USDZ", output_amount);
                } else {
                    println!("Swap Direction  : USDZ → ZUX");
                    println!("Input Amount    : {} USDZ", input_amount);
                    println!("Output Amount   : {} ZUX", output_amount);
                }
                
                // Calculate and display the effective price
                let effective_price = if *is_zux_to_usd {
                    *output_amount as f64 / *input_amount as f64
                } else {
                    *input_amount as f64 / *output_amount as f64
                };
                
                println!("Effective Price : {:.6} USDZ per ZUX", effective_price);
            },
        }
        
        // Print transaction details if any
        println!("Transactions     : {} transaction(s)", self.transactions.len());
        for (i, tx) in self.transactions.iter().enumerate() {
            println!("  Transaction #{}", i + 1);
            println!("    Sender    : {}", tx.sender);
            println!("    Recipient : {}", tx.recipient);
            println!("    Amount    : {} {}", tx.amount, tx.currency);
            println!("    Timestamp : {}", tx.timestamp);
        }
        
        println!("____________________________________________________________________________");
    }
}

/// Function to create a block with transactions and event information
/// This is a wrapper around Block::new for backward compatibility
fn create_block(current_block_id: u64, parent_hash: &str, transactions: &[Transaction], 
                network_name: &str, block_ver: &str, inception_year: u16, event: &BlockEvent) -> Result<(String, String)> {
    let block = Block::new(current_block_id, parent_hash, transactions, network_name, block_ver, inception_year, event)?;
    
    // Print block information
    block.print();
    
    // Return hash and state root
    Ok((block.hash, block.state_root))
}

// This duplicate function has been removed to fix compilation errors

/// Run the price monitor in a separate thread
fn run_price_monitor(amm_pool: Arc<Mutex<AmmPool>>, stop_signal: Arc<Mutex<bool>>) -> Result<()> {
    // Path to the price data file
    let price_file_path = "price_data.txt";
    
    // Start the price monitor in a separate process with release build for better performance
    let status = std::process::Command::new("cmd")
        .args(["/c", "start", "cmd", "/k", "cargo", "run", "--release", "--bin", "price_monitor"])
        .spawn()
        .map_err(|e| BlockchainError::System(format!("Failed to start price monitor: {}", e)))?;
    
    info!("Started price monitor in a separate terminal window.");
    
    // Spawn a thread to update the price data file
    thread::spawn(move || {
        loop {
            // Check if we should stop
            if *stop_signal.lock().unwrap() {
                break;
            }
            
            // Get the current price
            let current_price = {
                let pool = amm_pool.lock().unwrap();
                pool.get_zux_price()
            };
            
            // Write the current price to the file
            if let Ok(mut file) = std::fs::File::create(price_file_path) {
                if let Err(e) = writeln!(file, "{}", current_price) {
                    error!("Failed to write price data: {}", e);
                }
            }
            
            // Sleep for the update interval
            thread::sleep(Duration::from_millis(PRICE_UPDATE_INTERVAL_MS));
        }
        
        // Clean up the price data file when done
        if let Err(e) = std::fs::remove_file(price_file_path) {
            error!("Failed to remove price data file: {}", e);
        }
        
        Ok::<(), BlockchainError>(())
    });
    
    Ok(())
}

/// Run the blockchain simulation
fn run_simulation() -> Result<()> {
    // Initialize logging
    SimpleLogger::new().with_level(log::LevelFilter::Info).init()
        .map_err(|e| BlockchainError::System(format!("Failed to initialize logger: {}", e)))?;
    
    info!("Initializing ZUX Blockchain simulation...");
    
    // Blockchain configuration
    let mut current_block_id_counter: u64 = 0;
    let mut parent_hash_string: String = "0".repeat(64);
    let network_name: &str = "ZUX-Testnet";
    let block_ver: &str = "1.0.0.0.0";
    let inception_year: u16 = 2025;
    
    // Initialize the unique code generator for wallet addresses
    let mut code_generator = UniqueCodeGenerator::new()?;
    info!("Initialized unique wallet address generator to prevent address collisions.");

    info!("Starting ZUX Blockchain simulation...");
    info!("This simulation will create exactly 3002 blocks initially:");
    info!("  - 1 Genesis block");
    info!("  - 1 System Wallet creation block");
    info!("  - 1 AMM Pool initialization block");
    info!("  - 1000 blocks for wallet creation events");
    info!("  - 2000 blocks for token crediting events (500 USDZ and 100 ZUX per wallet)");
    info!("After block 3002, the simulation will continue indefinitely with random swap transactions.");
    
    info!("Block generation is event-triggered based on computation completion");
    
    // Create the genesis block
    current_block_id_counter += 1;
    let genesis_event = BlockEvent::Genesis;
    let (genesis_hash, _) = create_block(
        current_block_id_counter,
        &parent_hash_string,
        &[], // No transactions in genesis block
        network_name,
        block_ver,
        inception_year,
        &genesis_event
    )?;
    parent_hash_string = genesis_hash;
    info!("Genesis block created successfully! Block ID: {}", current_block_id_counter);

    // Create the System Wallet first
    let system_wallet = create_system_wallet(&mut code_generator)?;
    
    // Create a block for the System Wallet creation
    current_block_id_counter += 1;
    let system_wallet_event = BlockEvent::WalletCreation(system_wallet.address.clone());
    let (system_wallet_hash, _) = create_block(
        current_block_id_counter,
        &parent_hash_string,
        &[], // No transactions for wallet creation
        network_name,
        block_ver,
        inception_year,
        &system_wallet_event
    )?;
    parent_hash_string = system_wallet_hash;
    info!("System Wallet created successfully! Block ID: {}", current_block_id_counter);
    info!("System Wallet Address: {}", system_wallet.address);
    info!("System Wallet Balance: {} ZUX, {} USDZ", 
         system_wallet.get_balance("ZUX"),
         system_wallet.get_balance("USDZ"));
         
    // Create the AMM Pool with minimal initial liquidity (will be funded later)
    let fee_percent: f64 = 0.3; // 0.3% fee
    
    // Create AMM Pool with minimal values
    let amm_pool = AmmPool::new(1.0, 1.0, fee_percent);
    
    // Create a block for the AMM Pool creation
    current_block_id_counter += 1;
    let amm_pool_event = BlockEvent::AmmPoolCreation(AMM_POOL_ADDRESS.to_string());
    let (amm_pool_hash, _) = create_block(
        current_block_id_counter,
        &parent_hash_string,
        &[], // No transactions for AMM pool creation
        network_name,
        block_ver,
        inception_year,
        &amm_pool_event
    )?;
    parent_hash_string = amm_pool_hash;
    info!("AMM Pool created successfully! Block ID: {}", current_block_id_counter);
    info!("AMM Pool Address: {}", AMM_POOL_ADDRESS);
    info!("Initial Liquidity: {} ZUX, {} USDZ (will be funded later)", 1, 1);
    
    // Wrap the AMM pool in an Arc<Mutex> for thread-safe access
    let amm_pool = Arc::new(Mutex::new(amm_pool));
    
    // Create a stop signal for the price monitor thread
    let stop_signal = Arc::new(Mutex::new(false));

    // Create 1000 wallets with individual blocks for each wallet creation
    info!("Creating 1000 wallets...");
    let mut wallets = HashMap::new();
    
    // No file operations - everything stays in memory
    info!("Creating 1000 wallets in memory...");
    
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| BlockchainError::Time(format!("Time error: {}", e)))?            
        .as_secs();
    
    info!("Generated at: {} (UNIX Epoch Seconds)", timestamp);
    info!("Each wallet will be credited with: 100 ZUX and 500 USDZ");
    
    // Create 1000 wallets
    for i in 1..=1000 {
        // Create a wallet without initial balance
        let wallet = create_wallet_without_balance(&mut code_generator)?;
        
        // Create a block for this wallet creation
        current_block_id_counter += 1;
        let wallet_event = BlockEvent::WalletCreation(wallet.address.clone());
        let (new_block_hash, _) = create_block(
            current_block_id_counter,
            &parent_hash_string,
            &[], // No transactions for wallet creation
            network_name,
            block_ver,
            inception_year,
            &wallet_event
        )?;
        parent_hash_string = new_block_hash;
        
        // Add the wallet to our collection
        wallets.insert(wallet.address.clone(), wallet);
        
        // Print progress every 100 wallets
        if i % 100 == 0 || i == 1 {
            info!("Created {} wallets so far...", i);
        }
    }
    
    // Add the system wallet to the wallets map
    wallets.insert(system_wallet.address.clone(), system_wallet);
    
    info!("\nAll wallet creation blocks have been generated.");
    info!("Current block count: {}", current_block_id_counter);
    info!("Now crediting each wallet with initial balance from System Wallet...");
    
    // Credit each wallet with ZUX and USDZ tokens
    let wallet_addresses: Vec<String> = wallets.keys()
        .filter(|&addr| addr != SYSTEM_WALLET_ADDRESS) // Exclude the system wallet
        .cloned()
        .collect();
    
    // Define credit amounts for each currency
    let zux_credit_amount: f64 = 100.0; // 100 ZUX per wallet
    let usdz_credit_amount: f64 = 500.0; // 500 USDZ per wallet
    
    // First, clone the system wallet so we can use it for transactions without borrowing from wallets map
    let system_wallet_clone = wallets.get(SYSTEM_WALLET_ADDRESS)
        .ok_or_else(|| BlockchainError::Wallet("System wallet not found".to_string()))?
        .clone();
    
    // Now create transactions and blocks
    for (i, address) in wallet_addresses.iter().enumerate() {
        // Create ZUX transaction
        let zux_tx = create_transaction(&system_wallet_clone, address, zux_credit_amount, "ZUX", &wallets)?;
        
        // Create a block for this ZUX transaction
        current_block_id_counter += 1;
        let (new_block_hash, _) = create_block(
            current_block_id_counter,
            &parent_hash_string,
            &[zux_tx], // Include the transaction
            network_name,
            block_ver,
            inception_year,
            &BlockEvent::TokenCredit(address.clone(), "ZUX".to_string(), zux_credit_amount)
        )?;
        parent_hash_string = new_block_hash;
        
        // Print progress every 100 transactions
        if (i + 1) % 100 == 0 || i == 0 {
            info!("Processed ZUX credits for {} wallets so far...", i + 1);
        }
        
        // Create USDZ transaction
        let usdz_tx = create_transaction(&system_wallet_clone, address, usdz_credit_amount, "USDZ", &wallets)?;
        
        // Create a block for this USDZ transaction
        current_block_id_counter += 1;
        let (new_block_hash, _) = create_block(
            current_block_id_counter,
            &parent_hash_string,
            &[usdz_tx], // Include the transaction
            network_name,
            block_ver,
            inception_year,
            &BlockEvent::TokenCredit(address.clone(), "USDZ".to_string(), usdz_credit_amount)
        )?;
        parent_hash_string = new_block_hash;
        
        // Print progress every 100 transactions
        if (i + 1) % 100 == 0 || i == 0 {
            info!("Processed USDZ credits for {} wallets so far...", i + 1);
        }
        
        // Update wallet balances
        if let Some(wallet) = wallets.get_mut(address) {
            wallet.add_balance("ZUX", zux_credit_amount)?;
            wallet.add_balance("USDZ", usdz_credit_amount)?;
        }
    }
    
    // Update the System Wallet balances to reflect the transfers
    if let Some(system_wallet) = wallets.get_mut(SYSTEM_WALLET_ADDRESS) {
        // Calculate total tokens transferred
        let total_zux_transferred = zux_credit_amount * wallet_addresses.len() as f64;
        let total_usdz_transferred = usdz_credit_amount * wallet_addresses.len() as f64;
        
        // Deduct from System Wallet balances
        system_wallet.subtract_balance("ZUX", total_zux_transferred)?;
        system_wallet.subtract_balance("USDZ", total_usdz_transferred)?;
        
        info!("\nSystem Wallet transferred a total of {} ZUX and {} USDZ to {} wallets.", 
             total_zux_transferred, total_usdz_transferred, wallet_addresses.len());
        info!("System Wallet remaining balance: {} ZUX, {} USDZ", 
             system_wallet.get_balance("ZUX"),
             system_wallet.get_balance("USDZ"));
    }
    
    // Transfer all remaining balance from system wallet to AMM pool
    info!("\nTransferring all remaining balance from System Wallet to AMM Pool...");
    
    // First, let's create a clone of the system wallet to avoid borrow issues
    let system_wallet_clone = wallets.get(SYSTEM_WALLET_ADDRESS)
        .ok_or_else(|| BlockchainError::Wallet("System wallet not found".to_string()))?
        .clone(); // Clone the wallet to avoid borrow issues
    
    // Get the remaining balances
    let remaining_zux = system_wallet_clone.get_balance("ZUX");
    let remaining_usdz = system_wallet_clone.get_balance("USDZ");
    
    // Adjust the balances to create a specific price point (0.01 USDZ per ZUX)
    // But use only a microscopic fraction of the available liquidity for extreme volatility
    let target_price = 0.01;
    let liquidity_fraction = 0.0001; // Use only 0.01% of available liquidity for ultra-extreme volatility
    let adjusted_zux = remaining_zux * liquidity_fraction; // Use only 0.01% of ZUX
    let adjusted_usdz = adjusted_zux * target_price; // Set USDZ to create the target price
    
    // Create transactions using the cloned wallet
    let zux_tx = create_transaction(&system_wallet_clone, AMM_POOL_ADDRESS, adjusted_zux, "ZUX", &wallets)?;
    let usdz_tx = create_transaction(&system_wallet_clone, AMM_POOL_ADDRESS, adjusted_usdz, "USDZ", &wallets)?;
    
    // Now update the actual system wallet balances
    {
        let system_wallet = wallets.get_mut(SYSTEM_WALLET_ADDRESS)
            .ok_or_else(|| BlockchainError::Wallet("System wallet not found".to_string()))?;
        
        system_wallet.subtract_balance("ZUX", adjusted_zux)?;
        system_wallet.subtract_balance("USDZ", adjusted_usdz)?;
    }
    
    // Create blocks for these transactions
    current_block_id_counter += 1;
    let (new_block_hash, _) = create_block(
        current_block_id_counter,
        &parent_hash_string,
        &[zux_tx], // Include the ZUX transaction
        network_name,
        block_ver,
        inception_year,
        &BlockEvent::TokenCredit(AMM_POOL_ADDRESS.to_string(), "ZUX".to_string(), adjusted_zux)
    )?;
    parent_hash_string = new_block_hash;
    
    current_block_id_counter += 1;
    let (new_block_hash, _) = create_block(
        current_block_id_counter,
        &parent_hash_string,
        &[usdz_tx], // Include the USDZ transaction
        network_name,
        block_ver,
        inception_year,
        &BlockEvent::TokenCredit(AMM_POOL_ADDRESS.to_string(), "USDZ".to_string(), adjusted_usdz)
    )?;
    parent_hash_string = new_block_hash;
    
    // Update AMM pool with the transferred liquidity
    {
        let mut amm_pool_lock = amm_pool.lock().unwrap();
        *amm_pool_lock = AmmPool::new(adjusted_zux, adjusted_usdz, 0.3); // 0.3% fee
    }
    
    let current_price = amm_pool.lock().unwrap().get_zux_price();
    
    info!("Transferred {} ZUX and {} USDZ from System Wallet to AMM Pool", adjusted_zux, adjusted_usdz);
    info!("AMM Pool now has {} ZUX and {} USDZ", adjusted_zux, adjusted_usdz);
    info!("Initial ZUX Price: {:.6} USDZ per ZUX", current_price);
    
    info!("\nInitial blockchain setup completed!");
    info!("Total blocks created so far: {}", current_block_id_counter);
    info!("  - 1 Genesis block");
    info!("  - 1 System Wallet creation block");
    info!("  - 1 AMM Pool initialization block");
    info!("  - 1000 Wallet Creation blocks");
    info!("  - 2000 Token Credit blocks (1000 for ZUX and 1000 for USDZ)");
    info!("  - 2 System to AMM Pool transfer blocks");
    info!("\nAll wallets have been created and credited with:");
    info!("  - {} ZUX tokens from System Wallet", zux_credit_amount);
    info!("  - {} USDZ tokens from System Wallet", usdz_credit_amount);
    info!("\nTotal ZUX in circulation: 1,000,000,000 (preserved as required)");
    
    info!("\nAll wallet addresses are guaranteed to be unique using the base-62 encoding system.");
    
    // Start the price monitor in a separate thread
    info!("\nStarting ZUX/USDZ price monitor in a separate terminal...");
    run_price_monitor(Arc::clone(&amm_pool), Arc::clone(&stop_signal))?;
    
    // Initialize trading strategies for all wallets
    let initial_price = amm_pool.lock().unwrap().get_zux_price();
    info!("\nInitializing trading strategies for all wallets with initial price: {:.6} USDZ", initial_price);
    
    for (_, wallet) in wallets.iter_mut() {
        if wallet.address != SYSTEM_WALLET_ADDRESS {
            wallet.initialize_trading_strategy(initial_price);
        }
    }
    
    // Now start the transaction simulation after block 3002
    info!("\nStarting transaction simulation after block 3002...");
    info!("Will simulate 10000 intelligent transactions with price-aware trading strategies.");
    
    // Clone the AMM pool for the simulation
    let amm_pool_clone = Arc::clone(&amm_pool);
    
    // Track the number of transactions
    let mut swap_count = 0;
    let total_transactions = 10000;
    
    // Track wallet performance
    let mut initial_balances: HashMap<String, (f64, f64)> = HashMap::new();
    
    // Track wallet participation statistics
    let mut total_zux_traded = 0.0;
    let mut total_usdz_traded = 0.0;
    let mut max_trades_per_wallet = 0;
    let mut min_trades_per_wallet = total_transactions;
    let mut wallet_trade_counts: HashMap<String, usize> = HashMap::new();
    
    // Record initial balances for performance tracking
    for (addr, wallet) in wallets.iter() {
        if addr != SYSTEM_WALLET_ADDRESS {
            initial_balances.insert(
                addr.clone(), 
                (wallet.get_balance("ZUX"), wallet.get_balance("USDZ"))
            );
        }
    }
    
    while swap_count < total_transactions {
        // Create an intelligent swap based on trading strategy
        let (wallet_address, is_zux_to_usd, input_amount, output_amount, transaction) = 
            create_intelligent_swap(&mut wallets, &mut amm_pool_clone.lock().unwrap())?;
        
        // Create a block for this swap
        current_block_id_counter += 1;
        let swap_event = BlockEvent::Swap(
            wallet_address.clone(), 
            is_zux_to_usd, 
            input_amount, 
            output_amount
        );
        
        let (new_block_hash, _) = create_block(
            current_block_id_counter,
            &parent_hash_string,
            &[transaction], // Include the swap transaction
            network_name,
            block_ver,
            inception_year,
            &swap_event
        )?;
        parent_hash_string = new_block_hash;
        
        // Track wallet participation
        *wallet_trade_counts.entry(wallet_address.clone()).or_insert(0) += 1;
        
        // Track trading volume
        if is_zux_to_usd {
            total_zux_traded += input_amount;
            total_usdz_traded += output_amount;
        } else {
            total_usdz_traded += input_amount;
            total_zux_traded += output_amount;
        }
        
        // Increment swap count
        swap_count += 1;
        
        // Print progress every 250 transactions to reduce log clutter with increased transaction count
        if swap_count % 250 == 0 {
            let current_price = amm_pool_clone.lock().unwrap().get_zux_price();
            info!("Processed {} intelligent swaps ({:.1}% complete). Current ZUX price: {:.6} USDZ", 
                  swap_count, (swap_count as f64 / total_transactions as f64) * 100.0, current_price);
        }
        
        // Add a minimal delay to avoid overwhelming the system while allowing more transactions
        thread::sleep(Duration::from_millis(5));
    }
    
    // Verify total ZUX in circulation is still 1B
    let mut total_zux = 0.0;
    for (_, wallet) in wallets.iter() {
        total_zux += wallet.get_balance("ZUX") as f64;
    }
    
    // Add ZUX in AMM pool
    total_zux += amm_pool_clone.lock().unwrap().zux_reserve as f64;
    
    // Now this code is reachable since we have a bounded loop
    *stop_signal.lock().unwrap() = true;
    
    info!("\nBlockchain simulation completed with {} transactions!", swap_count);
    info!("  - {} intelligent swaps with enhanced trading strategies", swap_count);
    info!("  - All wallets actively participated with increased trading frequency");
    info!("\nTotal ZUX in circulation: {:.2} (should be 1,000,000,000)", total_zux);
    
    // Final AMM pool status
    let final_amm_pool = amm_pool_clone.lock().unwrap();
    info!("\nFinal AMM Pool Status:");
    info!("  - ZUX Reserve: {:.2}", final_amm_pool.zux_reserve);
    info!("  - USDZ Reserve: {:.2}", final_amm_pool.usd_reserve);
    info!("  - ZUX Price: {:.6} USDZ per ZUX", final_amm_pool.get_zux_price());
    
    // Calculate and display wallet performance
    info!("\nWallet Trading Performance:");
    
    // Track overall performance metrics
    let mut total_wallets = 0;
    let mut profitable_wallets = 0;
    let mut best_performer = (String::new(), 0.0);
    let mut worst_performer = (String::new(), 0.0);
    
    // Calculate performance for each wallet
    for (addr, wallet) in wallets.iter() {
        if addr != SYSTEM_WALLET_ADDRESS && initial_balances.contains_key(addr) {
            let (initial_zux, initial_usdz) = initial_balances.get(addr).unwrap();
            let final_zux = wallet.get_balance("ZUX");
            let final_usdz = wallet.get_balance("USDZ");
            
            // Calculate total value in USDZ (initial and final)
            let current_price = final_amm_pool.get_zux_price();
            let initial_value = initial_zux * current_price + initial_usdz;
            let final_value = final_zux * current_price + final_usdz;
            
            // Calculate performance percentage
            let performance_pct = ((final_value / initial_value) - 1.0) * 100.0;
            
            // Update overall metrics
            total_wallets += 1;
            if performance_pct > 0.0 {
                profitable_wallets += 1;
            }
            
            // Update best/worst performers
            if best_performer.0.is_empty() || performance_pct > best_performer.1 {
                best_performer = (addr.clone(), performance_pct);
            }
            
            if worst_performer.0.is_empty() || performance_pct < worst_performer.1 {
                worst_performer = (addr.clone(), performance_pct);
            }
        }
    }
    
    // Calculate wallet participation statistics
    for (_, count) in wallet_trade_counts.iter() {
        max_trades_per_wallet = max_trades_per_wallet.max(*count);
        min_trades_per_wallet = min_trades_per_wallet.min(*count);
    }
    
    let avg_trades_per_wallet = if !wallet_trade_counts.is_empty() {
        swap_count as f64 / wallet_trade_counts.len() as f64
    } else {
        0.0
    };
    
    let participation_rate = wallet_trade_counts.len() as f64 / total_wallets as f64 * 100.0;
    
    // Display overall performance statistics
    info!("  - Profitable wallets: {} out of {} ({:.1}%)", 
          profitable_wallets, total_wallets, 
          (profitable_wallets as f64 / total_wallets as f64) * 100.0);
    
    info!("  - Best performing wallet: {} with {:.2}% gain", 
          best_performer.0, best_performer.1);
    
    info!("  - Worst performing wallet: {} with {:.2}% change", 
          worst_performer.0, worst_performer.1);
    
    // Display participation statistics
    info!("\nWallet Participation Statistics:");
    info!("  - Wallets that participated in trading: {} out of {} ({:.1}%)", 
          wallet_trade_counts.len(), total_wallets, participation_rate);
    info!("  - Average trades per wallet: {:.1}", avg_trades_per_wallet);
    info!("  - Maximum trades by a single wallet: {}", max_trades_per_wallet);
    info!("  - Minimum trades by a participating wallet: {}", min_trades_per_wallet);
    info!("  - Total ZUX traded: {:.2}", total_zux_traded);
    info!("  - Total USDZ traded: {:.2}", total_usdz_traded);
    
    // Display some individual wallet performances
    info!("\nSample of Individual Wallet Performances:");
    
    // Create a vector of wallet performances for sorting
    let mut wallet_performances: Vec<(String, f64, f64, f64, f64, f64)> = Vec::new();
    
    for (addr, wallet) in wallets.iter() {
        if addr != SYSTEM_WALLET_ADDRESS && initial_balances.contains_key(addr) {
            let (initial_zux, initial_usdz) = initial_balances.get(addr).unwrap();
            let final_zux = wallet.get_balance("ZUX");
            let final_usdz = wallet.get_balance("USDZ");
            
            // Calculate total value in USDZ (initial and final)
            let current_price = final_amm_pool.get_zux_price();
            let initial_value = initial_zux * current_price + initial_usdz;
            let final_value = final_zux * current_price + final_usdz;
            
            // Calculate performance percentage
            let performance_pct = ((final_value / initial_value) - 1.0) * 100.0;
            
            wallet_performances.push((addr.clone(), performance_pct, *initial_zux, final_zux, *initial_usdz, final_usdz));
        }
    }
    
    // Sort by performance (descending)
    wallet_performances.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    
    // Display top 5 performers
    info!("  Top 5 Performing Wallets:");
    for (i, (addr, performance, initial_zux, final_zux, initial_usdz, final_usdz)) in wallet_performances.iter().take(5).enumerate() {
        // Calculate changes
        let zux_change = final_zux - initial_zux;
        let usdz_change = final_usdz - initial_usdz;
        
        info!("  #{} Wallet {} (Performance: +{:.2}%):", i+1, addr, performance);
        info!("    - ZUX: {:.2} → {:.2} ({:+.2})", initial_zux, final_zux, zux_change);
        info!("    - USDZ: {:.2} → {:.2} ({:+.2})", initial_usdz, final_usdz, usdz_change);
    }
    
    // Display bottom 5 performers
    info!("\n  Bottom 5 Performing Wallets:");
    let len = wallet_performances.len();
    for (i, (addr, performance, initial_zux, final_zux, initial_usdz, final_usdz)) in wallet_performances.iter().rev().take(5).enumerate() {
        // Calculate changes
        let zux_change = final_zux - initial_zux;
        let usdz_change = final_usdz - initial_usdz;
        
        info!("  #{} Wallet {} (Performance: {:.2}%):", len-i, addr, performance);
        info!("    - ZUX: {:.2} → {:.2} ({:+.2})", initial_zux, final_zux, zux_change);
        info!("    - USDZ: {:.2} → {:.2} ({:+.2})", initial_usdz, final_usdz, usdz_change);
    }
    
    Ok(())
}

fn main() {
    // Run the simulation and handle any errors
    if let Err(e) = run_simulation() {
        eprintln!("Error: {}", e);
        std::process::exit(1);
    }
}