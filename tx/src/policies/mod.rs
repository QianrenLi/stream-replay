use std::fmt::Debug;
use serde::{Deserialize, Serialize};
use core::packet::PacketType;

mod queue_aware;
mod conditional_rr;
mod hard_threshold;

// Define the `Policy` enum
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub enum Policy {
    QueueAware,
    ConditionalRR,
    #[default]
    HardThreshold,
}

impl Policy {
    // This function matches the policy and calls the corresponding function for each policy
    pub fn get_packet_state(
        &self,
        params: SchedulingMessage,
        policy_parameters: &PolicyParameter,
    ) -> Option<PacketType> {
        match self {
            Policy::QueueAware => queue_aware::get_packet_state(params, policy_parameters),
            Policy::ConditionalRR => conditional_rr::get_packet_state(params, policy_parameters),
            Policy::HardThreshold => hard_threshold::get_packet_state(params, policy_parameters),
        }
    }
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize)]
pub struct PolicyParameter {
    pub theta_1: f64,
    pub theta_2: f64,
    pub theta_3: f64,
    pub theta_4: f64,
}

pub struct SchedulingMessage {
    pub seq: usize,
    pub offset: usize,
    pub num: usize,
    pub arrival_time: f64,
    pub current_time: f64,
    pub blocked_signals: Vec<bool>,   // Now included in SchedulingMessage
    pub ac1_info: Vec<usize>,         // Store ac1_info directly in SchedulingMessage
    pub mcs_values: Option<Vec<f32>>, // MCS values for different access categories
}

impl SchedulingMessage {
    pub fn new(packet: core::packet::PacketWithMeta, current_time: f64, blocked_signals: Vec<bool>, ac1_info: Vec<usize>, mcs_values: Option<Vec<f32>>) -> Self {
        SchedulingMessage {
            seq: packet.seq as usize,
            arrival_time: packet.arrival_time,
            current_time: current_time,
            offset: packet.offset as usize,
            num: packet.num as usize,
            blocked_signals,
            ac1_info,
            mcs_values,
        }
    }
}

