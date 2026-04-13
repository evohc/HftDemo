use std::{path::Path, thread, time::Duration};

use common::{AggressorSide, TelemetryPayload};
use reqwest::blocking::Client;
use shm_ring_buffer::{ReadResult, ShmRing};

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct Trade {
    pub price: u64,
    pub quantity: u32,
    pub match_number: u64,
    pub side: AggressorSide,
}

#[inline]
fn flush_to_influx(client: &Client, url: &str, buffer: &mut String, count: &mut usize) {
    match client.post(url).body(buffer.clone()).send() {
        Ok(res) => {
            let status = res.status();

            if status.is_success() {
                println!("Success sending data to InFluxDb: {}.", status);
            } else {
                eprintln!("Error sending data to InFluxDb: {}.", status);
            }
        }
        Err(e) => {
            eprintln!("Network failur/timeout: {}", e)
        }
    }

    buffer.clear();
    *count = 0;
}

fn main() -> anyhow::Result<()> {
    println!("Starting visualiser engine.");

    let core_ids = core_affinity::get_core_ids().expect("Failed to read CPU cores.");
    if core_ids.len() > 1 {
        core_affinity::set_for_current(core_ids[1]);
        println!("Visualiser engine pinned to Core 1.");
    } else {
        println!("Cant grab a core, continue...");
    }

    let producer_core = core_ids[1];

    core_affinity::set_for_current(producer_core);

    println!("Wait 60 secs so that hft_telemetry is created.");

    let mut file_exists = false;
    for _ in 1..50 {
        if Path::new("/dev/shm/hft_telemetry").exists() {
            file_exists = true;
            break;
        }

        thread::sleep(Duration::from_secs(1));
    }

    if !file_exists {
        panic!("No hft_telemetry file exists...")
    } else {
        println!("hft_telemetry created.")
    }

    thread::sleep(Duration::from_millis(500));

    let ring: ShmRing = ShmRing::create_consumer("/dev/shm/hft_telemetry", 64 * 1024 * 1024)?; //64MB
    let mut sequence_num = 0;

    let client = Client::builder().timeout(Duration::from_secs(1)).build()?;
    let influx_url = "http://localhost:8086/write?db=hft";

    let mut batch_buffer = String::with_capacity(1024 * 1024);
    let mut records_in_batch = 0;
    const BATCH_SIZELIMIT: usize = 5000;

    loop {
        match ring.read(sequence_num) {
            ReadResult::Success(slot) => {
                let payload: TelemetryPayload =
                    unsafe { std::ptr::read(slot.data.as_ptr() as *const TelemetryPayload) };

                //read from some sort of config cache
                let symbol = if payload.locate_id == 13 {
                    "AAPL"
                } else {
                    "MSFT"
                };

                let line = format!(
                    "strategy_state,symbol={} price={},position={},pnl={},latency={} {}\n",
                    symbol,
                    payload.midpoint_px,
                    payload.position,
                    payload.realised_pnl,
                    payload.latency_ns,
                    payload.timestamp_ns
                );

                batch_buffer.push_str(&line);

                records_in_batch += 1;
                sequence_num += 1;

                if records_in_batch > BATCH_SIZELIMIT {
                    println!("fdsffds");

                    flush_to_influx(
                        &client,
                        influx_url,
                        &mut batch_buffer,
                        &mut records_in_batch,
                    );
                }
            }
            ReadResult::Empty => {
                //std::hint::spin_loop(); cant spin here choking CPU for docker containers
                if records_in_batch > 0 {
                    flush_to_influx(
                        &client,
                        influx_url,
                        &mut batch_buffer,
                        &mut records_in_batch,
                    );
                }

                std::thread::sleep(std::time::Duration::from_millis(1)); //yield back with system call.
            }
            ReadResult::Reset => {
                println!("Producer stopped/crashed...reset");
                sequence_num = 0;
                batch_buffer.clear();
                records_in_batch = 0;
            }
            ReadResult::Overlapped(new_seq_num) => {
                println!(
                    "Overlap has occurred, jump from {} to {}.",
                    sequence_num, new_seq_num
                );
                sequence_num = new_seq_num;
            }
            ReadResult::Interrupted(new_seq_num) => {
                println!(
                    "Interrupt has occurred, jump from {} to {}.",
                    sequence_num, new_seq_num
                );
                sequence_num = new_seq_num;
            }
        }
    }
}
