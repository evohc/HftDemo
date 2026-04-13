pub type Price = u32; // ITCH prices are 4-byte integers (Price 4)
pub type Quantity = u32; // ITCH quantities are 4-byte integers
pub type OrderId = u64; // ITCH Order Reference Numbers are 8 bytes

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AggressorSide {
    Buyer,
    Seller,
}

#[repr(C)] // Ensures memory layout is consistent for Shared Memory
#[derive(Debug, Clone, Copy)]
pub struct Trade {
    pub stock_locate: u16,
    pub price: Price,
    pub quantity: Quantity,
    pub match_number: u64,
    pub side: AggressorSide,
}

//see how mocking works in Rust...
pub trait MessageAnalyticsWriter {
    fn write(&mut self, data: &[u8]);
}

#[repr(C)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub enum OrderSide {
    Buy = 0,
    Sell = 1,
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct ExecutionReport {
    pub order_id: u64,
    pub last_qty: u32,
    pub last_price: u32,
    pub exec_type: u8,
    pub stock_locate: u16,
}

#[repr(C, packed)]
#[derive(Debug, Copy, Clone, PartialEq)]
pub struct OrderCommand {
    pub symbol_locate: u16,
    pub order_id: u64,
    pub price: u32,
    pub quantity: u32,
    pub side: OrderSide,
}

impl OrderCommand {
    pub fn as_bytes(&self) -> &[u8] {
        unsafe {
            std::slice::from_raw_parts(
                (self as *const Self) as *const u8,
                std::mem::size_of::<Self>(),
            )
        }
    }
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct TelemetryPayload {
    pub timestamp_ns: u64,
    pub realised_pnl: i64,
    pub midpoint_px: u32,
    pub position: i32,
    pub latency_ns: u32,
    pub locate_id: u16,
}
