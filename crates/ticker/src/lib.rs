use common::AggressorSide;

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct Trade {
    pub price: u64,
    pub quantity: u32,
    pub match_number: u64, // unique id for trade i.e. John sold to Jane
    //NASDAQ generates it. It is included in Trade, Order Executed (with Prce) message.
    //If an order is partially filled 5 times, it will have 1 Order ID but 5 different Match Numbers.
    //This allows you to track exactly which "fill" is which
    pub side: AggressorSide,
}
pub struct TradeTicker {
    pub trades: Vec<Trade>,
    pub total_volume: u64,
    pub last_sale: u64,
}

impl TradeTicker {
    pub fn new() -> Self {
        Self {
            trades: Vec::with_capacity(1000),
            total_volume: 0,
            last_sale: 0,
        }
    }

    pub fn add_trade(&mut self, price: u64, quantity: u32, match_number: u64, side: AggressorSide) {
        let t = Trade {
            price,
            quantity,
            match_number,
            side,
        };

        self.last_sale = price;
        self.total_volume += quantity as u64;
        self.trades.push(t); //dont do this..unbounded vector growth..heap allocations...

        //should be pushed via ring buffer for consumption...
        let _bytes = unsafe {
            std::slice::from_raw_parts(
                (&t as *const Trade) as *const u8,
                std::mem::size_of::<Trade>(),
            )
        };
    }
}
