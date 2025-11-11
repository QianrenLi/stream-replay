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
    pub schedule_message: Option<SchedulingMessage>,
}

impl TxPartCtler {
    pub fn new(policy: Policy, policy_parameters: PolicyParameter, mac_info_bus: LatestBus) -> Self {
        TxPartCtler {
            policy,
            blocked_signals: vec![false; 2],
            mac_info_bus,
            policy_parameters,
            log_str: String::new(),
            schedule_message: None,
        }
    }

    pub fn get_packet_state(&mut self, params: SchedulingMessage) -> PacketType {
        let packet_type = self.policy.get_packet_state(params, &self.policy_parameters);
        if let Some(ref mut sm) = self.schedule_message {
            sm.update_sended_counter(&packet_type);
        }
        return packet_type;
    }

    pub fn determine_schedule_info(&mut self, packet: core::packet::PacketWithMeta) -> Option<SchedulingMessage>{
        let mac_info = self.mac_info_bus.latest().as_ref().to_owned();
        if self.mac_info_bus.is_mon && ( mac_info.queues.is_empty() || mac_info.link.is_empty() ) {
            return None;
        }
        match self.schedule_message {
            // Borrow the existing message and update it in place if timestamp matches
            Some(ref mut sm) if sm.current_time == mac_info.taken_at => {
                sm.update(packet, self.blocked_signals.clone());
            }
            _ => {
                self.schedule_message = Some(SchedulingMessage::new(
                    packet, 
                    mac_info.taken_at,
                    self.blocked_signals.clone(),
                    mac_info.queues.values()
                    .map(|qinfo| qinfo.get(&1).cloned().unwrap_or(0))
                    .collect(),
                    mac_info.link.values().map(|link| link.tx_mbit_s).collect(),
                ));
            }
        };
        self.schedule_message.clone()
    }
}
