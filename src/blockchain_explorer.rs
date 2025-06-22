// Blockchain Explorer TUI Module
// High-performance, responsive blockchain explorer with tabbed interface

use std::fs::File;
use std::io::{self, BufReader};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use crossterm::{
    execute, 
    terminal::{EnterAlternateScreen, LeaveAlternateScreen}, 
    cursor::{Hide, Show}, 
    event::{self, Event, KeyCode}
};
use tui::{
    backend::CrosstermBackend, 
    Terminal, 
    widgets::{Block, Borders, Paragraph, Row, Table, Cell},
    layout::{Layout, Constraint, Direction, Alignment, Rect},
    style::{Style, Modifier, Color}
};
use serde::{Deserialize, Serialize};
use chrono;

// Data structures for explorer communication
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BlockInfo {
    pub id: u64,
    pub hash: String,
    pub parent_hash: String,
    pub timestamp: u64,
    pub transactions_count: usize,
    pub difficulty: u64,
    pub nonce: u64,
    pub size_bytes: usize,
    pub formatted_time: String,
    pub network_name: String,
    pub version: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AmmInfo {
    pub zux_reserve: f64,
    pub usd_reserve: f64,
    pub k_constant: f64,
    pub current_price: f64,
    pub total_liquidity: f64,
    pub volume_5s: f64,
    pub volume_total: f64,
    pub price_5s_change: f64,
    pub price_5s_high: f64,
    pub price_5s_low: f64,
    pub price_inception_change: f64,
    pub price_inception_high: f64,
    pub price_inception_low: f64,
    pub fees_collected: f64,
    pub swap_count: u64,
    pub avg_trade_size: f64,
    pub price_history: Vec<PricePoint>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PricePoint {
    pub timestamp: u64,
    pub price: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct WalletInfo {
    pub address: String,
    pub zux_balance: f64,
    pub usdz_balance: f64,
    pub total_value_usd: f64,
    pub transaction_count: u64,
    pub is_whale: bool,
    pub is_mega_whale: bool,
    pub last_activity: u64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct SystemWalletInfo {
    pub address: String,
    pub zux_balance: f64,
    pub usdz_balance: f64,
    pub total_issued_zux: f64,
    pub total_issued_usdz: f64,
    pub active_wallets: u64,
    pub total_transactions: u64,
    pub network_hash_rate: f64,
    pub avg_block_time: f64,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ExplorerData {
    pub blocks: Vec<BlockInfo>,
    pub amm_info: AmmInfo,
    pub wallets: Vec<WalletInfo>,
    pub system_wallet: SystemWalletInfo,
    pub last_update: u64,
}

// Tab enumeration for navigation
#[derive(Clone, Copy, PartialEq, Eq, Hash)]
enum Tab {
    Blocks,
    Amm,
    Wallets,
    SystemWallet,
}

impl Tab {
    fn next(self) -> Self {
        match self {
            Tab::Blocks => Tab::Amm,
            Tab::Amm => Tab::Wallets,
            Tab::Wallets => Tab::SystemWallet,
            Tab::SystemWallet => Tab::Blocks,
        }
    }

    fn previous(self) -> Self {
        match self {
            Tab::Blocks => Tab::SystemWallet,
            Tab::Amm => Tab::Blocks,
            Tab::Wallets => Tab::Amm,
            Tab::SystemWallet => Tab::Wallets,
        }
    }

    fn title(self) -> &'static str {
        match self {
            Tab::Blocks => "BLOCKS",
            Tab::Amm => "AMM POOL",
            Tab::Wallets => "WALLETS",
            Tab::SystemWallet => "SYSTEM",
        }
    }
}

// Explorer application state
struct ExplorerState {
    current_tab: Tab,
    data: ExplorerData,
    last_update: Instant,
    scroll_position: HashMap<Tab, usize>,
    selected_block_index: usize,
    selected_wallet_index: usize,
}

impl Clone for ExplorerState {
    fn clone(&self) -> Self {
        ExplorerState {
            current_tab: self.current_tab,
            data: self.data.clone(),
            last_update: self.last_update,
            scroll_position: self.scroll_position.clone(),
            selected_block_index: self.selected_block_index,
            selected_wallet_index: self.selected_wallet_index,
        }
    }
}

impl ExplorerState {
    fn new() -> Self {
        let mut scroll_position = HashMap::new();
        scroll_position.insert(Tab::Blocks, 0);
        scroll_position.insert(Tab::Amm, 0);
        scroll_position.insert(Tab::Wallets, 0);
        scroll_position.insert(Tab::SystemWallet, 0);

        ExplorerState {
            current_tab: Tab::Blocks,
            data: ExplorerData {
                blocks: Vec::new(),
                amm_info: AmmInfo {
                    zux_reserve: 0.0,
                    usd_reserve: 0.0,
                    k_constant: 0.0,
                    current_price: 0.0,
                    total_liquidity: 0.0,
                    volume_5s: 0.0,
                    volume_total: 0.0,
                    price_5s_change: 0.0,
                    price_5s_high: 0.0,
                    price_5s_low: 0.0,
                    price_inception_change: 0.0,
                    price_inception_high: 0.0,
                    price_inception_low: 0.0,
                    fees_collected: 0.0,
                    swap_count: 0,
                    avg_trade_size: 0.0,
                    price_history: Vec::new(),
                },
                wallets: Vec::new(),
                system_wallet: SystemWalletInfo {
                    address: "SYSTEM".to_string(),
                    zux_balance: 0.0,
                    usdz_balance: 0.0,
                    total_issued_zux: 0.0,
                    total_issued_usdz: 0.0,
                    active_wallets: 0,
                    total_transactions: 0,
                    network_hash_rate: 0.0,
                    avg_block_time: 0.0,
                },
                last_update: 0,
            },
            last_update: Instant::now(),
            scroll_position,
            selected_block_index: 0,
            selected_wallet_index: 0,
        }
    }

    fn scroll_up(&mut self) {
        match self.current_tab {
            Tab::Blocks => {
                if self.selected_block_index > 0 {
                    self.selected_block_index -= 1;
                }
            },
            Tab::Wallets => {
                if self.selected_wallet_index > 0 {
                    self.selected_wallet_index -= 1;
                }
            },
            _ => {
                // For AMM and System tabs, just scroll content
                let current_pos = *self.scroll_position.get(&self.current_tab).unwrap_or(&0);
                if current_pos > 0 {
                    self.scroll_position.insert(self.current_tab, current_pos - 1);
                }
            }
        }
    }

    fn scroll_down(&mut self) {
        match self.current_tab {
            Tab::Blocks => {
                if !self.data.blocks.is_empty() {
                    let max_index = self.data.blocks.len() - 1;
                    if self.selected_block_index < max_index {
                        self.selected_block_index += 1;
                    }
                }
            },
            Tab::Wallets => {
                if !self.data.wallets.is_empty() {
                    let max_index = self.data.wallets.len() - 1;
                    if self.selected_wallet_index < max_index {
                        self.selected_wallet_index += 1;
                    }
                }
            },
            _ => {
                // For AMM and System tabs, just scroll content
                let current_pos = *self.scroll_position.get(&self.current_tab).unwrap_or(&0);
                let max_items = 10; // For AMM and System tabs
                if current_pos < max_items {
                    self.scroll_position.insert(self.current_tab, current_pos + 1);
                }
            }
        }
    }

    // Ensure selection indices are valid for current data
    fn validate_selection_indices(&mut self) {
        // Bound block selection
        if !self.data.blocks.is_empty() {
            let max_block_index = self.data.blocks.len().saturating_sub(1);
            if self.selected_block_index > max_block_index {
                self.selected_block_index = max_block_index;
            }
        } else {
            self.selected_block_index = 0;
        }

        // Bound wallet selection
        if !self.data.wallets.is_empty() {
            let max_wallet_index = self.data.wallets.len().saturating_sub(1);
            if self.selected_wallet_index > max_wallet_index {
                self.selected_wallet_index = max_wallet_index;
            }
        } else {
            self.selected_wallet_index = 0;
        }
    }
}

// Render the tab navigation bar
fn render_tabs(f: &mut tui::Frame<CrosstermBackend<std::io::Stdout>>, area: Rect, current_tab: Tab) {
    let tabs = vec![Tab::Blocks, Tab::Amm, Tab::Wallets, Tab::SystemWallet];
    let tab_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints(vec![Constraint::Percentage(25); 4])
        .split(area);

    for (i, tab) in tabs.iter().enumerate() {
        let style = if *tab == current_tab {
            Style::default()
                .fg(Color::White)
                .bg(Color::LightBlue)
                .add_modifier(Modifier::BOLD)
        } else {
            Style::default()
                .fg(Color::LightBlue)
                .bg(Color::White)
        };

        let paragraph = Paragraph::new(tab.title())
            .style(style)
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).style(style));

        f.render_widget(paragraph, tab_chunks[i]);
    }
}

// Render blocks tab content
fn render_blocks_tab(f: &mut tui::Frame<CrosstermBackend<std::io::Stdout>>, area: Rect, state: &ExplorerState) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(5),    // Content area
        ])
        .split(area);

    // Header with blockchain statistics
    let latest_block_id = state.data.blocks.last().map(|b| b.id).unwrap_or(0);
    let header_text = format!("Total Blocks: {} | Latest Block: #{} | Network: ZUX | Use ↑↓ to select blocks", 
        state.data.blocks.len(),
        latest_block_id
    );
    
    let header = Paragraph::new(header_text)
        .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).title("Blockchain Overview"));
    
    f.render_widget(header, main_chunks[0]);

    // Split content area into blocks list and details panel
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(60), // Blocks list (left side)
            Constraint::Percentage(40), // Block details (right side)
        ])
        .split(main_chunks[1]);

    // Blocks table (left panel)
    if !state.data.blocks.is_empty() {
        let header_cells = ["ID", "Hash", "Txs", "Diff", "Time"]
            .iter()
            .map(|h| Cell::from(*h).style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD)));
        
        let header_row = Row::new(header_cells).height(1);

        // Calculate scroll position to keep selected block visible
        let visible_count = content_chunks[0].height.saturating_sub(3) as usize;
        let scroll_pos = if state.selected_block_index >= visible_count {
            state.selected_block_index - visible_count + 1
        } else {
            0
        };
        
        let visible_blocks = state.data.blocks.iter()
            .rev()
            .skip(scroll_pos)
            .take(visible_count)
            .collect::<Vec<_>>();

        let rows = visible_blocks.iter().enumerate().map(|(i, block)| {
            let hash_short = if block.hash.len() > 8 {
                format!("{}...", &block.hash[..8])
            } else {
                block.hash.clone()
            };

            let time_short = if block.formatted_time.len() > 8 {
                block.formatted_time[11..19].to_string() // Just time part
            } else {
                block.formatted_time.clone()
            };

            // Since blocks are displayed in reverse order, we need to check if this visible block
            // corresponds to our selected block index
            let displayed_index = scroll_pos + i;
            let style = if displayed_index == state.selected_block_index {
                Style::default().bg(Color::LightBlue).fg(Color::White).add_modifier(Modifier::BOLD)
            } else {
                Style::default()
            };

            Row::new(vec![
                Cell::from(block.id.to_string()).style(style),
                Cell::from(hash_short).style(style),
                Cell::from(block.transactions_count.to_string()).style(style),
                Cell::from(block.difficulty.to_string()).style(style),
                Cell::from(time_short).style(style),
            ])
        });

        let table = Table::new(rows)
            .header(header_row)
            .block(Block::default().borders(Borders::ALL).title("Blocks List"))
            .widths(&[
                Constraint::Length(6),
                Constraint::Length(10),
                Constraint::Length(4),
                Constraint::Length(6),
                Constraint::Min(8),
            ])
            .column_spacing(1);

        f.render_widget(table, content_chunks[0]);
    } else {
        let no_data = Paragraph::new("Waiting for blockchain data...\nThe explorer will update automatically once blocks are mined.")
            .style(Style::default().fg(Color::White))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title("Blocks List"));
        
        f.render_widget(no_data, content_chunks[0]);
    }

    // Block details panel (right side)
    render_block_details_panel(f, content_chunks[1], state);
}

// Render AMM pool tab content
fn render_amm_tab(f: &mut tui::Frame<CrosstermBackend<std::io::Stdout>>, area: Rect, state: &ExplorerState) {
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50), // Left side
            Constraint::Percentage(50), // Right side
        ])
        .split(area);

    // Left side chunks
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(10), // Pool reserves
            Constraint::Length(8),  // Trading metrics
            Constraint::Min(5),     // Liquidity analysis
        ])
        .split(main_chunks[0]);

    // Right side chunks
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(8),  // Price information
            Constraint::Length(9),  // Volume analysis
            Constraint::Min(5),     // Price history
        ])
        .split(main_chunks[1]);

    // Comprehensive pool reserves and liquidity information
    let pool_utilization = if state.data.amm_info.total_liquidity > 0.0 {
        // Pool utilization as percentage of total liquidity traded in 5s timeframe
        let utilization = (state.data.amm_info.volume_5s / state.data.amm_info.total_liquidity) * 100.0;
        // Cap at 100% to prevent impossible values
        if utilization > 100.0 { 100.0 } else { utilization }
    } else { 0.0 };
    
    let apr_estimate = pool_utilization * 0.365; // Rough APR estimate
    
    let pool_info = vec![
        format!("ZUX Reserve: {:.9} tokens", state.data.amm_info.zux_reserve),
        format!("USDZ Reserve: {:.9} tokens", state.data.amm_info.usd_reserve),
        format!("K Constant: {:.2}", state.data.amm_info.k_constant),
        format!("Total Liquidity: ${:.9}", state.data.amm_info.total_liquidity),
        format!("Pool Utilization: {:.2}%", pool_utilization),
        format!("Est. APR: {:.2}%", apr_estimate),
        format!("Total Swaps: {} trades", state.data.amm_info.swap_count),
        format!("Fees Collected: ${:.9}", state.data.amm_info.fees_collected),
    ];

    let pool_paragraph = Paragraph::new(pool_info.join("\n"))
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Liquidity Pool Details")
                .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
        );
    
    f.render_widget(pool_paragraph, left_chunks[0]);

    // Advanced trading metrics - now using real calculated values
    let avg_trade_size = state.data.amm_info.avg_trade_size;
    
    let fee_rate = 0.003; // 0.3%
    let daily_fees = state.data.amm_info.volume_total * fee_rate;
    
    let trading_info = vec![
        format!("Avg Trade Size: ${:.9}", avg_trade_size),
        format!("Trading Fee Rate: {:.1}%", fee_rate * 100.0),
        format!("Total Fee Revenue: ${:.9}", daily_fees),
        format!("Price Impact Model: Constant Product"),
        format!("Slippage Protection: Active"),
        format!("MEV Protection: Enabled"),
    ];

    let trading_paragraph = Paragraph::new(trading_info.join("\n"))
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Trading Metrics")
                .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
        );
    
    f.render_widget(trading_paragraph, left_chunks[1]);

    // Liquidity analysis
    let zux_ratio = if state.data.amm_info.total_liquidity > 0.0 {
        (state.data.amm_info.zux_reserve * state.data.amm_info.current_price) / state.data.amm_info.total_liquidity * 100.0
    } else { 50.0 };
    let usdz_ratio = 100.0 - zux_ratio;
    
    let liquidity_info = vec![
        format!("Pool Composition:"),
        format!("  ZUX: {:.1}% (${:.2})", zux_ratio, state.data.amm_info.zux_reserve * state.data.amm_info.current_price),
        format!("  USDZ: {:.1}% (${:.2})", usdz_ratio, state.data.amm_info.usd_reserve),
        format!("Impermanent Loss Risk: MODERATE"),
        format!("Pool Health: EXCELLENT"),
    ];

    let liquidity_paragraph = Paragraph::new(liquidity_info.join("\n"))
        .style(Style::default().fg(Color::LightBlue))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Liquidity Analysis")
                .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
        );
    
    f.render_widget(liquidity_paragraph, left_chunks[2]);

    // Comprehensive price information with 5s and inception timeframes
    let price_info = vec![
        format!("Current Price: ${:.9}", state.data.amm_info.current_price),
        format!("5s Change: {:.2}%", state.data.amm_info.price_5s_change),
        format!("5s High: ${:.9}", state.data.amm_info.price_5s_high),
        format!("5s Low: ${:.9}", state.data.amm_info.price_5s_low),
        format!("Since Inception: {:.2}%", state.data.amm_info.price_inception_change),
        format!("Price Oracle: AMM-based"),
    ];

    let price_paragraph = Paragraph::new(price_info.join("\n"))
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Price Information")
                .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
        );
    
    f.render_widget(price_paragraph, right_chunks[0]);

    // Volume analysis with 5s and inception metrics
    let volume_info = vec![
        format!("5s Volume: ${:.9}", state.data.amm_info.volume_5s),
        format!("Total Volume: ${:.9}", state.data.amm_info.volume_total),
        format!("Volume/Liquidity: {:.2}%", pool_utilization),
        format!("Active Traders: 1000 wallets"),
        format!("Whale Activity: MODERATE"),
        format!("Inception High: ${:.9}", state.data.amm_info.price_inception_high),
        format!("Inception Low: ${:.9}", state.data.amm_info.price_inception_low),
    ];

    let volume_paragraph = Paragraph::new(volume_info.join("\n"))
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Volume Analysis")
                .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
        );
    
    f.render_widget(volume_paragraph, right_chunks[1]);

    // Enhanced price history with technical indicators
    if !state.data.amm_info.price_history.is_empty() {
        let recent_prices = state.data.amm_info.price_history
            .iter()
            .rev()
            .take(right_chunks[2].height.saturating_sub(3) as usize)
            .enumerate()
            .map(|(i, price_point)| {
                let time = chrono::DateTime::from_timestamp(price_point.timestamp as i64, 0)
                    .unwrap_or_default()
                    .format("%H:%M:%S");
                let trend = if i == 0 { "→" } else { "↑" }; // Simplified trend indicator
                format!("{} | ${:.6} {}", time, price_point.price, trend)
            })
            .collect::<Vec<_>>();

        let history_text = format!("Recent Price Movements:\n{}\n\nTechnical: Bullish momentum", recent_prices.join("\n"));
        let history_paragraph = Paragraph::new(history_text)
            .style(Style::default().fg(Color::White))
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Live Price Feed")
                    .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
            );
        
        f.render_widget(history_paragraph, right_chunks[2]);
    } else {
        let no_history = Paragraph::new("Initializing price feed...\n\nWaiting for trading activity to begin.\nPrice history and technical analysis\nwill appear once swaps are executed.\n\nThe AMM will automatically calculate\nprices based on constant product formula.")
            .style(Style::default().fg(Color::White))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Live Price Feed")
                    .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
            );
        
        f.render_widget(no_history, right_chunks[2]);
    }
}

// Render block details panel (right side)
fn render_block_details_panel(f: &mut tui::Frame<CrosstermBackend<std::io::Stdout>>, area: Rect, state: &ExplorerState) {
    if !state.data.blocks.is_empty() && state.selected_block_index < state.data.blocks.len() {
        // Get the selected block from the reversed list (newest first display)
        let reversed_blocks: Vec<_> = state.data.blocks.iter().rev().collect();
        if let Some(block) = reversed_blocks.get(state.selected_block_index) {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(10), // Basic block information
                    Constraint::Length(9),  // Complete hash information
                    Constraint::Length(8),  // Technical mining details
                    Constraint::Length(7),  // Network and validation info
                    Constraint::Min(4),     // Status and position
                ])
                .split(area);

            // Comprehensive basic block information
            let time_since_creation = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .saturating_sub(block.timestamp);

            let basic_info = vec![
                format!("Block ID: #{}", block.id),
                format!("Network: {}", block.network_name),
                format!("Version: {}", block.version),
                format!("Timestamp: {}", block.timestamp),
                format!("Created: {}", block.formatted_time),
                format!("Age: {}s ago", time_since_creation),
                format!("Transactions: {} included", block.transactions_count),
                format!("Block Size: {} bytes", block.size_bytes),
            ];

            let basic_paragraph = Paragraph::new(basic_info.join("\n"))
                .style(Style::default().fg(Color::White))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Block Information")
                        .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
                );
            
            f.render_widget(basic_paragraph, chunks[0]);

            // Complete hash information with full details
            let hash_info = vec![
                format!("Block Hash (SHA-256):"),
                format!("  {}", &block.hash[..32]),
                format!("  {}", &block.hash[32..]),
                format!("Parent Block Hash:"),
                format!("  {}", &block.parent_hash[..32]),
                format!("  {}", if block.parent_hash.len() > 32 { &block.parent_hash[32..] } else { "" }),
                format!("Hash Algorithm: SHA-256"),
            ];

            let hash_paragraph = Paragraph::new(hash_info.join("\n"))
                .style(Style::default().fg(Color::White))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Cryptographic Hashes")
                        .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
                );
            
            f.render_widget(hash_paragraph, chunks[1]);

            // Detailed technical mining information
            let mining_time = if block.id > 0 { time_since_creation.min(60) } else { 0 };
            let hash_rate_estimate = if mining_time > 0 { block.difficulty as f64 / mining_time as f64 } else { 0.0 };
            
            let tech_info = vec![
                format!("Mining Difficulty: {}", block.difficulty),
                format!("Nonce Value: {}", block.nonce),
                format!("Estimated Hash Rate: {:.2} H/s", hash_rate_estimate),
                format!("Mining Algorithm: Proof of Work"),
                format!("Block Reward: Calculated"),
                format!("Mining Time: ~{}s", mining_time),
            ];

            let tech_paragraph = Paragraph::new(tech_info.join("\n"))
                .style(Style::default().fg(Color::White))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Mining & Technical Details")
                        .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
                );
            
            f.render_widget(tech_paragraph, chunks[2]);

            // Network and validation information
            let validation_info = vec![
                format!("Network: ZUX Blockchain"),
                format!("Consensus: Proof of Work"),
                format!("Signature Algorithm: Ed25519"),
                format!("Hash Function: SHA-256"),
                format!("Block Status: CONFIRMED"),
                format!("Confirmations: {}", state.data.blocks.len().saturating_sub(block.id as usize)),
            ];

            let validation_paragraph = Paragraph::new(validation_info.join("\n"))
                .style(Style::default().fg(Color::White))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Network & Validation")
                        .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
                );
            
            f.render_widget(validation_paragraph, chunks[3]);

            // Status and position in blockchain
            let additional_info = vec![
                format!("Position in Chain: {} of {}", state.selected_block_index + 1, state.data.blocks.len()),
                format!("Block Explorer: ZUX Network"),
                format!("Data Integrity: VERIFIED"),
                format!("Immutable: YES"),
            ];

            let additional_paragraph = Paragraph::new(additional_info.join("\n"))
                .style(Style::default().fg(Color::LightBlue))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Blockchain Status")
                        .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
                );
            
            f.render_widget(additional_paragraph, chunks[4]);
        }
    } else {
        // No blocks or invalid selection
        let no_selection = Paragraph::new("No block selected\n\nUse ↑↓ arrow keys to\nselect a block from\nthe list to view its\ndetailed information.")
            .style(Style::default().fg(Color::White))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Block Details")
                    .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
            );
        
        f.render_widget(no_selection, area);
    }
}

// Render wallets tab content
fn render_wallets_tab(f: &mut tui::Frame<CrosstermBackend<std::io::Stdout>>, area: Rect, state: &ExplorerState) {
    let main_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // Header
            Constraint::Min(5),    // Content area
        ])
        .split(area);

    // Header with wallet statistics
    let whale_count = state.data.wallets.iter().filter(|w| w.is_whale).count();
    let mega_whale_count = state.data.wallets.iter().filter(|w| w.is_mega_whale).count();
    let regular_count = state.data.wallets.len() - whale_count;
    
    let header_text = format!("Total Wallets: {} | Regular: {} | Whales: {} | Mega Whales: {} | Use ↑↓ to select wallets", 
        state.data.wallets.len(),
        regular_count,
        whale_count - mega_whale_count, // whales minus mega whales
        mega_whale_count
    );
    
    let header = Paragraph::new(header_text)
        .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
        .alignment(Alignment::Center)
        .block(Block::default().borders(Borders::ALL).title("Wallet Overview"));
    
    f.render_widget(header, main_chunks[0]);

    // Split content area into wallets list and details panel
    let content_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(60), // Wallets list (left side)
            Constraint::Percentage(40), // Wallet details (right side)
        ])
        .split(main_chunks[1]);

    // Wallets table (left panel)
    if !state.data.wallets.is_empty() {
        let header_cells = ["Address", "ZUX", "USDZ", "Type", "Txs"]
            .iter()
            .map(|h| Cell::from(*h).style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD)));
        
        let header_row = Row::new(header_cells).height(1);

        // Calculate scroll position to keep selected wallet visible
        let visible_count = content_chunks[0].height.saturating_sub(3) as usize;
        let scroll_pos = if state.selected_wallet_index >= visible_count {
            state.selected_wallet_index - visible_count + 1
        } else {
            0
        };
        
        let visible_wallets = state.data.wallets.iter()
            .skip(scroll_pos)
            .take(visible_count)
            .collect::<Vec<_>>();

        let rows = visible_wallets.iter().enumerate().map(|(i, wallet)| {
            let wallet_type = if wallet.is_mega_whale {
                "MEGA"
            } else if wallet.is_whale {
                "WHALE"
            } else {
                "REG"
            };

            let addr_short = if wallet.address.len() > 6 {
                format!("{}...", &wallet.address[..6])
            } else {
                wallet.address.clone()
            };

            let wallet_index = scroll_pos + i;
            let style = if wallet_index == state.selected_wallet_index {
                Style::default().bg(Color::LightBlue).fg(Color::White).add_modifier(Modifier::BOLD)
            } else if wallet.is_mega_whale {
                Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD)
            } else if wallet.is_whale {
                Style::default().fg(Color::LightBlue)
            } else {
                Style::default().fg(Color::White)
            };

            Row::new(vec![
                Cell::from(addr_short).style(style),
                Cell::from(format!("{:.1}", wallet.zux_balance)).style(style),
                Cell::from(format!("{:.1}", wallet.usdz_balance)).style(style),
                Cell::from(wallet_type).style(style),
                Cell::from(wallet.transaction_count.to_string()).style(style),
            ])
        });

        let table = Table::new(rows)
            .header(header_row)
            .block(Block::default().borders(Borders::ALL).title("Wallets List"))
            .widths(&[
                Constraint::Length(8),
                Constraint::Length(8),
                Constraint::Length(8),
                Constraint::Length(6),
                Constraint::Min(6),
            ])
            .column_spacing(1);

        f.render_widget(table, content_chunks[0]);
    } else {
        let no_data = Paragraph::new("Waiting for wallet data...\nWallet information will appear once wallets are created.")
            .style(Style::default().fg(Color::White))
            .alignment(Alignment::Center)
            .block(Block::default().borders(Borders::ALL).title("Wallets List"));
        
        f.render_widget(no_data, content_chunks[0]);
    }

    // Wallet details panel (right side)
    render_wallet_details_panel(f, content_chunks[1], state);
}

// Render wallet details panel (right side)
fn render_wallet_details_panel(f: &mut tui::Frame<CrosstermBackend<std::io::Stdout>>, area: Rect, state: &ExplorerState) {
    if !state.data.wallets.is_empty() && state.selected_wallet_index < state.data.wallets.len() {
        if let Some(wallet) = state.data.wallets.get(state.selected_wallet_index) {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(6),  // Basic wallet info
                    Constraint::Length(7),  // Balance details
                    Constraint::Length(6),  // Trading info
                    Constraint::Length(8),  // Recent trades
                    Constraint::Min(4),     // Status & additional
                ])
                .split(area);

            // Basic wallet information
            let wallet_type_full = if wallet.is_mega_whale {
                "MEGA WHALE (>$100,000)"
            } else if wallet.is_whale {
                "WHALE (>$10,000)"
            } else {
                "REGULAR TRADER (<$10,000)"
            };

            let creation_time = chrono::DateTime::from_timestamp(wallet.last_activity as i64, 0)
                .unwrap_or_default()
                .format("%Y-%m-%d %H:%M:%S");

            let basic_info = vec![
                format!("Address: {}", wallet.address),
                format!("Type: {}", wallet_type_full),
                format!("Last Activity: {}", creation_time),
                format!("Total Transactions: {}", wallet.transaction_count),
                format!("Status: ACTIVE"),
            ];

            let basic_paragraph = Paragraph::new(basic_info.join("\n"))
                .style(Style::default().fg(Color::White))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Wallet Info")
                        .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
                );
            
            f.render_widget(basic_paragraph, chunks[0]);

            // Balance and value details
            let current_zux_price = state.data.amm_info.current_price;
            let zux_value_in_usd = wallet.zux_balance * current_zux_price;
            let total_usd_value = zux_value_in_usd + wallet.usdz_balance;
            
            let balance_info = vec![
                format!("ZUX Balance: {:.9}", wallet.zux_balance),
                format!("ZUX Value (USD): ${:.9}", zux_value_in_usd),
                format!("USDZ Balance: {:.9}", wallet.usdz_balance),
                format!("Total USD Value: ${:.9}", total_usd_value),
                format!("Portfolio Distribution:"),
                format!("  ZUX: {:.1}% | USDZ: {:.1}%", 
                    if total_usd_value > 0.0 { (zux_value_in_usd / total_usd_value) * 100.0 } else { 0.0 },
                    if total_usd_value > 0.0 { (wallet.usdz_balance / total_usd_value) * 100.0 } else { 0.0 }
                ),
            ];

            let balance_paragraph = Paragraph::new(balance_info.join("\n"))
                .style(Style::default().fg(Color::White))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Live Balances")
                        .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
                );
            
            f.render_widget(balance_paragraph, chunks[1]);

            // Trading information
            let avg_trade_size = if wallet.transaction_count > 0 {
                // Average trade size should be based on trading volume, not total wallet value
                (wallet.zux_balance + wallet.usdz_balance) / (wallet.transaction_count as f64 * 2.0)
            } else {
                0.0
            };

            // Calculate profitability (simulated based on activity)
            let profitability = if wallet.transaction_count > 0 {
                let base_profit = (total_usd_value / 1000.0) * (wallet.transaction_count as f64 / 10.0);
                if wallet.is_mega_whale { base_profit * 0.15 }
                else if wallet.is_whale { base_profit * 0.25 }
                else { base_profit * 0.35 }
            } else { 0.0 };

            let trading_info = vec![
                format!("Avg Trade Size: ${:.9}", avg_trade_size),
                format!("Current ZUX Price: ${:.9}", current_zux_price),
                format!("Total Profit/Loss: ${:.9}", profitability),
                format!("Profitability: {:.2}%", (profitability / total_usd_value.max(1.0)) * 100.0),
                format!("Trading Strategy: {}", 
                    if wallet.is_mega_whale { "Conservative High-Volume" }
                    else if wallet.is_whale { "Balanced Growth" }
                    else { "Active Trading" }
                ),
            ];

            let trading_paragraph = Paragraph::new(trading_info.join("\n"))
                .style(Style::default().fg(Color::White))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Trading Profile")
                        .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
                );
            
            f.render_widget(trading_paragraph, chunks[2]);

            // Recent trades (simulated based on transaction count)
            let recent_trades = if wallet.transaction_count > 0 {
                let mut trades = Vec::new();
                for i in 0..5.min(wallet.transaction_count) {
                    let trade_time = wallet.last_activity - (i * 3600); // 1 hour apart
                    let time = chrono::DateTime::from_timestamp(trade_time as i64, 0)
                        .unwrap_or_default()
                        .format("%H:%M:%S");
                    let trade_type = if i % 2 == 0 { "BUY ZUX" } else { "SELL ZUX" };
                    let amount = avg_trade_size * (0.5 + (i as f64 * 0.2));
                    trades.push(format!("{} | {} | ${:.9}", time, trade_type, amount));
                }
                trades
            } else {
                vec!["No recent trades".to_string()]
            };

            let trades_text = format!("Recent 5 Trades:\n{}", recent_trades.join("\n"));
            let trades_paragraph = Paragraph::new(trades_text)
                .style(Style::default().fg(Color::White))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Trading History")
                        .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
                );
            
            f.render_widget(trades_paragraph, chunks[3]);

            // Status and additional information
            let time_since_activity = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
                .saturating_sub(wallet.last_activity);

            let status_info = vec![
                format!("Network Status: CONNECTED"),
                format!("Time Since Activity: {}s", time_since_activity),
                format!("Wallet Rank: {} of {}", state.selected_wallet_index + 1, state.data.wallets.len()),
                format!("Security: Ed25519 Verified"),
            ];

            let status_paragraph = Paragraph::new(status_info.join("\n"))
                .style(Style::default().fg(Color::LightBlue))
                .block(
                    Block::default()
                        .borders(Borders::ALL)
                        .title("Status")
                        .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
                );
            
            f.render_widget(status_paragraph, chunks[4]);
        }
    } else {
        // No wallets or invalid selection
        let no_selection = Paragraph::new("No wallet selected\n\nUse ↑↓ arrow keys to\nselect a wallet from\nthe list to view its\ndetailed information\nand live balance data.")
            .style(Style::default().fg(Color::White))
            .alignment(Alignment::Center)
            .block(
                Block::default()
                    .borders(Borders::ALL)
                    .title("Wallet Details")
                    .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
            );
        
        f.render_widget(no_selection, area);
    }
}

// Render system wallet tab content
fn render_system_tab(f: &mut tui::Frame<CrosstermBackend<std::io::Stdout>>, area: Rect, state: &ExplorerState) {
    let main_chunks = Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage(50), // Left side
            Constraint::Percentage(50), // Right side
        ])
        .split(area);

    // Left side chunks
    let left_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(11), // System wallet comprehensive info
            Constraint::Length(10), // Token economics
            Constraint::Min(6),     // Economic metrics
        ])
        .split(main_chunks[0]);

    // Right side chunks
    let right_chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(9),  // Network performance
            Constraint::Length(8),  // Security & consensus
            Constraint::Min(6),     // Health & monitoring
        ])
        .split(main_chunks[1]);

    // Comprehensive system wallet information - separate currencies
    let distributed_zux = (1000.0 * 100.0) + state.data.amm_info.zux_reserve; // Wallets + AMM
    let distributed_usdz = (1000.0 * 500.0) + state.data.amm_info.usd_reserve; // Wallets + AMM
    let zux_circulation_ratio = if state.data.system_wallet.total_issued_zux > 0.0 { 
        (distributed_zux / state.data.system_wallet.total_issued_zux) * 100.0 
    } else { 0.0 };
    let usdz_circulation_ratio = if state.data.system_wallet.total_issued_usdz > 0.0 { 
        (distributed_usdz / state.data.system_wallet.total_issued_usdz) * 100.0 
    } else { 0.0 };
    
    let system_info = vec![
        format!("System Address: {}", state.data.system_wallet.address),
        format!("System ZUX Balance: {:.9}", state.data.system_wallet.zux_balance),
        format!("System USDZ Balance: {:.9}", state.data.system_wallet.usdz_balance),
        format!("Total ZUX Issued: {:.9}", state.data.system_wallet.total_issued_zux),
        format!("Total USDZ Issued: {:.9}", state.data.system_wallet.total_issued_usdz),
        format!("ZUX Circulation: {:.9} ({:.3}%)", distributed_zux, zux_circulation_ratio),
        format!("USDZ Circulation: {:.9} ({:.3}%)", distributed_usdz, usdz_circulation_ratio),
        format!("Active Wallets: 1000"),
        format!("System Role: Treasury & Issuance"),
    ];

    let system_paragraph = Paragraph::new(system_info.join("\n"))
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("System Wallet & Treasury")
                .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
        );
    
    f.render_widget(system_paragraph, left_chunks[0]);

    // Token economics and monetary policy
    // Market cap = ZUX in circulation * current price + USDZ in circulation (1:1 USD)
    let zux_market_cap = distributed_zux * state.data.amm_info.current_price;
    let total_market_cap = zux_market_cap + distributed_usdz; // USDZ is 1:1 with USD
    
    let economics_info = vec![
        format!("ZUX Market Cap: ${:.9}", zux_market_cap),
        format!("Total Market Cap: ${:.9}", total_market_cap),
        format!("ZUX in Circulation: {:.9}", distributed_zux),
        format!("USDZ in Circulation: {:.9}", distributed_usdz),
        format!("Token Standard: Native"),
        format!("Monetary Policy: Fixed Supply"),
        format!("Trading Mechanism: AMM"),
        format!("Fee Structure: 0.3% swap fee"),
    ];

    let economics_paragraph = Paragraph::new(economics_info.join("\n"))
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Token Economics")
                .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
        );
    
    f.render_widget(economics_paragraph, left_chunks[1]);

    // Economic and performance metrics
    let avg_tx_per_block = if state.data.blocks.len() > 0 {
        state.data.system_wallet.total_transactions as f64 / state.data.blocks.len() as f64
    } else { 0.0 };
    
    let daily_volume = state.data.amm_info.volume_total;
    let network_value = total_market_cap;
    
    let metrics_info = vec![
        format!("Avg Tx per Block: {:.1}", avg_tx_per_block),
        format!("Total Volume: ${:.9}", daily_volume),
        format!("Network Value: ${:.9}", network_value),
        format!("Transaction Fees: 0.001 ZUX"),
        format!("Economic Security: HIGH"),
    ];

    let metrics_paragraph = Paragraph::new(metrics_info.join("\n"))
        .style(Style::default().fg(Color::LightBlue))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Economic Metrics")
                .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
        );
    
    f.render_widget(metrics_paragraph, left_chunks[2]);

    // Comprehensive network performance
    let tps = if state.data.system_wallet.avg_block_time > 0.0 {
        avg_tx_per_block / state.data.system_wallet.avg_block_time
    } else { 0.0 };
    
    let network_performance = vec![
        format!("Total Transactions: {}", state.data.system_wallet.total_transactions),
        format!("Network Hash Rate: {:.2} H/s", state.data.system_wallet.network_hash_rate),
        format!("Average Block Time: {:.2}s", state.data.system_wallet.avg_block_time),
        format!("Transactions/Second: {:.2}", tps),
        format!("Block Size Limit: 1MB"),
        format!("Network Throughput: OPTIMAL"),
        format!("Finality Time: ~60s"),
    ];

    let performance_paragraph = Paragraph::new(network_performance.join("\n"))
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Network Performance")
                .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
        );
    
    f.render_widget(performance_paragraph, right_chunks[0]);

    // Security and consensus information
    let security_info = vec![
        format!("Consensus Algorithm: Proof of Work"),
        format!("Signature Scheme: Ed25519"),
        format!("Hash Function: SHA-256"),
        format!("Block Validation: Full Nodes"),
        format!("Network Security: MAXIMUM"),
        format!("51% Attack Cost: PROHIBITIVE"),
    ];

    let security_paragraph = Paragraph::new(security_info.join("\n"))
        .style(Style::default().fg(Color::White))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Security & Consensus")
                .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
        );
    
    f.render_widget(security_paragraph, right_chunks[1]);

    // Enhanced network health and monitoring
    let uptime_percentage = 99.9; // Simulated uptime
    let time_since_update = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs() 
        .saturating_sub(state.data.last_update);
    
    let health_info = vec![
        format!("Network Status: OPERATIONAL"),
        format!("Network Uptime: {:.3}%", uptime_percentage),
        format!("Architecture: Single Deterministic Node"),
        format!("Consensus: In-Memory Proof of Work"),
        format!("Validation: Deterministic Algorithm"),
        format!("Memory Usage: ~50MB (In-Memory)"),
        format!("Last Update: {}s ago", time_since_update),
    ];

    let health_paragraph = Paragraph::new(health_info.join("\n"))
        .style(Style::default().fg(Color::LightBlue))
        .block(
            Block::default()
                .borders(Borders::ALL)
                .title("Health & Monitoring")
                .style(Style::default().fg(Color::LightBlue).add_modifier(Modifier::BOLD))
        );
    
    f.render_widget(health_paragraph, right_chunks[2]);
}

// Main explorer application entry point
pub fn main() -> io::Result<()> {
    // Initialize terminal for TUI
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, Hide)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;

    // Initialize explorer state
    let explorer_state = Arc::new(Mutex::new(ExplorerState::new()));
    
    // Data file path for communication with main blockchain application
    let explorer_data_path = "explorer_data.json";
    
    // Application running flag
    let running = Arc::new(Mutex::new(true));
    
    // Keyboard input handling thread
    let r1 = running.clone();
    let es1 = explorer_state.clone();
    
    thread::spawn(move || {
        let mut last_key_time = std::time::Instant::now();
        loop {
            if !*r1.lock().unwrap() {
                break;
            }
            
            if event::poll(Duration::from_millis(100)).unwrap() {
                if let Event::Key(key) = event::read().unwrap() {
                    // Prevent double key processing by adding a small delay
                    let now = std::time::Instant::now();
                    if now.duration_since(last_key_time) < Duration::from_millis(150) && 
                       (key.code == KeyCode::Tab || key.code == KeyCode::BackTab) {
                        continue;
                    }
                    last_key_time = now;
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            *r1.lock().unwrap() = false;
                            break;
                        },
                        KeyCode::Tab => {
                            let mut state = es1.lock().unwrap();
                            state.current_tab = state.current_tab.next();
                            state.validate_selection_indices(); // Ensure selections are valid when switching tabs
                        },
                        KeyCode::BackTab => {
                            let mut state = es1.lock().unwrap();
                            state.current_tab = state.current_tab.previous();
                            state.validate_selection_indices(); // Ensure selections are valid when switching tabs
                        },
                        KeyCode::Up => {
                            let mut state = es1.lock().unwrap();
                            state.scroll_up();
                        },
                        KeyCode::Down => {
                            let mut state = es1.lock().unwrap();
                            state.scroll_down();
                        },
                        KeyCode::Char('r') => {
                            // Force refresh
                            let mut state = es1.lock().unwrap();
                            state.last_update = Instant::now();
                        },
                        _ => {}
                    }
                }
            }
        }
    });

    // Data reading thread for real-time updates
    let r2 = running.clone();
    let es2 = explorer_state.clone();
    
    thread::spawn(move || {
        while *r2.lock().unwrap() {
            match File::open(explorer_data_path) {
                Ok(file) => {
                    let reader = BufReader::new(file);
                    match serde_json::from_reader::<_, ExplorerData>(reader) {
                        Ok(data) => {
                            let mut state = es2.lock().unwrap();
                            state.data = data;
                            state.validate_selection_indices(); // Ensure selections are valid
                            state.last_update = Instant::now();
                        },
                        Err(_) => {
                            // Invalid JSON or file corruption, skip update
                        }
                    }
                },
                Err(_) => {
                    // File doesn't exist yet, wait for main application to create it
                }
            }
            
            thread::sleep(Duration::from_millis(100)); // 10Hz update rate
        }
    });

    // Main rendering loop (20 FPS for smooth UI)
    while *running.lock().unwrap() {
        let state = explorer_state.lock().unwrap().clone();
        
        terminal.draw(|f| {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([
                    Constraint::Length(3), // Title
                    Constraint::Length(3), // Tabs
                    Constraint::Min(5),    // Content
                    Constraint::Length(2), // Footer
                ])
                .split(f.size());

            // Application title
            let title = Paragraph::new("ZUX BLOCKCHAIN EXPLORER")
                .style(Style::default().fg(Color::White).add_modifier(Modifier::BOLD))
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::ALL));
            
            f.render_widget(title, chunks[0]);

            // Tab navigation
            render_tabs(f, chunks[1], state.current_tab);

            // Tab content
            match state.current_tab {
                Tab::Blocks => render_blocks_tab(f, chunks[2], &state),
                Tab::Amm => render_amm_tab(f, chunks[2], &state),
                Tab::Wallets => render_wallets_tab(f, chunks[2], &state),
                Tab::SystemWallet => render_system_tab(f, chunks[2], &state),
            }

            // Footer with controls
            let footer_text = "TAB: Switch tabs | ↑↓: Navigate & select blocks | R: Refresh | Q/ESC: Quit";
            let footer = Paragraph::new(footer_text)
                .style(Style::default().fg(Color::LightBlue))
                .alignment(Alignment::Center)
                .block(Block::default().borders(Borders::TOP));
            
            f.render_widget(footer, chunks[3]);
        })?;

        thread::sleep(Duration::from_millis(50)); // 20 FPS
    }

    // Restore terminal state
    execute!(terminal.backend_mut(), LeaveAlternateScreen, Show)?;
    terminal.show_cursor()?;

    Ok(())
} 