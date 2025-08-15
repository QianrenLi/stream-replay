#![allow(dead_code)]

use std::sync::{Arc, Mutex};
use std::thread;
use std::{fs};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};

use crate::utils::ip_helper::{get_dev_from_ip};

pub type MACQueueInfo = HashMap<u8, usize>;
pub type GuardedMACMonitor = Arc<Mutex<MACQueueMonitor>>;

#[derive(Debug, Clone)]
pub struct MACQueueQuery {
    proc_file: String,
    queue_info: MACQueueInfo,
}

#[derive(Debug, Clone)]
pub struct MACQueueMonitor {
    query: HashMap<String, MACQueueQuery>,
}

fn parse_digits(s: &str, start: usize) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut num: usize = 0;
    let mut found = false;
    
    // Process each byte from start position
    for &b in bytes.iter().skip(start) {
        match b {
            b'0'..=b'9' => {
                found = true;
                // Check for overflow during calculation
                num = num
                    .checked_mul(10)?
                    .checked_add((b - b'0') as usize)?;
            }
            _ => break, // Stop at first non-digit
        }
    }
    
    found.then_some(num)
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
                        let ac_val = if ac_val > usize::from(u8::MAX) { 0 } else { ac_val as u8 };
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

    pub fn get_ac_queue(&mut self, ac: u8) -> Vec<usize> {
        // Collect all the queue values for the given `ac` from multiple IPs
        self.query.iter_mut().map(|(_ip, query)| {
            // Return the value for `ac`, defaulting to 0 if not found
            query.get_queue_info().get(&ac).cloned().unwrap_or(0)
        }).collect() // Collect all found values into a Vec<usize>
    }
    
}

pub fn mon_mac_thread(
    mac_mon: GuardedMACMonitor,
) -> thread::JoinHandle<()> {
    let mon_thread = thread::spawn(move || {
        let spin_sleeper = spin_sleep::SpinSleeper::new(100_000)
            .with_spin_strategy(spin_sleep::SpinStrategy::YieldThread);

        loop {
            let deadline = SystemTime::now() + Duration::from_nanos(300_000_000); // 300ms           
            match mac_mon.lock() {
                Ok(mut mac_mon) => {
                    // Update the queue info for each IP
                    mac_mon.query.iter_mut().for_each(|(_ip, query)| {
                        query.update_queue_info();
                    });
                }
                Err(e) => {
                    eprintln!("Failed to lock MACQueueMonitor: {}", e);
                    continue;
                }
            }

            if let Ok(remaining_time) = deadline.duration_since(SystemTime::now()) {
                spin_sleeper.sleep(remaining_time);
            }
        }
    });
    mon_thread
}