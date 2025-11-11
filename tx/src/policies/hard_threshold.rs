use crate::policies::{SchedulingMessage, PolicyParameter};
use core::packet::PacketType;

pub fn get_packet_state(params: SchedulingMessage, policy_parameters: &PolicyParameter) -> PacketType {
    let is_last = params.offset == params.num - 1;
    if params.offset as f32 >= policy_parameters.theta_1 * params.num as f32 {
        if is_last { PacketType::LastPacketInSecondLink }
        else { PacketType::SecondLink }
    } else {
        if is_last { PacketType::LastPacketInFirstLink }
        else { PacketType::FirstLink }
    }
}
