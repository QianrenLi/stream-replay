use core::packet::{PacketType};
use crate::statistic::mac_queue::{LatestBus};
use crate::policies::{Policy, PolicyParameter, SchedulingMessage};


#[derive(Debug)]
pub struct TxPartCtler {
    pub policy: Policy,
    pub blocked_signals: Vec<bool>,
    pub log_str: String,
    pub policy_parameters: PolicyParameter,
    pub mac_info_bus: LatestBus,
}

impl TxPartCtler {
    pub fn new(policy: Policy, policy_parameters: PolicyParameter, mac_info_bus: LatestBus) -> Self {
        TxPartCtler {
            policy,
            blocked_signals: vec![false; 2],
            mac_info_bus,
            policy_parameters,
            log_str: String::new(),
        }
    }

    pub fn get_packet_state(&mut self, params: SchedulingMessage) -> Option<PacketType> {
        self.policy.get_packet_state(params, &self.policy_parameters)
    }
}
