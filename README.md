<div align="center">
  <img src="https://img.shields.io/badge/ZUX-Blockchain-blue?style=for-the-badge&logo=blockchain.com&logoColor=white" alt="ZUX Blockchain">
  <h1>ZUX Blockchain Simulator</h1>
  <p><strong>High-Performance In-Memory Blockchain with AMM DEX & Real-Time Price Monitoring</strong></p>
  <p>
    <img src="https://img.shields.io/badge/Rust-1.70%2B-orange?style=flat-square&logo=rust" alt="Rust 1.70+">
    <img src="https://img.shields.io/badge/License-MIT-blue?style=flat-square" alt="License">
    <img src="https://img.shields.io/badge/Status-Beta-yellow?style=flat-square" alt="Status">
  </p>
</div>

---

## Table of Contents

- [Overview](#overview)
- [Key Features](#key-features)
- [Technical Architecture](#technical-architecture)
- [Security Features](#security-features)
- [Prerequisites](#prerequisites)
- [Installation & Usage](#installation--usage)
- [Understanding the Output](#understanding-the-output)
- [Advanced Features](#advanced-features)
- [Security Considerations](#security-considerations)
- [Technical Details](#technical-details)
- [Roadmap](#roadmap)
- [Disclaimer](#disclaimer)
- [Developer](#developer)

---

## Overview

ZUX Blockchain Simulator is an advanced, high-performance blockchain implementation that operates entirely in-memory. It's designed to simulate a complete blockchain ecosystem, including wallets, transactions, blocks, and an Automated Market Maker (AMM) for decentralized token swaps.

This simulator features a dual-token economy (ZUX and USDZ), with a fully functional AMM pool that enables real-time price discovery through simulated trading activity. The system includes sophisticated trading agent behavior, hyper-volatile price action, and a real-time price monitoring dashboard.

Perfect for research, education, and testing blockchain applications without the overhead of a distributed network.

---

## Key Features

- **Complete Blockchain Implementation**: Genesis block, wallet creation, transactions, and mining
- **In-Memory Operation**: Zero disk footprint for enhanced privacy and security
- **Dual-Token Economy**: Native ZUX token and stable USDZ token
- **Automated Market Maker (AMM)**: Constant product market maker (x*y=k) with fee model
- **Real-Time Price Monitoring**: TUI-based dashboard with price charts and market statistics
- **Advanced Trading Simulation**: AI-driven trading agents with various strategies
- **Cryptographic Security**: Ed25519 signatures and SHA-256 hashing
- **Deterministic Address Generation**: Secure, collision-resistant wallet addresses
- **Hyper-Volatile Market Simulation**: Realistic market conditions with price volatility
- **Performance Optimized**: Capable of processing thousands of transactions per second

---

## Technical Architecture

The ZUX Blockchain Simulator is built with a modular architecture:

```
┌─────────────────────────────────────────────────────────────┐
│                     ZUX Blockchain Core                     │
├───────────────┬───────────────────────────┬─────────────────┤
│  Wallet System │        Block System       │ Transaction System │
│ ┌───────────┐ │ ┌─────────────────────┐   │ ┌───────────────┐ │
│ │ Ed25519   │ │ │ Block Creation      │   │ │ Transaction   │ │
│ │ Keypairs  │ │ │ ┌─────────────────┐ │   │ │ Verification  │ │
│ └───────────┘ │ │ │ Proof of Work   │ │   │ └───────────────┘ │
│ ┌───────────┐ │ │ └─────────────────┘ │   │ ┌───────────────┐ │
│ │ Balance   │ │ │ ┌─────────────────┐ │   │ │ Signature     │ │
│ │ Management│ │ │ │ Merkle Tree     │ │   │ │ Management    │ │
│ └───────────┘ │ │ └─────────────────┘ │   │ └───────────────┘ │
└───────────────┴───────────────────────┴─────────────────────┘
        │                   │                      │
        ▼                   ▼                      ▼
┌───────────────┐  ┌────────────────────┐  ┌────────────────┐
│  AMM System   │  │ Trading Simulation │  │ Price Monitor  │
│ ┌───────────┐ │  │ ┌────────────────┐ │  │ ┌────────────┐ │
│ │ Liquidity │ │  │ │ Trading Agents │ │  │ │ Real-time  │ │
│ │ Pool      │ │◄─┼─┤ with Strategies│ │  │ │ Charts     │ │
│ └───────────┘ │  │ └────────────────┘ │  │ └────────────┘ │
│ ┌───────────┐ │  │ ┌────────────────┐ │  │ ┌────────────┐ │
│ │ Price     │ │──┼─► Market         │ │──┼─► Market     │ │
│ │ Discovery │ │  │ │ Simulation     │ │  │ │ Statistics │ │
│ └───────────┘ │  │ └────────────────┘ │  │ └────────────┘ │
└───────────────┘  └────────────────────┘  └────────────────┘
```

---

## Security Features

- **In-Memory Operation**: All data (wallets, transactions, blocks) is stored exclusively in RAM
- **Zero Disk Persistence**: No data is written to disk at any point
- **Clean Exit**: When the program terminates, all data is automatically cleared from memory
- **Cryptographic Security**: Ed25519 signatures for transactions and SHA-256 for block hashing
- **Private Key Isolation**: All cryptographic keys are generated and held only in memory
- **Deterministic Address Generation**: Secure permutation-based algorithm for unique addresses

---

## Prerequisites

- **Rust Programming Language**: Latest stable version (1.70+)
- **Cargo Package Manager**: Included with Rust
- **Terminal with TUI Support**: For the price monitoring dashboard
- **System Requirements**:
  - RAM: 1GB minimum (2GB+ recommended)
  - CPU: Multi-core processor recommended for optimal performance
  - OS: Windows, macOS, or Linux

---

## Installation & Usage

### Step 1: Clone and Build

```bash
# Clone the repository (if available)
git clone https://github.com/yourusername/zux-blockchain-simulator
cd zux-blockchain-simulator

# Or use the code directly
cd path/to/zux-blockchain-simulator

# Build the program in release mode for optimal performance
cargo build --release
```

### Step 2: Run the Simulator

```bash
# Run directly with cargo
cargo run --release

# Or run the compiled binary
./target/release/practicerust2
```

### Step 3: Interact with the Simulator

When you run the program, it will:

1. Start the main blockchain simulation in the current terminal
2. Automatically open a separate terminal window showing the live ZUX/USDZ price chart
3. Begin simulating trading activity and block creation

**Price Monitor Controls:**
- Press `q` or `ESC` to exit the price monitor
- The chart automatically scales to show price movements

### Step 4: Safe Termination

When you're done with the simulation:

1. Let the program complete naturally, or terminate it with `Ctrl+C`
2. For additional security, consider clearing your terminal history:
   - Bash: `history -c`
   - PowerShell: `Clear-History`
   - Windows CMD: `doskey /reinstall`

---

## Understanding the Output

The simulator provides rich output across multiple components:

### Blockchain Core
- Genesis block creation details
- System wallet initialization
- Creation of 1000 regular wallets with unique addresses
- Token credit transactions (50 ZUX and 100 USDZ per wallet)
- Block mining and validation information

### AMM DEX
- Initial liquidity pool establishment
- Real-time swap transactions
- Price impact calculations
- Fee collection metrics
- Liquidity provider statistics

### Price Monitor
- Real-time ZUX/USDZ price chart
- Price change indicators (↑ for increase, ↓ for decrease)
- 24-hour high and low prices
- Trading volume simulation
- Market capitalization metrics

---

## Advanced Features

### Trading Agent Simulation

The simulator includes sophisticated trading agents with various behaviors:

- **Standard Traders**: Regular market participants with moderate trading patterns
- **Whales**: Large holders who can significantly impact price (10% of wallets)
- **Mega Whales**: Ultra-large holders capable of dramatic market moves (1% of wallets)
- **Market Manipulators**: Agents with specific price direction goals (30% of wallets)
- **FOMO/Panic Traders**: Agents that react to price movements with emotional trading

### AMM Pool Mechanics

The AMM implementation follows the constant product formula (x*y=k) with these features:

- **Dynamic Fee Model**: 0.3% fee on all swaps
- **Price Impact Calculation**: Larger trades have proportionally larger price impact
- **Slippage Protection**: Minimum output amount enforcement
- **Liquidity Depth Simulation**: Realistic market depth and price resistance

### Hyper-Volatile Market Conditions

The simulator creates realistic but extreme market conditions:

- Rapid price fluctuations
- Sudden price spikes and crashes
- Momentum-based trading waves
- Liquidity crunches during high volatility

---

## Security Considerations

For maximum security when using the simulator:

- **Memory Dump Protection**: Be aware that sophisticated memory forensics could potentially recover data from RAM after program execution. For maximum security, consider rebooting your system after use.
- **Terminal History**: Your terminal may save commands in its history. Clear this as mentioned in the usage section.
- **Swap Files**: If your system uses swap files/partition, sensitive data might be written there. Consider disabling swap before running for maximum security.
- **Debug Information**: The program outputs information to the console. Run in a private terminal session if this is a concern.

---

## Technical Details

### Cryptography

- **Digital Signatures**: Ed25519 for transaction signing and verification
- **Hashing Algorithm**: SHA-256 for block hashing and merkle tree construction
- **Address Generation**: Base-62 encoding with permutation-based uniqueness

### Consensus Mechanism

- **Proof-of-Work**: Simplified mining algorithm for block validation
- **Difficulty Adjustment**: Dynamic difficulty based on network conditions
- **Block Verification**: Full validation of block integrity and transaction signatures

### Performance Metrics

- **Transaction Throughput**: Capable of processing 5,000+ transactions per second
- **Block Creation Time**: Configurable, defaults to ~1 second per block
- **Memory Footprint**: Approximately 50MB for full simulation with 1,000 wallets

---

## Roadmap

Future development plans for the ZUX Blockchain Simulator:

- [ ] Multi-node network simulation
- [ ] Consensus algorithm variants (PoS, DPoS, etc.)
- [ ] Smart contract execution environment
- [ ] Advanced trading strategy framework
- [ ] Network partition and recovery simulation
- [ ] Graphical user interface
- [ ] Exportable simulation results
- [ ] Customizable simulation parameters

---

## Disclaimer

This is a simulation tool for educational purposes only. It is not intended for use with real assets or in production environments. The cryptographic implementations, while functional, have not been audited for security vulnerabilities.

The simulated market behavior does not represent financial advice and should not be used as a basis for real-world trading decisions.

---

## Developer

<div align="center">
  <img src="https://img.shields.io/badge/Developed%20by-Amin%20Nizam-blue?style=for-the-badge" alt="Developer">
</div>

<br>

<div align="center">
  <p>
    <a href="https://github.com/aminnizamdev" target="_blank">
      <img src="https://img.shields.io/badge/GitHub-aminnizamdev-181717?style=flat-square&logo=github" alt="GitHub">
    </a>
    <a href="https://x.com/aminnizamdev" target="_blank">
      <img src="https://img.shields.io/badge/Twitter-aminnizamdev-1DA1F2?style=flat-square&logo=twitter" alt="Twitter">
    </a>
  </p>
  <p>
    <a href="mailto:aminnizam.dev@yahoo.com">
      <img src="https://img.shields.io/badge/Email-aminnizam.dev%40yahoo.com-D14836?style=flat-square&logo=yahoo" alt="Email">
    </a>
  </p>
</div>

<div align="center">
  <p>Rust + Python engineer building real-time blockchain intelligence systems.<br>Creator of ULTRETH, TACX, and Decentralized Parliament (DePar)</p>
</div>

---

<div align="center">
  <p><strong>ZUX Blockchain Simulator</strong> - In-Memory Edition</p>
  <p>© 2023-2024 Amin Nizam. All Rights Reserved.</p>
</div>