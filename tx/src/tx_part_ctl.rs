use core::packet::{PacketType};

use serde::{Deserialize, Serialize};

#[derive(Debug)]
pub struct TxPartCtler {
    pub tx_part: f64,
    pub policy: Policy,
    pub blocked_signals: Vec<bool>,
}

#[derive(Serialize, Deserialize, Debug,Clone, Default, Copy)]
pub enum Policy {
    Proposed,
    ConditionalRR,
    #[default]
    HardThreshold,
}

pub struct SchedulingParameters {
    pub arrival_time: f64,
    pub current_time: f64,
    pub offset: usize,
    pub num: usize,
}


pub trait Schedulable {
    fn schedule(&self, params: &SchedulingParameters) -> PacketType;
}


impl TxPartCtler {
    pub fn new(tx_part: f64, policy: Policy) -> Self {
        TxPartCtler {
            tx_part,
            policy,
            blocked_signals: vec![false; 2],
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