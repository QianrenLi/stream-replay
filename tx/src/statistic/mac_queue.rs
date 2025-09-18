#![allow(dead_code)]
use std::sync::{Arc, Mutex};
use std::process::Command;
use std::thread;
use std::{fs, io};
use std::collections::HashMap;
use std::time::{Duration, SystemTime};
use arc_swap::ArcSwap;
use serde::{Deserialize, Serialize};
use regex::Regex;

use crate::utils::ip_helper::{get_dev_from_ip};

pub type MACQueueInfo = HashMap<u8, usize>;
pub type GuardedMACMonitor = Arc<Mutex<MACQueueMonitor>>;

#[derive(Debug, Clone)]
pub struct MACQueueMonitor {
    query: HashMap<String, MACQueueQuery>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MACQueuesSnapshot {
    pub taken_at: SystemTime,
    pub queues: HashMap<String, MACQueueInfo>,
    pub link: HashMap<String, LinkInfo>,
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

#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct LinkInfo {
    bssid: Option<String>,
    ssid: Option<String>,
    freq_mhz: Option<u32>,
    signal_dbm: Option<i32>,
    pub tx_mbit_s: Option<f32>,
}

fn run_iw_link(iface: &str) -> Option<String> {
    let out = Command::new("iw")
        .arg("dev")
        .arg(iface)
        .arg("link")
        .output().unwrap();

    if !out.status.success() {
        None
    }
    else{
        Some(String::from_utf8_lossy(&out.stdout).to_string())
    }
}

fn parse_link_info(s: &str) -> LinkInfo {
    let re_bssid = Regex::new(r"(?m)^\s*Connected to\s+([0-9a-fA-F:]{17})\b").unwrap();
    let re_ssid  = Regex::new(r"(?m)^\s*SSID:\s*(.+)\s*$").unwrap();
    let re_freq  = Regex::new(r"(?m)^\s*freq:\s*(\d+)\s*$").unwrap();
    let re_sig   = Regex::new(r"(?m)^\s*signal:\s*(-?\d+)\s*dBm\b").unwrap();
    let re_tx    = Regex::new(r"(?m)^\s*tx bitrate:\s*([0-9]+(?:\.[0-9]+)?)\s*MBit/s\b").unwrap();

    let bssid = re_bssid.captures(s).map(|c| c[1].to_string());
    let ssid  = re_ssid.captures(s).map(|c| c[1].trim().to_string());
    let freq_mhz = re_freq
        .captures(s)
        .and_then(|c| c[1].parse::<u32>().ok());
    let signal_dbm = re_sig
        .captures(s)
        .and_then(|c| c[1].parse::<i32>().ok());
    let tx_mbit_s = re_tx
        .captures(s)
        .and_then(|c| c[1].parse::<f32>().ok());

    LinkInfo { bssid, ssid, freq_mhz, signal_dbm, tx_mbit_s }
}

#[derive(Debug, Clone)]
pub struct MACQueueQuery {
    dev: String,
    proc_file: String,
    queue_info: MACQueueInfo,
    link_info: LinkInfo,
}

impl MACQueueQuery {
    pub fn new(dev: &str) -> Self {
        Self {
            dev: dev.to_string(),
            proc_file: format!("/proc/net/rtl88XXau/{}/mac_qinfo", dev),
            queue_info: HashMap::new(),
            link_info: LinkInfo::default(),
        }
    }

    fn update_queue_info(&mut self){
        // Read the contents of the proc file
        let data = match fs::read_to_string(&self.proc_file) {
            Ok(s) => s,
            Err(e) if e.kind() == io::ErrorKind::NotFound => {
                // device not up or driver not exposing the file yet — just pass
                return;
            }
            Err(e) => {
                eprintln!("Failed to read {}: {}", self.proc_file, e);
                return;
            }
        };

        // Clear the existing queue info
        self.queue_info.clear();

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

    fn update_link_info(&mut self) {
        if let Some(output) = run_iw_link(&self.dev) {
            self.link_info = parse_link_info(&output);
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

#[derive(Debug, Clone)]
pub struct LatestBus {
    inner: Arc<ArcSwap<MACQueuesSnapshot>>,
}

impl LatestBus {
    pub fn new() -> Self {
        // Start with an empty snapshot if you like:
        let init = Arc::new(MACQueuesSnapshot {
            taken_at: std::time::SystemTime::now(),
            queues: std::collections::HashMap::new(),
            link: std::collections::HashMap::new(),
        });
        Self { inner: Arc::new(ArcSwap::from(init)) }
    }

    // Publisher: never blocks readers
    pub fn publish(&self, snap: MACQueuesSnapshot) {
        self.inner.store(Arc::new(snap));
    }

    // Reader: lock-free, cheap clone of Arc
    pub fn latest(&self) -> Arc<MACQueuesSnapshot> {
        self.inner.load_full()
    }
}


pub fn mon_mac_thread(
    mut mac_mon: MACQueueMonitor, bus: LatestBus
) -> thread::JoinHandle<()> {
    let mon_thread = thread::spawn(move || {
        let spin_sleeper = spin_sleep::SpinSleeper::new(100_000)
            .with_spin_strategy(spin_sleep::SpinStrategy::YieldThread);
        loop {
            let deadline = SystemTime::now() + Duration::from_nanos(300_000_000); // 300ms    
            let mut all: HashMap<String, MACQueueInfo> = HashMap::new();
            let mut link: HashMap<String, LinkInfo> = HashMap::new();

            mac_mon.query.iter_mut().for_each(|(ip, q)| {
                q.update_queue_info();
                q.update_link_info();
                all.insert(ip.clone(), q.queue_info.clone());
                link.insert(ip.clone(), q.link_info.clone());
            });


            bus.publish(MACQueuesSnapshot {
                taken_at: SystemTime::now(),
                queues: all,
                link,
            });

            if let Ok(remaining_time) = deadline.duration_since(SystemTime::now()) {
                spin_sleeper.sleep(remaining_time);
            }
        }
    });
    mon_thread
}