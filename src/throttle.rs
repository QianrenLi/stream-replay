use std::time::SystemTime;
use std::fs::File;
use std::io::prelude::*;
use std::collections::VecDeque;
use crate::packet::PacketStruct;

type TIME = SystemTime;
type SIZE = usize;

struct SlidingWindow<T> {
    size: usize,
    pub window: VecDeque<T>
}

impl<T> SlidingWindow<T>
where T:Sized + Copy
{
    pub fn new(size: usize) -> Self {
        let window = VecDeque::with_capacity(size);
        Self{ size, window }
    }

    pub fn push(&mut self, item: T) {
        if self.window.len()==self.size {
            self.window.pop_front();
        }
        self.window.push_back(item);
    }
}

pub struct RateThrottler {
    pub name: String,
    logger: Option<File>,
    window: SlidingWindow<(TIME, SIZE)>,
    buffer: VecDeque<PacketStruct>,
    pub throttle: f64,
}

impl RateThrottler {
    pub fn new(name:String, throttle: f64, window_size:usize, no_logging:bool) -> Self {
        let buffer = VecDeque::new();
        let logger = match no_logging {
            false => Some(File::create( format!("logs/log-{}.txt", name) ).unwrap()),
            true => None
        };
        let window = SlidingWindow::new(window_size);
        Self{ name, logger, window, buffer, throttle }
    }

    pub fn current_rate_mbps(&self, extra_bytes:Option<usize>) -> Option<f64> {
        let acc_size: usize = self.window.window.iter().map(|&x| x.1).sum();
        let acc_size = acc_size  + extra_bytes.unwrap_or(0);

        let _last_time = self.window.window.get(0)?.0;
        let acc_time = SystemTime::now().duration_since( _last_time ).unwrap();
        let acc_time = acc_time.as_nanos();

        let average_rate_mbps = 8.0 * (acc_size as f64/1e6) / (acc_time as f64*1e-9);
        Some(average_rate_mbps)
    }

    pub fn prepare(&mut self, packets: Vec<PacketStruct>) {
        let timestamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64();
        let _rate_mbps = self.current_rate_mbps(None).unwrap_or(0.0);
        if let Some(ref mut logger) = self.logger {
            logger.write_all( format!("{:.9} {} {:.6}\n",
            timestamp, self.buffer.len(), _rate_mbps ).as_bytes() ).unwrap();
        }
        for packet in packets.into_iter() {
            self.buffer.push_back(packet);
        }
    }

    pub fn try_consume<T>(&mut self, callback:T) -> Option<bool>
    where T: Fn(PacketStruct) -> bool {
        match self.buffer.front().cloned() {
            None => None,
            Some(packet) => {
                if self.exceeds_with(packet.length as usize) {
                    std::thread::sleep( std::time::Duration::from_nanos(100_000) );
                    return Some(false);
                }
                match callback(packet) {
                    true => {
                        self.consume();
                        Some(true)
                    }
                    false => Some(false)
                }
            }
        }
    }

    pub fn consume(&mut self) -> Option<PacketStruct> {
        let timestamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64();
        let _rate_mbps = self.current_rate_mbps(None).unwrap_or(0.0);
        if let Some(ref mut logger) = self.logger {
            logger.write_all( format!("{:.9} {} {:.6}\n",
            timestamp, self.buffer.len(), _rate_mbps ).as_bytes() ).unwrap();
        }
        self.buffer.pop_front()
    }

    pub fn exceeds_with(&mut self, size_bytes:usize) -> bool {
        if self.throttle==0.0 || self.window.window.len()==0 {
            self.window.push(( SystemTime::now(), size_bytes ));
            return false;
        }

        let average_rate_mbps = self.current_rate_mbps( Some(size_bytes) );
        if average_rate_mbps.unwrap() < self.throttle {
            self.window.push(( SystemTime::now(), size_bytes ));
            false
        }
        else {
            true
        }
    }
}
