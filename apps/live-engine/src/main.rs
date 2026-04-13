use common::{ExecutionReport, MessageAnalyticsWriter, OrderCommand};
use market_handler::{ExchangeGateway, MarketState};
use shm_ring_buffer::{ReadResult, ShmRing};
use std::net::UdpSocket;
use strategies::SimpleMidPoint;

pub struct DummyWriter;
impl MessageAnalyticsWriter for DummyWriter {
    fn write(&mut self, _data: &[u8]) {}
}

pub struct ShmGateway {
    outbound_shm: ShmRing,
    inbound_shm: ShmRing,
    next_seq: u64,
}

impl ShmGateway {
    pub fn new() -> Self {
        let outbound_shm = ShmRing::create_producer("/dev/shm/hft_execution", 64 * 1024 * 1024)
            .expect("Fatal: Could not allocate Outbound SHM");

        let inbound_shm = ShmRing::create_consumer("/dev/shm/hft_exchange_info", 64 * 1024 * 1024)
            .expect("Fatal: Could not allocate Inbound SHM");

        Self {
            outbound_shm,
            inbound_shm,
            next_seq: 0,
        }
    }
}

impl ExchangeGateway for ShmGateway {
    fn send_order(&mut self, cmd: &OrderCommand) {
        self.outbound_shm.write(&cmd.as_bytes());
    }

    fn poll_confirmed_orders(&mut self) -> Option<ExecutionReport> {
        match self.inbound_shm.read(self.next_seq) {
            ReadResult::Success(slot) => {
                let report = unsafe {
                    std::ptr::read_unaligned(slot.data.as_ptr() as *const ExecutionReport)
                };
                self.next_seq += 1;
                Some(report)
            }
            ReadResult::Empty => None,
            ReadResult::Reset | ReadResult::Overlapped(_) | ReadResult::Interrupted(_) => {
                panic!("Fatal Error reading from Inbound SHM");
            }
        }
    }

    fn get_position(&self, _stock_locate: u16) -> i32 {
        0
    }
    fn get_cash_balance(&self, _stock_locate: u16) -> i64 {
        0
    }
}

fn main() {
    println!("Starting live engine.");

    let core_ids = core_affinity::get_core_ids().expect("Failed to read CPU cores.");
    if core_ids.len() > 2 {
        core_affinity::set_for_current(core_ids[2]);
        println!("Live engine pinned to Core 2.");
    } else {
        println!("Cant grab a core, continue...");
    }

    let producer_core = core_ids[2];

    core_affinity::set_for_current(producer_core);

    let mut gateway = ShmGateway::new();
    let writer = DummyWriter;
    let strategy = SimpleMidPoint::new(0.8);

    let mut market = MarketState::new(
        vec!["AAPL".to_string(), "MSFT".to_string()],
        strategy,
        writer,
    );

    //Note : this is already tested by test_end_to_end "unit test" however I will add a python script to generate a few packets
    // to trigger an order.

    //note in a real system you are going to bypass the entire TCP/IP stack, see the burst detector project.
    let socket = UdpSocket::bind("127.0.0.1:8080").expect("Failed to bind UDP socket");
    socket
        .set_nonblocking(true)
        .expect("Failed to set non-blocking");

    println!("Engine Live. Listening for UDP ITCH packets on 127.0.0.1:8080...");

    let mut buf = [0u8; 1024];

    loop {
        match socket.recv_from(&mut buf) {
            Ok((size, _src)) => {
                market.handle_raw_packet(&buf[..size], &mut gateway);
            }
            Err(e) => {
                if e.kind() == std::io::ErrorKind::WouldBlock {
                    // Poll the C++ gateway to check for fills!
                    market.poll_confirmed_orders(&mut gateway);
                } else {
                    eprintln!("UDP Error: {}", e);
                }
            }
        }

        std::hint::spin_loop();
    }
}
