use std::fs::File;
use std::io::{self, BufRead, BufReader};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};
use std::collections::VecDeque;
use std::sync::{Arc, Mutex};
use crossterm::{execute, terminal::{EnterAlternateScreen, LeaveAlternateScreen}, cursor::{Hide, Show}, event::{self, Event, KeyCode}};
use tui::{backend::CrosstermBackend, Terminal, widgets::{Block, Borders, Chart, Dataset, Axis, GraphType}, symbols, layout::{Layout, Constraint, Direction}, style::{Style, Modifier}};
// Removed unused import: colored::*;

// Structure to hold price data points
#[derive(Clone)]
struct PricePoint {
    timestamp: u64,
    price: f64,
}

// Structure to hold market statistics
struct MarketStats {
    price: f64,
    price_change_pct: f64,
    price_change_24h_pct: f64,
    high_24h: f64,
    low_24h: f64,
    volume_24h: f64,
    market_cap: f64,
    last_update: Instant,
}

impl MarketStats {
    fn new() -> Self {
        MarketStats {
            price: 0.0,
            price_change_pct: 0.0,
            price_change_24h_pct: 0.0,
            high_24h: 0.0,
            low_24h: 0.0,
            volume_24h: 0.0,
            market_cap: 0.0,
            last_update: Instant::now(),
        }
    }

    fn update(&mut self, current_price: f64, price_history: &VecDeque<PricePoint>) {
        // Update price
        let old_price = self.price;
        self.price = current_price;
        
        // Calculate price change percentage since last update
        if old_price > 0.0 {
            self.price_change_pct = ((current_price - old_price) / old_price) * 100.0;
        }
        
        // Update 24h high and low
        if price_history.len() > 0 {
            // Find 24h high and low
            let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
            let one_day_ago = now.saturating_sub(24 * 60 * 60);
            
            let mut high: f64 = 0.0;
            let mut low: f64 = f64::MAX;
            
            for point in price_history.iter() {
                if point.timestamp >= one_day_ago {
                    high = high.max(point.price);
                    low = low.min(point.price);
                }
            }
            
            if low == f64::MAX {
                low = current_price;
            }
            
            self.high_24h = high;
            self.low_24h = low;
            
            // Calculate 24h price change
            if let Some(oldest_point) = price_history.iter().find(|p| p.timestamp >= one_day_ago) {
                if oldest_point.price > 0.0 {
                    self.price_change_24h_pct = ((current_price - oldest_point.price) / oldest_point.price) * 100.0;
                }
            }
        } else {
            self.high_24h = current_price;
            self.low_24h = current_price;
        }
        
        // Simulate volume and market cap based on price
        self.volume_24h = current_price * 1_000_000.0 * (1.0 + (rand::random::<f64>() * 0.1));
        self.market_cap = current_price * 10_000_000.0;
        
        self.last_update = Instant::now();
    }
}

fn main() -> io::Result<()> {
    // Initialize terminal
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, Hide)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;
    terminal.clear()?;
    
    // Initialize price history and market stats
    let price_history = Arc::new(Mutex::new(VecDeque::with_capacity(1000)));
    let market_stats = Arc::new(Mutex::new(MarketStats::new()));
    
    // Path to the price data file that will be updated by the main program
    let price_file_path = "price_data.txt";
    
    // Flag to indicate when to exit
    let running = Arc::new(Mutex::new(true));
    let r = running.clone();
    
    // Spawn a thread to handle keyboard input
    thread::spawn(move || {
        loop {
            if event::poll(Duration::from_millis(100)).unwrap() {
                if let Event::Key(key) = event::read().unwrap() {
                    if key.code == KeyCode::Char('q') || key.code == KeyCode::Esc {
                        *r.lock().unwrap() = false;
                        break;
                    }
                }
            }
        }
    });
    
    // Spawn a thread to read price data
    let ph = price_history.clone();
    let ms = market_stats.clone();
    let r2 = running.clone();
    
    thread::spawn(move || {
        let mut last_price = 0.0;
        let mut last_check = Instant::now();
        
        while *r2.lock().unwrap() {
            // Try to read the current price from the file
            match File::open(price_file_path) {
                Ok(file) => {
                    let reader = BufReader::new(file);
                    if let Some(Ok(line)) = reader.lines().next() {
                        if let Ok(current_price) = line.trim().parse::<f64>() {
                            // Only update if price has changed or 100ms has passed
                            if current_price != last_price || last_check.elapsed() > Duration::from_millis(100) {
                                // Update last price and check time
                                last_price = current_price;
                                last_check = Instant::now();
                                
                                // Add to price history
                                let timestamp = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
                                let price_point = PricePoint {
                                    timestamp,
                                    price: current_price,
                                };
                                
                                let mut history = ph.lock().unwrap();
                                history.push_back(price_point);
                                
                                // Keep only the last 1000 points
                                if history.len() > 1000 {
                                    history.pop_front();
                                }
                                
                                // Update market stats
                                ms.lock().unwrap().update(current_price, &history);
                            }
                        }
                    }
                },
                Err(_) => {
                    // File doesn't exist yet, just wait
                    thread::sleep(Duration::from_millis(100));
                }
            }
            
            // Sleep for a short interval before checking again
            thread::sleep(Duration::from_millis(50));
        }
    });
    
    // Main rendering loop
    while *running.lock().unwrap() {
        let stats = market_stats.lock().unwrap().clone();
        let history = price_history.lock().unwrap().clone();
        
        terminal.draw(|f| {
            // Create layout
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .margin(1)
                .constraints([
                    Constraint::Length(3),  // Title
                    Constraint::Length(8),  // Stats
                    Constraint::Min(10),    // Chart
                    Constraint::Length(1),  // Footer
                ].as_ref())
                .split(f.size());
            
            // Title block
            let title_block = Block::default()
                .title(" ZUX/USDZ LIVE PRICE MONITOR ")
                .borders(Borders::ALL);
            f.render_widget(title_block, chunks[0]);
            
            // Render title text
            let title_text = format!("ZUX/USDZ: {:.6} USDZ", stats.price);
            f.render_widget(
                tui::widgets::Paragraph::new(title_text)
                    .style(Style::default().add_modifier(Modifier::BOLD))
                    .alignment(tui::layout::Alignment::Center),
                chunks[0]
            );
            
            // Stats block
            let stats_block = Block::default()
                .title(" MARKET STATISTICS ")
                .borders(Borders::ALL);
            f.render_widget(stats_block, chunks[1]);
            
            // Render stats
            // Color is now handled directly in the text formatting
            
            let price_change_text = if stats.price_change_pct > 0.0 {
                format!("↑ +{:.2}%", stats.price_change_pct)
            } else if stats.price_change_pct < 0.0 {
                format!("↓ {:.2}%", stats.price_change_pct)
            } else {
                "0.00%".to_string()
            };
            
            let price_change_24h_text = if stats.price_change_24h_pct > 0.0 {
                format!("↑ +{:.2}%", stats.price_change_24h_pct)
            } else if stats.price_change_24h_pct < 0.0 {
                format!("↓ {:.2}%", stats.price_change_24h_pct)
            } else {
                "0.00%".to_string()
            };
            
            let stats_text = vec![
                format!("Price Change: {}", price_change_text),
                format!("24h Change: {}", price_change_24h_text),
                format!("24h High: {:.6} USDZ", stats.high_24h),
                format!("24h Low: {:.6} USDZ", stats.low_24h),
                format!("24h Volume: {:.2} USDZ", stats.volume_24h),
                format!("Market Cap: {:.2} USDZ", stats.market_cap),
            ];
            
            // Create a layout for the stats
            let stats_chunks = Layout::default()
                .direction(Direction::Horizontal)
                .constraints([
                    Constraint::Percentage(50),
                    Constraint::Percentage(50),
                ].as_ref())
                .split(chunks[1]);
            
            // Left stats
            let left_stats = tui::widgets::Paragraph::new(vec![
                stats_text[0].clone(),
                stats_text[1].clone(),
                stats_text[2].clone(),
            ].join("\n"))
                .style(Style::default())
                .block(Block::default());
            f.render_widget(left_stats, stats_chunks[0]);
            
            // Right stats
            let right_stats = tui::widgets::Paragraph::new(vec![
                stats_text[3].clone(),
                stats_text[4].clone(),
                stats_text[5].clone(),
            ].join("\n"))
                .style(Style::default())
                .block(Block::default());
            f.render_widget(right_stats, stats_chunks[1]);
            
            // Chart block
            let chart_block = Block::default()
                .title(" PRICE CHART ")
                .borders(Borders::ALL);
            
            // Prepare chart data
            let mut chart_data = Vec::new();
            let mut min_price: f64 = f64::MAX;
            let mut max_price: f64 = 0.0;
            
            for (i, point) in history.iter().enumerate() {
                chart_data.push((i as f64, point.price));
                min_price = min_price.min(point.price);
                max_price = max_price.max(point.price);
            }
            
            // Add some padding to min/max
            let price_range = max_price - min_price;
            min_price = min_price - (price_range * 0.05);
            max_price = max_price + (price_range * 0.05);
            
            if !chart_data.is_empty() {
                let datasets = vec![
                    Dataset::default()
                        .name("ZUX/USDZ")
                        .marker(symbols::Marker::Braille)
                        .graph_type(GraphType::Line)
                        .style(Style::default().fg(tui::style::Color::Cyan))
                        .data(&chart_data),
                ];
                
                let chart = Chart::new(datasets)
                    .block(chart_block)
                    .x_axis(
                        Axis::default()
                            .title("Time")
                            .style(Style::default().fg(tui::style::Color::DarkGray))
                            .bounds([0.0, chart_data.len() as f64])
                            .labels(vec![]),
                    )
                    .y_axis(
                        Axis::default()
                            .title("Price (USDZ)")
                            .style(Style::default().fg(tui::style::Color::DarkGray))
                            .bounds([min_price, max_price])
                            .labels(vec![
                                format!("{:.6}", min_price).into(),
                                format!("{:.6}", (min_price + max_price) / 2.0).into(),
                                format!("{:.6}", max_price).into(),
                            ]),
                    );
                
                f.render_widget(chart, chunks[2]);
            } else {
                // If no data yet, just show the empty block
                f.render_widget(chart_block, chunks[2]);
            }
            
            // Footer
            let footer_text = "Press 'q' or 'Esc' to quit";
            let footer = tui::widgets::Paragraph::new(footer_text)
                .style(Style::default().fg(tui::style::Color::DarkGray))
                .alignment(tui::layout::Alignment::Center);
            f.render_widget(footer, chunks[3]);
        })?;
        
        thread::sleep(Duration::from_millis(100));
    }
    
    // Restore terminal
    execute!(terminal.backend_mut(), LeaveAlternateScreen, Show)?;
    terminal.show_cursor()?;
    
    Ok(())
}

// Clone implementation for MarketStats
impl Clone for MarketStats {
    fn clone(&self) -> Self {
        MarketStats {
            price: self.price,
            price_change_pct: self.price_change_pct,
            price_change_24h_pct: self.price_change_24h_pct,
            high_24h: self.high_24h,
            low_24h: self.low_24h,
            volume_24h: self.volume_24h,
            market_cap: self.market_cap,
            last_update: self.last_update,
        }
    }
}