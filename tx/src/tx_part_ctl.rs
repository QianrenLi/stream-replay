use core::packet::{PacketType};
use crate::statistic::mac_queue::{GuardedMACMonitor};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Debug,Clone, Default, Copy)]
pub enum Policy {
    Proposed,
    ConditionalRR,
    #[default]
    HardThreshold,
}

pub struct SchedulingMessage {
    pub seq: usize,
    pub offset: usize,
    pub num: usize,
    pub arrival_time: f64,
    pub current_time: f64,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct PolicyParameter{
    pub theta_1: f64,
    pub theta_2: f64,
    pub theta_3: f64,
    pub theta_4: f64,
}

#[derive(Debug)]
pub struct TxPartCtler {
    pub policy: Policy,
    pub blocked_signals: Vec<bool>,
    pub log_str: String,
    pub policy_parameters: PolicyParameter,
    mac_monitor: GuardedMACMonitor,
}

impl TxPartCtler {
    pub fn new(policy: Policy, policy_parameters:PolicyParameter ,mac_monitor: GuardedMACMonitor) -> Self {
        TxPartCtler {
            policy,
            blocked_signals: vec![false; 2],
            mac_monitor,
            policy_parameters,
            log_str: String::new(),
        }
    }

    fn parameterized_function(&mut self, left_pkts: usize, queue1: usize, queue2: usize) -> bool {
        let val1 = ( (left_pkts + queue1) as f64 ).powf(self.policy_parameters.theta_1) / (self.policy_parameters.theta_3);
        let val2 = ( (left_pkts + queue2) as f64 ).powf(self.policy_parameters.theta_2) / (self.policy_parameters.theta_4);
        return  val1 < val2;
    }

    //  ---------> | Channel 0  
    //  Channel 1  | --------------------->
    // 0, 1, ..., 12, 13, 14, 15,..., 49, 50
    //             ^           
    //             |           
    //     tx_part * num
    pub fn get_packet_state(&mut self, params: SchedulingMessage) -> Option<PacketType> {
        match self.policy {
            Policy::Proposed => {
                let is_last = params.offset == params.num - 1;
                let ac1_info = self.mac_monitor.lock().unwrap().get_ac_queue(1);
                self.log_str.push_str(&format!("{} {} {} {:?} ", params.seq, params.offset, params.num, ac1_info));
                println!("{:?} ", self.policy_parameters);
                if self.parameterized_function(params.num - params.offset, ac1_info[0], ac1_info[1]) { 
                // if (params.num - params.offset) + ac1_info[0] < 50 || (params.offset as f64 / params.num as f64) < 0.95 { 
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
                if params.offset as f64 >= self.policy_parameters.theta_1 * params.num as f64 {
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