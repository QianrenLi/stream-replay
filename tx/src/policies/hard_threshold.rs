use crate::policies::{SchedulingMessage, PolicyParameter};
use core::packet::PacketType;

pub fn get_packet_state(params: SchedulingMessage, policy_parameters: &PolicyParameter) -> Option<PacketType> {
    let is_last = params.offset == params.num - 1;
    if params.offset as f64 >= policy_parameters.theta_1 * params.num as f64 {
        if is_last { Some(PacketType::LastPacketInSecondLink) }
        else { Some(PacketType::SecondLink) }
    } else {
        if is_last { Some(PacketType::LastPacketInFirstLink) }
        else { Some(PacketType::FirstLink) }
    }
}
