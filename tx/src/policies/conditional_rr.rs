use crate::policies::{SchedulingMessage, PolicyParameter};
use core::packet::PacketType;


pub fn get_packet_state(params: SchedulingMessage, _policy_parameters: &PolicyParameter) -> PacketType {
    let is_last = params.offset == params.num - 1;
    if params.blocked_signals[0] && !params.blocked_signals[1] {
        if is_last { return PacketType::LastPacketInSecondLink }
        else { return PacketType::SecondLink }
    }
    if is_last { return PacketType::LastPacketInFirstLink }
    else { return PacketType::FirstLink }
}