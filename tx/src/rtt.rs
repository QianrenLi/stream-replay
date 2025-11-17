use std::fs::File;
use std::io::{prelude::*, BufWriter};
use std::net::UdpSocket;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};

use crate::statistic::rtt_records::RttRecords;

pub type GuardedRttRecords = Arc<Mutex<RttRecords>>;
static PONG_PORT_INC: u16 = 1024;

pub fn now_secs_f64() -> f64 {
    SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64()
}

pub struct RttRecorder {
    recv_handle: Option<JoinHandle<()>>,
    name: String,
    port: u16,
    pub rtt_records: GuardedRttRecords,
}

impl RttRecorder {
    pub fn new(name: &String, port: u16, _mul_link_num: usize, target_rtt: f64) -> Self {
        let name = name.clone();
        let port = port + PONG_PORT_INC; // pong recv port
        let rtt_records = Arc::new(Mutex::new(RttRecords::new(200, target_rtt)));

        RttRecorder {
            name,
            port,
            recv_handle: None,
            rtt_records,
        }
    }

    /// Start only the RX (pong) thread.
    pub fn start(&mut self, tx_ipaddr: String) {
        let name = self.name.clone();
        let port = self.port;
        let rtt_for_rx = Arc::clone(&self.rtt_records);

        self.recv_handle = Some(thread::spawn(move || {
            pong_recv_thread(name, port, rtt_for_rx, tx_ipaddr);
        }));
    }
}

fn pong_recv_thread(
    name: String,
    port: u16,
    rtt_records: GuardedRttRecords,
    tx_ipaddr: String,
) {
    let mut buf = [0u8; 2048];
    let sock = UdpSocket::bind(format!("{}:{}", tx_ipaddr, port)).unwrap();
    // Avoid tiny packets coalescing delays on some stacks
    let _ = sock.set_read_timeout(Some(Duration::from_millis(200)));

    let mut logger = {
        let f = File::create(format!("logs/rtt-{}.txt", name)).ok();
        f.map(BufWriter::new)
    };

    loop {
        match sock.recv_from(&mut buf) {
            Ok((_n, _addr)) => {
                // seq: [0..4], delta: [19..27]
                let seq = u32::from_le_bytes(buf[..4].try_into().unwrap());
                let delta = f64::from_le_bytes(buf[19..27].try_into().unwrap());
                let pong_time = now_secs_f64();

                // Update records (short lock)
                let rtt = {
                    let mut rec = rtt_records.lock().unwrap();
                    rec.update(seq as usize, pong_time, delta)
                };

                if let Some(ref mut w) = logger {
                    // keep allocations out of the lock
                    let _ = writeln!(w, "{} {:.6} {:.6}", seq, rtt, delta);
                }
            }
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // timeout: continue
                continue;
            }
            Err(_) => break,
        }
    }
}
