use common::{ExecutionReport, MessageAnalyticsWriter, OrderCommand, OrderSide};
use market_handler::{ExchangeGateway, MarketState};
use memmap2::MmapOptions;
use shm_ring_buffer::ShmRing;
use std::collections::HashMap;
use std::time::Instant;
use std::{collections::VecDeque, fs::File};
use strategies::SimpleMidPoint;

pub struct TelemetryWriter {
    outbound: ShmRing,
}

impl TelemetryWriter {
    pub fn new() -> Self {
        let outbound = ShmRing::create_producer("/dev/shm/hft_telemetry", 64 * 1024 * 1024)
            .expect("Fatal: Could not allocate telemetry writer SHM");

        Self { outbound }
    }
}

impl MessageAnalyticsWriter for TelemetryWriter {
    fn write(&mut self, data: &[u8]) {
        self.outbound.write(data);
    }
}

// This demo code doesnt the fact that when we execute a buy/sell it never makes it to order
// book (the file is a days completed trading)...its essentially a ghost.
// In replay mode when we dumped 3500 shares onto market no effect, in reality it would eat
// up bids and drop price. Other systems would see agressive selling and cancel their buy orders.
// I presume real back testing applications would handle this...
pub struct ReplayGateway {
    pub pending_fills: VecDeque<ExecutionReport>,
    pub cash_balances: HashMap<u16, i64>, //+ =cash collectes,- =cash spent
    pub positions: HashMap<u16, i32>,     //track per stock
    pub last_prices: HashMap<u16, u32>,   //track final price
}

impl ReplayGateway {
    pub fn new() -> Self {
        Self {
            pending_fills: VecDeque::new(),
            cash_balances: HashMap::new(),
            positions: HashMap::new(),
            last_prices: HashMap::new(),
        }
    }

    pub fn print_final_pnl(&self) {
        let total_cash: i64 = self.cash_balances.values().sum();
        let cash_dollars = total_cash as f64 / 10000.0;

        let mut inventory_value = 0.0;

        println!("\n************************************************************************");

        println!("Replay complete. vBank a/c details: ");
        println!("Realized Cash Balance : ${:.2}", cash_dollars); //the actual money sitting in bank account

        for (locate, &pos) in self.positions.iter() {
            if let Some(&price) = self.last_prices.get(locate) {
                let market_price = price as f64 / 10000.0;
                let pos_value = (pos as f64) * market_price;
                inventory_value += pos_value;

                println!(
                    "Stock Locate ID {:02}   : {} shares @ ${:.2} (Value: ${:.2})",
                    locate, pos, market_price, pos_value
                );
            }
        }

        /*
        Scenario 1: Short Selling
        The Action: Borrowed and sold shares.
        The Inventory: Position is -3500.
        -3500 shares * $322.00 = $-1,127,000.00
        Because the value is negative, it acts as a liability.
        "We owe these shares. Subtract this amount from our cash to see our true net worth."

        Scenario 2: Going Long (Buying)
        Fired Buy orders for 3,500 shares of Apple.
        Spent cash to physically buy the shares. We own them.
        The Inventory: Position is +3500.
        +3500 shares * $322.00 = +$1,127,000.00
        Because the value is positive, it acts as an asset.
        If we sold it right now, we could add this amount to our cash."

        Negative Inventory Value = Short (Liability).
        Positive Inventory Value = Long (Asset).

        In my own simple terms:
        For short our cash is up, but we owe
        For long our cash is down, but we own.
        */

        let total_equity = cash_dollars + inventory_value;

        println!("Unrealized Inv Value  : ${:.2}", inventory_value);
        println!("TOTAL PnL(equity)     : {:.2}", total_equity);
        println!("\n************************************************************************");
    }
}

impl ExchangeGateway for ReplayGateway {
    fn send_order(&mut self, cmd: &OrderCommand) {
        let fake_fill = ExecutionReport {
            order_id: cmd.order_id,
            last_qty: cmd.quantity,
            last_price: cmd.price,
            stock_locate: cmd.symbol_locate,
            exec_type: 0, // 0 = Filled
        };

        self.pending_fills.push_back(fake_fill);

        let order_id = cmd.order_id;
        println!("Simulated fill generated for Order ID: {}", order_id);

        let trade_value = (cmd.quantity as i64) * (cmd.price as i64);
        let current_pos = self.positions.entry(cmd.symbol_locate).or_insert(0);
        let current_cash = self.cash_balances.entry(cmd.symbol_locate).or_insert(0); // <-- Fetch specific cash

        match cmd.side {
            OrderSide::Buy => {
                *current_cash -= trade_value; // Spend money
                *current_pos += cmd.quantity as i32; // Gain shares
            }
            OrderSide::Sell => {
                *current_cash += trade_value; // Collect money
                *current_pos -= cmd.quantity as i32; // Lose/Short shares
            }
        }

        self.last_prices.insert(cmd.symbol_locate, cmd.price);
    }

    fn poll_confirmed_orders(&mut self) -> Option<ExecutionReport> {
        self.pending_fills.pop_front()
    }

    fn get_position(&self, stock_locate: u16) -> i32 {
        *self.positions.get(&stock_locate).unwrap_or(&0)
    }
    fn get_cash_balance(&self, stock_locate: u16) -> i64 {
        *self.cash_balances.get(&stock_locate).unwrap_or(&0)
    }
}

fn main() {
    println!("Starting replay engine.");

    let core_ids = core_affinity::get_core_ids().expect("Failed to read CPU cores.");
    if core_ids.len() > 2 {
        core_affinity::set_for_current(core_ids[2]);
        println!("Replay engine pinned to Core 2.");
    } else {
        println!("Cant grab a core, continue...");
    }

    // well known high volatility day for testing...01302020.NASDAQ_ITCH50
    // calm day 10302019.NASDAQ_ITCH50.
    let data_file = File::open("/home/amurray/Downloads/10302019.NASDAQ_ITCH50")
        .expect("Cant open 10302019.NASDAQ_ITCH50...");
    let mmap = unsafe { MmapOptions::new().map(&data_file) }.expect("Read only memory map failed.");

    let mut gateway = ReplayGateway::new();
    let writer = TelemetryWriter::new();
    let strategy = SimpleMidPoint::new(0.8);

    let mut market = MarketState::new(
        vec!["AAPL".to_string(), "MSFT".to_string()],
        strategy,
        writer,
    );

    let mut cursor = 0;
    let mut num_of_packets_sent: u64 = 0;

    let now = Instant::now();

    while (cursor + 2) < mmap.len() {
        let msg_len: usize = u16::from_be_bytes([mmap[cursor], mmap[cursor + 1]]) as usize;

        let msg_start = cursor + 2;
        let msg_end = msg_start + msg_len;

        if msg_end > mmap.len() {
            break;
        }

        let msg_payload = &mmap[msg_start..msg_end];

        market.handle_raw_packet(msg_payload, &mut gateway);

        num_of_packets_sent += 1;
        cursor = msg_end;
    }

    let elapsed = now.elapsed();
    let exact_seconds = elapsed.as_secs_f64();
    let packets_per_second = (num_of_packets_sent as f64) / exact_seconds;

    println!(
        "\nComplete, all packets sent. Count: {}. Time taken(secs): {}. Rate: {:.0} packets/sec.",
        num_of_packets_sent,
        elapsed.as_secs(),
        packets_per_second,
    );

    gateway.print_final_pnl();
}
