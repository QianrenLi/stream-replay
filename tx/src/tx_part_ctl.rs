use core::packet::{PacketType};
use crate::statistic::mac_queue::{GuardedMACMonitor, MACQueueInfo};
use std::{collections::HashMap, fs::File};

use serde::{Deserialize, Serialize};

type ChannelValue = [usize; 2];
type TxRecord = HashMap<usize, ChannelValue>;


#[derive(Serialize, Deserialize, Debug,Clone, Default, Copy)]
pub enum Policy {
    Proposed,
    ConditionalRR,
    #[default]
    HardThreshold,
}

pub struct SchedulingParameters {
    pub seq: usize,
    pub offset: usize,
    pub num: usize,
    pub arrival_time: f64,
    pub current_time: f64,
}


#[derive(Debug)]
pub struct TxPartCtler {
    pub tx_part: f64,
    pub hyper_parameters: Vec<f64>,
    pub policy: Policy,
    pub blocked_signals: Vec<bool>,
    pub log_str: String,
    mac_monitor: GuardedMACMonitor,
}

impl TxPartCtler {
    pub fn new(tx_part: f64, policy: Policy, mac_monitor: GuardedMACMonitor) -> Self {
        TxPartCtler {
            tx_part,
            policy,
            hyper_parameters: vec![0.8, 0.1],
            blocked_signals: vec![false; 2],
            mac_monitor,
            log_str: String::new(),
        }
    }

    pub fn set_tx_part(&mut self, tx_part: f64) {
        self.tx_part = tx_part;
    }

    //  ---------> | Channel 0  
    //  Channel 1  | --------------------->
    // 0, 1, ..., 12, 13, 14, 15,..., 49, 50
    //             ^           
    //             |           
    //     tx_part * num
    pub fn get_packet_state(&mut self, params: SchedulingParameters) -> Option<PacketType> {
        match self.policy {
            Policy::Proposed => {
                let is_last = params.offset == params.num - 1;
                let ac1_info = self.mac_monitor.lock().unwrap().get_ac_queue(1);
                let link1_frac = (60 - ac1_info[0]) as f64 * self.hyper_parameters[0];
                let link2_frac = (60 - ac1_info[1]) as f64 * self.hyper_parameters[1];
                self.log_str.push_str(&format!("{} {} {} {} {} {:?} ", params.seq, params.offset, params.num, link1_frac, link2_frac, ac1_info));
                // if params.num < 65 || (params.offset as f64 / params.num as f64) < link1_frac / (link1_frac + link2_frac) { 
                if params.num < 50 || (params.offset as f64 / params.num as f64) < 0.95 { 
                    self.log_str.push_str("1\n");
                    if is_last { return Some(PacketType::LastPacketInFirstLink) } else { return Some(PacketType::FirstLink) }
                } else {
                    self.log_str.push_str("2\n");
                    if is_last { return Some(PacketType::LastPacketInSecondLink) } else { return Some(PacketType::SecondLink) }
                }
            }
            Policy::ConditionalRR => {
                let is_last = params.offset == params.num - 1;
                if !self.blocked_signals[0] {
                    if is_last { return Some(PacketType::LastPacketInFirstLink) } else { return Some(PacketType::FirstLink) }
                }
                if self.blocked_signals[0] && !self.blocked_signals[1] {
                    if is_last { return Some(PacketType::LastPacketInSecondLink) } else { return Some(PacketType::SecondLink) }
                }
                self.blocked_signals[0] = false;
                self.blocked_signals[1] = false;
                None
            },
            Policy::HardThreshold => {
                let is_last = params.offset == params.num - 1;
                if params.offset as f64 >= self.tx_part * params.num as f64 {
                    if is_last { Some(PacketType::LastPacketInSecondLink) } else { Some(PacketType::SecondLink) }
                } else {
                    if is_last { Some(PacketType::LastPacketInFirstLink) } else { Some(PacketType::FirstLink) }
                }
            },
            _ => {
                panic!("Unsupported policy for TxPartCtler");
            }
        }

    }

}