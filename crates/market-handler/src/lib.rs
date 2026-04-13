use common::MessageAnalyticsWriter;
use common::{AggressorSide, ExecutionReport, OrderCommand, OrderSide, Side, TelemetryPayload};
use itch_parser::ItchMessage;
use itch_parser::parse_itch_message;
use orderbook::L3OrderBook;
use risk_engine::{LocalRisk, RiskDecision};
use std::collections::HashSet;
use strategies::Strategy;
use ticker::TradeTicker;

pub trait ExchangeGateway {
    fn send_order(&mut self, cmd: &OrderCommand);
    fn poll_confirmed_orders(&mut self) -> Option<ExecutionReport>;
    fn get_position(&self, stock_locate: u16) -> i32;
    fn get_cash_balance(&self, stock_locate: u16) -> i64;
}

pub struct SymbolProcessor<S: Strategy> {
    pub symbol: String,
    pub stock_locate: u16,
    pub order_book: L3OrderBook,
    pub strategy: S,
    pub ticker: TradeTicker,
    pub order_tracker: HashSet<u64>,
    pub risk_engine: LocalRisk,
    pub last_midpoint: u32,
}

impl<S: Strategy> SymbolProcessor<S> {
    pub fn handle_event(
        &mut self,
        msg: ItchMessage,
        telemetry_writer: &mut impl MessageAnalyticsWriter,
        gateway: &mut impl ExchangeGateway,
        global_order_id: &mut u64,
        current_time_ns: u64,
    ) {
        match msg {
            ItchMessage::AddOrder {
                stock_locate: _,
                order_id,
                side,
                quantity,
                price,
            } => self.order_book.add_order(order_id, side, price, quantity),

            ItchMessage::ModifyOrder {
                stock_locate: _,
                order_id,
                quantity_to_remove,
            } => self.order_book.cancel_order(order_id, quantity_to_remove),

            // This doesn't affect the book, just the ticker/last sale
            ItchMessage::Trade {
                stock_locate: _,
                price,
                quantity,
                match_number,
                bid,
            } => {
                let side = match bid {
                    Side::Buy => AggressorSide::Buyer,
                    Side::Sell => AggressorSide::Seller,
                };

                self.ticker.add_trade(price, quantity, match_number, side);
            }

            ItchMessage::DeleteOrder {
                stock_locate: _,
                order_id,
            } => {
                if let Some(order) = self.order_book.orders.get(&order_id) {
                    let current_qty = order.quantity;
                    self.order_book.cancel_order(order_id, current_qty);
                }
            }
            //Execution: Someone else bought those shares. The shares must leave the book.
            ItchMessage::OrderExecuted {
                stock_locate: _,
                order_id,
                quantity,
                match_number,
            } => {
                // Borrow checker requires copy to allow cancel order (need mut access)
                let trade_info = if let Some(order) = self.order_book.orders.get(&order_id) {
                    let side = match order.side {
                        Side::Buy => AggressorSide::Seller,
                        Side::Sell => AggressorSide::Buyer,
                    };
                    // return a small tuple containing the data not references
                    Some((order.price, side))
                } else {
                    None
                };

                if let Some((price, side)) = trade_info {
                    self.ticker.add_trade(price, quantity, match_number, side);

                    // mutable borrow is now allowed
                    self.order_book.cancel_order(order_id, quantity);
                }
            }

            // Remove liquidity at original price, but record trade at 'price'
            ItchMessage::OrderExecutedWithPrice {
                stock_locate: _,
                order_id,
                quantity,
                price,
                match_number,
            } => {
                if let Some(order) = self.order_book.orders.get(&order_id) {
                    //If the order in the book was a Bid, the person who executed against it is a seller and vice versa.
                    let side = match order.side {
                        Side::Buy => AggressorSide::Seller,
                        Side::Sell => AggressorSide::Buyer,
                    };

                    self.ticker.add_trade(price, quantity, match_number, side);

                    self.order_book.cancel_order(order_id, quantity);
                }
            }

            _ => {}
        }

        if let Some(mut cmd) = self.strategy.on_book_update(
            self.order_book.get_best_bid(),
            self.order_book.get_best_ask(),
        ) {
            cmd.symbol_locate = self.stock_locate;
            let order_price = cmd.price;
            let order_quantity = cmd.quantity;
            let side = cmd.side;

            match self.risk_engine.check(
                &cmd,
                self.order_book.get_best_bid(),
                self.order_book.get_best_ask(),
            ) {
                RiskDecision::Pass => {
                    println!(
                        "Order Id {} sent to execution engine Stock:{} | Price:{} | Quantity:{} | Type: {}",
                        *global_order_id,
                        self.stock_locate,
                        order_price,
                        order_quantity,
                        side as u8
                    );

                    cmd.order_id = *global_order_id;
                    *global_order_id += 1;

                    gateway.send_order(&cmd);

                    let _agg_side = match cmd.side {
                        OrderSide::Buy => AggressorSide::Buyer,
                        OrderSide::Sell => AggressorSide::Seller,
                    };

                    self.order_tracker.insert(cmd.order_id);
                }
                RiskDecision::Fail(_reason) => {
                    eprintln!("Risk rejection {} | Reason: {}", self.symbol, _reason);
                }
            }
        }

        self.send_telemetry(gateway, telemetry_writer, current_time_ns);
    }
}

impl<S: Strategy> SymbolProcessor<S> {
    pub fn send_telemetry(
        &mut self,
        gateway: &mut impl ExchangeGateway,
        telemetry_writer: &mut impl MessageAnalyticsWriter,
        current_time_ns: u64,
    ) {
        let current_bid = self
            .order_book
            .get_best_bid()
            .map(|(price, _qty)| price)
            .unwrap_or(0);
        let current_ask = self
            .order_book
            .get_best_ask()
            .map(|(price, _qty)| price)
            .unwrap_or(0);

        let current_midpoint = if current_bid > 0 && current_ask > 0 {
            ((current_bid + current_ask) / 2) as u32
        } else {
            0
        };

        // ONLY send telemetry if the price actually changed (or if it's the first tick)
        if current_midpoint > 0 && current_midpoint != self.last_midpoint {
            self.last_midpoint = current_midpoint;

            let cash = gateway.get_cash_balance(self.stock_locate);
            let pos = gateway.get_position(self.stock_locate) as i64;
            let price = current_midpoint as i64;

            // Total Equity = Cash Balance + (Inventory * Current Price)
            let total_equity = cash + (pos * price);

            let payload = TelemetryPayload {
                timestamp_ns: current_time_ns,
                realised_pnl: total_equity,
                midpoint_px: current_midpoint,
                position: gateway.get_position(self.stock_locate),
                latency_ns: 30, // update later
                locate_id: self.stock_locate,
            };

            // ZERO-COPY CAST: Trick compiler into viewing the struct as raw bytes
            let payload_bytes: &[u8] = unsafe {
                std::slice::from_raw_parts(
                    (&payload as *const common::TelemetryPayload) as *const u8,
                    std::mem::size_of::<common::TelemetryPayload>(),
                )
            };

            telemetry_writer.write(payload_bytes);
        }
    }
}

pub struct MarketState<S: Strategy, W: MessageAnalyticsWriter> {
    pub processors: Vec<SymbolProcessor<S>>,
    pub lookup_table: [i16; 65536], //-1 ignored
    pub global_order_id_counter: u64,
    pub telemetry_writer: W,
    pub current_time_ns: u64,
    pub midnight_today_ns: u64,
}

impl<S: Strategy + Clone, W: MessageAnalyticsWriter> MarketState<S, W> {
    fn get_locate_from_msg(&self, msg: &ItchMessage) -> u16 {
        match msg {
            ItchMessage::AddOrder { stock_locate, .. } => *stock_locate,
            ItchMessage::ModifyOrder { stock_locate, .. } => *stock_locate,
            ItchMessage::Trade { stock_locate, .. } => *stock_locate,
            ItchMessage::DeleteOrder { stock_locate, .. } => *stock_locate,
            ItchMessage::OrderExecuted { stock_locate, .. } => *stock_locate,
            ItchMessage::OrderExecutedWithPrice { stock_locate, .. } => *stock_locate,
            ItchMessage::StockDirectory { stock_locate, .. } => *stock_locate,
        }
    }
}

impl<S: Strategy + Clone, W: MessageAnalyticsWriter> MarketState<S, W> {
    pub fn new(target_symbols: Vec<String>, base_strategy: S, writer: W) -> Self {
        let mut processors = Vec::new();

        for sym in target_symbols {
            processors.push(SymbolProcessor {
                symbol: sym,
                stock_locate: 0, //assigned when we get 'R' message
                order_book: L3OrderBook::new(),
                strategy: base_strategy.clone(), //each stock get their own stragety and their own position tracker. In a real system Total Position is required.
                ticker: TradeTicker::new(),
                order_tracker: HashSet::new(),
                //in a real system would come from config...
                risk_engine: LocalRisk::new(1000, 0.02, 3500),
                last_midpoint: 0,
            });
        }

        let now_secs = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        let seconds_since_midnight = now_secs % 86400; // 86400 seconds in a day
        let midnight_today_ns = (now_secs - seconds_since_midnight) * 1_000_000_000;

        Self {
            processors,
            lookup_table: [-1; 65536],
            global_order_id_counter: 0,
            telemetry_writer: writer,
            current_time_ns: 0,
            midnight_today_ns,
        }
    }

    pub fn handle_stock_directory(&mut self, stock_locate: u16, symbol: String) {
        //let clean_symbol = symbol.trim(); //remove right padded spaces
        let clean_symbol = symbol.trim_matches(char::from(0)).trim();

        if let Some(pos) = self
            .processors
            .iter()
            .position(|p| p.symbol == clean_symbol)
        {
            self.processors[pos].stock_locate = stock_locate;
            self.lookup_table[stock_locate as usize] = pos as i16;
            println!(
                "Successfully mapped {} to stock locate ID: {}",
                clean_symbol, stock_locate
            );
        }
    }

    pub fn poll_confirmed_orders(&mut self, gateway: &mut impl ExchangeGateway) {
        while let Some(exe_report) = gateway.poll_confirmed_orders() {
            let order_id = exe_report.order_id;
            let last_qty = exe_report.last_qty;
            let last_price = exe_report.last_price;
            let stock_locate = exe_report.stock_locate;

            let slot_id = self.lookup_table[stock_locate as usize];

            if slot_id != -1 {
                let processor = &mut self.processors[slot_id as usize];
                if processor.order_tracker.contains(&order_id) {
                    println!(
                        "Read order back from simulator exchange. Stock: {} | ID: {} | Quantity: {} | Price: {}",
                        processor.symbol, order_id, last_qty, last_price
                    );

                    processor
                        .strategy
                        .on_trade(last_price as u64, last_qty, AggressorSide::Buyer);

                    processor.order_tracker.remove(&order_id);
                }
            }
        }
    }
    pub fn handle_raw_packet(&mut self, raw_data: &[u8], gateway: &mut impl ExchangeGateway) {
        let ts_bytes = [
            0,
            0,
            raw_data[5],
            raw_data[6],
            raw_data[7],
            raw_data[8],
            raw_data[9],
            raw_data[10],
        ];
        let ns_since_midnight = u64::from_be_bytes(ts_bytes);
        self.current_time_ns = self.midnight_today_ns + ns_since_midnight;

        if let Some(msg) = parse_itch_message(raw_data) {
            self.filter_message_for_symbols(msg, gateway);
        }

        self.poll_confirmed_orders(gateway);
    }

    fn filter_message_for_symbols(&mut self, msg: ItchMessage, gateway: &mut impl ExchangeGateway) {
        match msg {
            ItchMessage::StockDirectory {
                stock_locate,
                symbol,
            } => {
                self.handle_stock_directory(stock_locate, symbol);
            }

            // For all other messages, use look up table
            _ => {
                let locate = self.get_locate_from_msg(&msg);
                let slot_idx = self.lookup_table[locate as usize];

                //one of our stocks!
                if slot_idx != -1 {
                    let processor = &mut self.processors[slot_idx as usize];
                    processor.handle_event(
                        msg,
                        &mut self.telemetry_writer,
                        gateway,
                        &mut self.global_order_id_counter,
                        self.current_time_ns,
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use common::MessageAnalyticsWriter;
    use strategies::SimpleMidPoint;

    pub struct MockGateway {
        pub sent_orders: Vec<OrderCommand>,
    }

    impl ExchangeGateway for MockGateway {
        fn send_order(&mut self, cmd: &OrderCommand) {
            self.sent_orders.push(cmd.clone());
        }

        fn poll_confirmed_orders(&mut self) -> Option<ExecutionReport> {
            None
        }
        fn get_position(&self, _stock_locate: u16) -> i32 {
            0
        }
        fn get_cash_balance(&self, _stock_locate: u16) -> i64 {
            0
        }
    }

    pub struct MockAnalytics {
        pub messages: Vec<Vec<u8>>,
    }

    impl MessageAnalyticsWriter for MockAnalytics {
        fn write(&mut self, data: &[u8]) {
            self.messages.push(data.to_vec());
        }
    }

    //Note NASDAQ ITCH  the price field in almost all messages is a 4-byte unsigned integer
    //u32 represents the price with 4 decimal places of precision e.g. 1.2345
    fn create_itch_packet(
        msg_type: char,
        locate: u16,
        id: u64,
        qty: u32,
        price: u32,
        match_number: u64,
        side: char,
        symbol: &str,
    ) -> Vec<u8> {
        let mut buf = vec![0u8; 50];
        buf[0] = msg_type as u8;

        buf[1..3].copy_from_slice(&locate.to_be_bytes());

        match msg_type {
            //stock directory
            'R' => {
                let mut sym_bytes = [b' '; 8];
                let bytes = symbol.as_bytes();
                let len = std::cmp::min(bytes.len(), 8);
                sym_bytes[..len].copy_from_slice(&bytes[..len]);
                buf[11..19].copy_from_slice(&sym_bytes);
                buf.truncate(39);
            }

            //new order
            'A' => {
                buf[11..19].copy_from_slice(&id.to_be_bytes());
                buf[19] = side as u8;
                buf[20..24].copy_from_slice(&qty.to_be_bytes());
                buf[32..36].copy_from_slice(&price.to_be_bytes());
                buf.truncate(36);
            }

            //OrderExcuted
            'E' => {
                buf[11..19].copy_from_slice(&id.to_be_bytes());
                buf[19..23].copy_from_slice(&qty.to_be_bytes());
                buf[23..31].copy_from_slice(&match_number.to_be_bytes());
                buf.truncate(31);
            }

            //OrderExecutedWithPrice
            'C' => {
                buf[11..19].copy_from_slice(&id.to_be_bytes());
                buf[19..23].copy_from_slice(&qty.to_be_bytes());
                buf[23..31].copy_from_slice(&match_number.to_be_bytes());
                buf[32..36].copy_from_slice(&price.to_be_bytes());
                buf.truncate(36);
            }

            //Modify
            'X' => {
                buf[11..19].copy_from_slice(&id.to_be_bytes());
                buf[19..23].copy_from_slice(&qty.to_be_bytes()); // Canceled shares
                buf.truncate(23);
            }

            //DeleteOrder
            'D' => {
                buf[11..19].copy_from_slice(&id.to_be_bytes());
                buf.truncate(19);
            }

            //Trade
            'P' => {
                buf[19] = side as u8;
                buf[20..24].copy_from_slice(&qty.to_be_bytes());
                buf[32..36].copy_from_slice(&price.to_be_bytes());
                buf[36..44].copy_from_slice(&match_number.to_be_bytes());
                buf.truncate(44);
            }
            _ => panic!("Unsupported: {}", msg_type),
        }

        buf
    }

    fn setup_test_market(symbol: &str, locate: u16) -> MarketState<SimpleMidPoint, MockAnalytics> {
        let strategy = SimpleMidPoint::new(0.8);

        let writer = MockAnalytics {
            messages: Vec::new(),
        };

        let mut market = MarketState::new(vec![symbol.to_string()], strategy, writer);

        let r_packet = create_itch_packet('R', locate, 0, 0, 0, 0, ' ', symbol);

        let mut dummy_gateway = MockGateway {
            sent_orders: Vec::new(),
        };
        market.handle_raw_packet(&r_packet, &mut dummy_gateway);

        market
    }

    fn setup_multi_stock_market(
        symbols: Vec<(&str, u16)>,
    ) -> MarketState<SimpleMidPoint, MockAnalytics> {
        let strategy = SimpleMidPoint::new(0.8);
        let writer = MockAnalytics {
            messages: Vec::new(),
        };

        let name_list: Vec<String> = symbols.iter().map(|(s, _)| s.to_string()).collect();
        let mut market = MarketState::new(name_list, strategy, writer);

        let mut dummy_gateway = MockGateway {
            sent_orders: Vec::new(),
        };

        for (name, locate) in symbols {
            let r_packet = create_itch_packet('R', locate, 0, 0, 0, 0, ' ', name);
            market.handle_raw_packet(&r_packet, &mut dummy_gateway);
        }

        market
    }

    #[test]
    fn test_multi_stock_routing_isolation() {
        let mut market = setup_multi_stock_market(vec![("AAPL", 10), ("MSFT", 20)]);

        let mut gateway = MockGateway {
            sent_orders: Vec::new(),
        };

        market.handle_raw_packet(
            &create_itch_packet('A', 10, 101, 100, 15000, 0, 'B', "AAPL"),
            &mut gateway,
        );
        market.handle_raw_packet(
            &create_itch_packet('A', 20, 201, 100, 30000, 0, 'B', "MSFT"),
            &mut gateway,
        );
        market.handle_raw_packet(
            &create_itch_packet('E', 10, 101, 40, 888, 0, ' ', "AAPL"),
            &mut gateway,
        );

        // Processor 0 = AAPL, Processor 1 = MSFT (based on setup order)
        let aapl = &market.processors[0];
        let msft = &market.processors[1];

        // AAPL was updated
        let (_, aapl_qty) = aapl.order_book.get_best_bid().unwrap();
        assert_eq!(aapl_qty, 60); // 100 - 40
        assert_eq!(aapl.ticker.trades.len(), 1);

        // MSFT remains untouched
        let (msft_price, msft_qty) = msft.order_book.get_best_bid().unwrap();
        assert_eq!(msft_price, 30000);
        assert_eq!(msft_qty, 100);
        assert_eq!(msft.ticker.trades.len(), 0);
    }

    #[test]
    fn test_modify_order_does_not_create_trade() {
        let mut market = setup_test_market("AAPL", 10);

        let mut gateway = MockGateway {
            sent_orders: Vec::new(),
        };

        market.handle_raw_packet(
            &create_itch_packet('A', 10, 100, 100, 5000, 0, 'B', "AAPL"),
            &mut gateway,
        );
        market.handle_raw_packet(
            &create_itch_packet('X', 10, 100, 30, 0, 0, ' ', "AAPL"),
            &mut gateway,
        );

        let processor = &market.processors[0];
        let best_bid = processor.order_book.get_best_bid();

        let (_, qty) = best_bid.expect("Bid should exist in AAPL book");
        assert_eq!(qty, 70);

        // Check ticker inside the processor
        assert_eq!(processor.ticker.trades.len(), 0);

        // Check our MockAnalytics writer inside market
        assert_eq!(market.telemetry_writer.messages.len(), 0);
    }

    #[test]
    fn test_execute_order_e_creates_trade_with_book_price() {
        let mut market = setup_multi_stock_market(vec![("AAPL", 10), ("MSFT", 20)]);

        let mut gateway = MockGateway {
            sent_orders: Vec::new(),
        };

        market.handle_raw_packet(
            &create_itch_packet('A', 10, 100, 60, 555, 0, 'B', "AAPL"),
            &mut gateway,
        );
        // Someone else sells into this bid for 40 shares
        market.handle_raw_packet(
            &create_itch_packet('E', 10, 100, 40, 999, 999, ' ', "AAPL"),
            &mut gateway,
        );

        let processor_appl = &market.processors[0];

        let (_, qty) = processor_appl
            .order_book
            .get_best_bid()
            .expect("Bid should exist.");

        assert_eq!(qty, 20);
        assert_eq!(processor_appl.ticker.trades.len(), 1);
        assert_eq!(processor_appl.ticker.trades[0].price, 555); // Price recovered from book    
        assert_eq!(processor_appl.ticker.trades[0].quantity, 40);
        assert_eq!(processor_appl.ticker.trades[0].match_number, 999);

        market.handle_raw_packet(
            &create_itch_packet('A', 20, 100, 60, 555, 0, 'B', "MSFT"),
            &mut gateway,
        );
        // Someone else sells into this bid for 40 shares
        market.handle_raw_packet(
            &create_itch_packet('E', 20, 100, 40, 999, 999, ' ', "MSFT"),
            &mut gateway,
        );

        let processor_msft = &market.processors[1];

        let (_, qty) = processor_msft
            .order_book
            .get_best_bid()
            .expect("Bid should exist.");

        assert_eq!(qty, 20);
        assert_eq!(processor_msft.ticker.trades.len(), 1);
        assert_eq!(processor_msft.ticker.trades[0].price, 555); // Price recovered from book    
        assert_eq!(processor_msft.ticker.trades[0].quantity, 40);
        assert_eq!(processor_msft.ticker.trades[0].match_number, 999);
    }

    #[test]
    fn test_execute_with_price_c_overrides_book_price() {
        let mut market = setup_multi_stock_market(vec![("AAPL", 10), ("MSFT", 20)]);

        let mut gateway = MockGateway {
            sent_orders: Vec::new(),
        };

        market.handle_raw_packet(
            &create_itch_packet('A', 10, 10, 1000, 500, 0, 'S', "AAPL"),
            &mut gateway,
        );
        //Trade happens at higher price up
        market.handle_raw_packet(
            &create_itch_packet('C', 10, 10, 1000, 1001, 9999, ' ', "AAPL"),
            &mut gateway,
        );

        let processor_appl = &market.processors[0];

        assert!(processor_appl.order_book.get_best_ask().is_none()); // Order fully consumed
        assert_eq!(processor_appl.ticker.trades[0].price, 1001); // Uses message price, not book price
        assert_eq!(processor_appl.ticker.trades[0].match_number, 9999);

        market.handle_raw_packet(
            &create_itch_packet('A', 20, 10, 1000, 500, 0, 'S', "MSFT"),
            &mut gateway,
        );

        //Trade happens at higher price up
        market.handle_raw_packet(
            &create_itch_packet('C', 20, 10, 1000, 1001, 9999, ' ', "MSFT"),
            &mut gateway,
        );

        let processor_msft = &market.processors[1];

        assert!(processor_msft.order_book.get_best_ask().is_none()); // Order fully consumed
        assert_eq!(processor_msft.ticker.trades[0].price, 1001); // Uses message price, not book price
        assert_eq!(processor_msft.ticker.trades[0].match_number, 9999);
    }

    #[test]
    fn test_delete_order_removes_from_book_only() {
        let mut market = setup_multi_stock_market(vec![("AAPL", 50), ("MSFT", 120)]);

        let mut gateway = MockGateway {
            sent_orders: Vec::new(),
        };

        market.handle_raw_packet(
            &create_itch_packet('A', 50, 100, 2500, 0, 0, 'B', "AAPL"),
            &mut gateway,
        );
        market.handle_raw_packet(
            &create_itch_packet('D', 50, 100, 2500, 0, 0, ' ', "AAPL"),
            &mut gateway,
        );

        let processor_appl = &market.processors[0];

        assert!(processor_appl.order_book.get_best_bid().is_none());
        assert_eq!(processor_appl.ticker.trades.len(), 0); // Deletes are not trades

        market.handle_raw_packet(
            &create_itch_packet('A', 120, 100, 2500, 0, 0, 'B', "MSFT"),
            &mut gateway,
        );
        market.handle_raw_packet(
            &create_itch_packet('D', 120, 100, 2500, 0, 0, ' ', "MSFT"),
            &mut gateway,
        );

        let processor_msft = &market.processors[1];

        assert!(processor_msft.order_book.get_best_bid().is_none());
        assert_eq!(processor_msft.ticker.trades.len(), 0);
    }

    #[test]
    fn test_non_displayable_trade_updates_ticker_only() {
        let mut market = setup_multi_stock_market(vec![("AAPL", 50), ("MSFT", 120), ("NVDA", 456)]);

        let mut gateway = MockGateway {
            sent_orders: Vec::new(),
        };

        market.handle_raw_packet(
            &create_itch_packet('P', 50, 100, 6000, 555, 0, 'S', "AAPL"),
            &mut gateway,
        );
        let processor_appl = &market.processors[0];
        assert!(processor_appl.order_book.get_best_bid().is_none()); // Book remains empty
        assert_eq!(processor_appl.ticker.total_volume, 6000);
        assert_eq!(processor_appl.ticker.last_sale, 555);

        market.handle_raw_packet(
            &create_itch_packet('P', 120, 100, 6000, 555, 0, 'S', "MSFT"),
            &mut gateway,
        );
        let processor_msft = &market.processors[1];
        assert!(processor_msft.order_book.get_best_bid().is_none());
        assert_eq!(processor_msft.ticker.total_volume, 6000);
        assert_eq!(processor_msft.ticker.last_sale, 555);

        market.handle_raw_packet(
            &create_itch_packet('P', 456, 100, 6000, 555, 0, 'S', "NVDA"),
            &mut gateway,
        );
        let processor_nvda = &market.processors[2];
        assert!(processor_nvda.order_book.get_best_bid().is_none());
        assert_eq!(processor_nvda.ticker.total_volume, 6000);
        assert_eq!(processor_nvda.ticker.last_sale, 555);
    }

    #[test]
    fn test_end_to_end_add_and_execute() {
        let mut market = setup_multi_stock_market(vec![("AAPL", 50), ("MSFT", 120)]);

        let mut gateway = MockGateway {
            sent_orders: Vec::new(),
        };

        market.handle_raw_packet(
            &create_itch_packet('A', 50, 100, 1_500_000, 100, 0, 'B', "AAPL"),
            &mut gateway,
        );

        {
            let processor_appl = &market.processors[0];

            let (p, q) = processor_appl
                .order_book
                .get_best_bid()
                .expect("Bid should exist");
            assert_eq!(q, 1_500_000);
            assert_eq!(p, 100);
        }

        market.handle_raw_packet(
            &create_itch_packet('E', 50, 100, 1_499_000, 0, 8888, ' ', "APPL"),
            &mut gateway,
        );

        let processor_appl = &market.processors[0];

        let (_, remaining_quantity) = processor_appl.order_book.get_best_bid().unwrap();
        assert_eq!(remaining_quantity, 1000);

        assert_eq!(processor_appl.ticker.trades.len(), 1);
        let trade = processor_appl.ticker.trades[0];
        assert_eq!(trade.price, 100); // For order executed, no price. Look up the existing order by ID get the price from the book
        assert_eq!(trade.quantity, 1_499_000); // 1,499_000 from order executed. This is the amount filled
        assert_eq!(trade.match_number, 8888); // match id from order executed 
        assert_eq!(trade.side, AggressorSide::Seller); // Seller hit the buy order
    }

    #[test]
    fn test_stragety_mid_price_calc() {
        let mut market = setup_multi_stock_market(vec![("AAPL", 50), ("MSFT", 120)]);

        let mut gateway = MockGateway {
            sent_orders: Vec::new(),
        };

        market.handle_raw_packet(
            &create_itch_packet('A', 50, 100, 1000, 80, 0, 'B', "AAPL"),
            &mut gateway,
        );
        market.handle_raw_packet(
            &create_itch_packet('A', 50, 100, 1100, 90, 0, 'S', "AAPL"),
            &mut gateway,
        );

        let processor_appl = &market.processors[0];

        assert_eq!(processor_appl.order_book.get_best_bid(), Some((80, 1000)));
        assert_eq!(processor_appl.order_book.get_best_ask(), Some((90, 1100)));

        assert_eq!(processor_appl.strategy.mid_price, 85.0);

        market.handle_raw_packet(
            &create_itch_packet('A', 120, 100, 1000, 200, 0, 'B', "MSFT"),
            &mut gateway,
        );
        market.handle_raw_packet(
            &create_itch_packet('A', 120, 100, 1100, 300, 0, 'S', "MSFT"),
            &mut gateway,
        );

        let processor_msft = &market.processors[1];

        assert_eq!(processor_msft.order_book.get_best_bid(), Some((200, 1000)));
        assert_eq!(processor_msft.order_book.get_best_ask(), Some((300, 1100)));

        assert_eq!(processor_msft.strategy.mid_price, 250.0);
    }

    #[test]
    fn test_strategy_lifecycle_buy_and_position_lock() {
        let mut market = setup_multi_stock_market(vec![("AAPL", 10), ("MSFT", 120)]);

        let mut gateway = MockGateway {
            sent_orders: Vec::new(),
        };

        // Best Bid: 10,000 shares 150.00
        // Best Ask: 100 shares 150.10
        market.processors[0].strategy.mid_price = 14990.0;

        // Send Add Order for Bid
        market.handle_raw_packet(
            &create_itch_packet('A', 10, 101, 10000, 15000, 0, 'B', "AAPL"),
            &mut gateway,
        );
        // Send Add Order for Ask - This will trigger the strategy
        market.handle_raw_packet(
            &create_itch_packet('A', 10, 102, 100, 15010, 0, 'S', "AAPL"),
            &mut gateway,
        );

        let processor = &mut market.processors[0];

        {
            let open_orders: Vec<u64> = processor.order_tracker.iter().copied().collect();

            for oid in open_orders {
                // Give the strategy its 100 shares
                processor
                    .strategy
                    .on_trade(15010, 100, AggressorSide::Buyer);
                processor.order_tracker.remove(&oid);
            }
        }

        assert_eq!(
            processor.strategy.position, 100,
            "Position should have updated to 100"
        );
        assert_eq!(
            processor.strategy.trade_count, 1,
            "Should have recorded 1 trade"
        );

        // Send another Ask order at the same price. Even though the "Imbalance" is still there,
        // the strategy should return None because it already has a position of 100.
        market.handle_raw_packet(
            &create_itch_packet('A', 10, 103, 100, 15010, 0, 'S', "AAPL"),
            &mut gateway,
        );

        let processor_after = &market.processors[0];
        assert_eq!(
            processor_after.strategy.trade_count, 1,
            "Trade count should not increase due to position guard"
        );
        assert_eq!(
            processor_after.strategy.position, 100,
            "Position should remain 100"
        );
    }

    /*
    Setup: Send create_itch_packet with Type 'A' (Add Order), Side 'S' (Sell), Price 15100, and Qty 1000.
    It stayed there for the entire duration of the test
    Bid quantity keep increasing
    'A' Messages are cumulative: 'A' message adds a new order to the book. It doesn't replace the old one.
    By the 51st iteration, you hadn't just sent 9000 shares; you had told the book there were 51 different people each wanting 9000 share
    The sell wall stayed at 1000, while the buy wall grew to 459,000.
    The imbalance was effectively 459,000 / (Successfully mapped AAPL to stock,000 + 1000) = 0.997.

    Summary : For every loop we created an imbalance and becuase we hardcoded 100 it took 51 itertations to make hard coded limit of 5000

    */
    #[test]
    fn test_market_handler_risk_saturation_full_itch() {
        let symbol = "AAPL";
        let locate = 1;

        let mut market = setup_test_market("AAPL", 1);

        let mut gateway = MockGateway {
            sent_orders: Vec::new(),
        };

        market.processors[0].risk_engine.max_absolute_position = 5000;

        let ask_packet = create_itch_packet('A', locate, 100, 1000, 15100, 0, 'S', symbol);
        market.handle_raw_packet(&ask_packet, &mut gateway);

        for i in 1..=51 {
            let bid_packet = create_itch_packet(
                'A',
                locate,
                (200 + i) as u64,
                9000,
                15000 + i as u32, // <--- Increment price: 15001, 15002, 15003...
                0,
                'B',
                symbol,
            );

            market.handle_raw_packet(&bid_packet, &mut gateway);

            let processor = &mut market.processors[0];
            let open_orders: Vec<u64> = processor.order_tracker.iter().copied().collect();

            for oid in open_orders {
                // Give the strategy its shares
                processor
                    .strategy
                    .on_trade(15000, 100, AggressorSide::Buyer);
                // Clear the tracker just like polling execution engine would
                processor.order_tracker.remove(&oid);
            }
        }

        let processor = &mut market.processors[0];

        // Risk Engine should be exactly 5000
        assert_eq!(
            processor.risk_engine.current_position, 5000,
            "Risk engine failed! It allowed {} shares through.",
            processor.risk_engine.current_position
        );

        // Strategy must be in sync (not 6000)
        assert_eq!(
            processor.strategy.position, 5000,
            "Strategy and Risk Engine are out of sync!"
        );

        println!("Integration Test Success");
    }

    //test to Rust order => C++ execute engine ==> Simulator exchange ==> C++ execute engine ==> Rust.
    /*
    To test system end-to-end, we had to trick  SimpleMidPoint strategy into generating a BUY signal exactly 15 times in a row
    Strategy triggers when the buy pressure is overwhelming
    Ask Packet Size: 100 shares.
    Bid Packet Size: 10,000 shares.
    Every single time the loop runs, the strategy looks at the book and sees a 99% buy-side imbalance (10000 / 10100). Buy on every tick
    We didn't want the bid and ask to cross each other, so we locked the spread at exactly 10 ticks

     */
    //#[test] not a real unit test - used to test Live end to end flow.
    #[allow(dead_code)]
    fn test_end_to_end() {
        let symbol = "AAPL";
        let locate = 10;

        let mut market = setup_test_market(symbol, locate);

        let mut gateway = MockGateway {
            sent_orders: Vec::new(),
        };

        println!("Starting execution engine shared memory test.");

        for i in 0..15 {
            // Move Ask up
            let ask_packet =
                create_itch_packet('A', locate, 100 + i, 100, 15000 + i as u32, 0, 'S', symbol);
            market.handle_raw_packet(&ask_packet, &mut gateway);

            // Move Bid up
            let bid_packet = create_itch_packet(
                'A',
                locate,
                (1000 + i) as u64,
                10000,
                14990 + i as u32,
                0,
                'B',
                symbol,
            );

            // This triggers the order AND checks poll_confirmed_orders()
            market.handle_raw_packet(&bid_packet, &mut gateway);

            // Because the network is slower than the CPU, we must spin a little bit
            // to allow C++ to talk to Python and write the fill back to us.

            //flow here:
            /*
            Rust: Parses the ITCH packet and updates the L3OrderBook.
            Rust: Strategy sees the imbalance - buy".
            Rust: Writes the 64-byte OrderCommand into the Outbound Shared Memory.
            Rust: Adds the Order ID to order_tracker.
            Rust: Reaches the bottom of handle_raw_packet and calls poll_confirmed_orders().
            Rust: poll_confirmed_orders() looks at the Inbound Shared Memory. It is completely empty. 
            Time elapsed: ~150 nanoseconds

            It will take C++ and Python about 500 microseconds to route the TCP packets 
            back and forth. That is thousands of times slower than Rust.

            If we didn't have this  while loop, Rust would finish handle_raw_packet in less than a 
            microsecond, instantly jump to the next for i in 0..15 iteration, and fire the next order. 
            It would blast all 15 orders in a fraction of a millisecond and shut down the test 
            before C++ and Python even processed the first one.

            The while loop acts as a pause button for the test.  "Do not move on to the next ITCH 
            packet until the network round-trip actually finishes and C++ drops the fill into our inbox."
            */

            while market.processors[0].order_tracker.len() > 0 {
                market.poll_confirmed_orders(&mut gateway);
                std::hint::spin_loop();
            }
        }

        println!("Finished execution engine shared mmenory test.");
    }
}
