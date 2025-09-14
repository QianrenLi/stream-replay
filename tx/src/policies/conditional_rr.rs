use crate::policies::{SchedulingMessage, PolicyParameter};
use core::packet::PacketType;


pub fn get_packet_state(params: SchedulingMessage, _policy_parameters: &PolicyParameter) -> Option<PacketType> {
    let is_last = params.offset == params.num - 1;
    if !params.blocked_signals[0] {
        if is_last { return Some(PacketType::LastPacketInFirstLink) }
        else { return Some(PacketType::FirstLink) }
    }
    if params.blocked_signals[0] && !params.blocked_signals[1] {
        if is_last { return Some(PacketType::LastPacketInSecondLink) }
        else { return Some(PacketType::SecondLink) }
    }
    None
}