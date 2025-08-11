use std::fs::File;
use std::io::Write;
use std::thread;
use std::{fs, time::Duration};
use std::collections::HashMap;
use regex::Regex;
use std::time::SystemTime;

use crate::utils::ip_helper::{get_dev_from_ip};

type MACQueueInfo = HashMap<u8, u64>;

fn parse_digits(s: &str, start: usize) -> Option<u64> {
    let bytes = s.as_bytes();
    let mut num: u64 = 0;
    let mut found = false;
    
    // Process each byte from start position
    for &b in bytes.iter().skip(start) {
        match b {
            b'0'..=b'9' => {
                found = true;
                // Check for overflow during calculation
                num = num
                    .checked_mul(10)?
                    .checked_add((b - b'0') as u64)?;
            }
            _ => break, // Stop at first non-digit
        }
    }
    
    found.then_some(num)
}
pub struct MACQueueQuery {
    proc_file: String,
    queue_info: MACQueueInfo,
}

impl MACQueueQuery {
    pub fn new(dev: &str) -> Self {
        Self {
            proc_file: format!("/proc/net/rtl88XXau/{}/mac_qinfo", dev),
            queue_info: HashMap::new(),
        }
    }

    fn update_queue_info(&mut self){
        // Clear the existing queue info
        self.queue_info.clear();

        // Read the contents of the proc file
        let data = fs::read_to_string(&self.proc_file).expect("Failed to read PROC_FILE");

        // Regex to match lines with "pkt_num" and "ac" (excluding BCN)
        // let re = Regex::new(r"head:[^,]+,\s+tail:[^,]+,\s+pkt_num:(\d+),\s+macid:\d+,?\s*ac:(\d+)").unwrap();
    
        for line in data.lines() {
            // Fast skip BCN lines (as per comment requirement)
            if line.contains("BCN") {
                continue;
            }
        
            // First find pkt_num marker
            if let Some(p_pos) = line.find("pkt_num:") {
                // Only look for ac AFTER pkt_num
                let after_pkt = &line[p_pos + 8..];
                if let Some(a_pos) = after_pkt.find("ac:") {
                    // Calculate absolute position in line
                    let a_abs = p_pos + 8 + a_pos;
                    
                    // Extract both values with direct byte parsing
                    if let (Some(pkt_num), Some(ac_val)) = (
                        parse_digits(&line, p_pos + 8),
                        parse_digits(&line, a_abs + 3)
                    ) {
                        // Handle u8 overflow same as original (unwrap_or(0))
                        let ac_val = if ac_val > u64::from(u8::MAX) { 0 } else { ac_val as u8 };
                        *self.queue_info.entry(ac_val).or_insert(0) += pkt_num;
                    }
                }
            }
        }
    }

    pub fn get_queue_info(&mut self) -> &MACQueueInfo {
        &self.queue_info
    }
    
}

pub struct MACQueueMonitor {
    query: HashMap<String, MACQueueQuery>,
}

impl MACQueueMonitor {
    pub fn new(ips: &Vec<String>) -> Self {
        let mut query = HashMap::new();
        ips.into_iter().for_each(|ip| {
            if let Some(dev) = get_dev_from_ip(ip) {
                query.insert(ip.clone(), MACQueueQuery::new(&dev));
            }
        });

        Self {
            query
        }
    }

    pub fn get_queue_info_by_ip(&mut self, ip: &str) -> Option<&MACQueueInfo> {
        if let Some(dev) = get_dev_from_ip(ip) {
            if !self.query.contains_key(ip) {
                self.query.insert(ip.to_string(), MACQueueQuery::new(&dev));
                self.query.get_mut(ip).unwrap().update_queue_info();
            }
            self.query.get_mut(ip).map(|query| {
                query.get_queue_info()
            })
        }
        else {
            None
        }
    }

}

pub fn mon_mac_thread(
    mut mac_mon: MACQueueMonitor,
) -> thread::JoinHandle<()> {
    let mon_thread = thread::spawn(move || {
        let spin_sleeper = spin_sleep::SpinSleeper::new(100_000)
            .with_spin_strategy(spin_sleep::SpinStrategy::YieldThread);

        let mut counter = 0;
        let mut logger = File::create( "logs/mac-info.txt" ).unwrap();

        let mut log_line = String::new();
        loop {
            // spin_sleeper.sleep(Duration::from_nanos(100_000));
            counter += 1;

            let current_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64();

            mac_mon.query.iter_mut().for_each(|(_ip, query)| {
                query.update_queue_info();
                // Capture the MAC queue info along with timestamp
                log_line.push_str(&format!(
                    "{:?} - {:?}\n",
                    current_time,
                    query.get_queue_info(),
                    
                ));
            });

            // Write to file every 1000 iterations or after 1 second
            if counter % 1000 == 0 {
                logger.write_all(log_line.as_bytes())
                    .expect("Failed to write to log file");
                log_line.clear();
            }
        }
    });
    mon_thread
}