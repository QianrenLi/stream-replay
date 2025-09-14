use crate::policies::{SchedulingMessage, PolicyParameter};
use core::packet::PacketType;


pub fn get_packet_state(params: SchedulingMessage, policy_parameters: &PolicyParameter) -> Option<PacketType> {
    let ac1_info = params.ac1_info;
    let is_last = params.offset == params.num - 1;
    if parameterized_function(params.num - params.offset, ac1_info[0], ac1_info[1], policy_parameters) {
        if is_last { return Some(PacketType::LastPacketInFirstLink) }
        else { return Some(PacketType::FirstLink) }
    } else {
        if is_last { return Some(PacketType::LastPacketInSecondLink) }
        else { return Some(PacketType::SecondLink) }
    }
}

fn parameterized_function(left_pkts: usize, queue1: usize, queue2: usize, policy_parameters: &PolicyParameter) -> bool {
    let val1 = ((left_pkts + queue1) as f64).powf(policy_parameters.theta_1) / policy_parameters.theta_3;
    let val2 = ((left_pkts + queue2) as f64).powf(policy_parameters.theta_2) / policy_parameters.theta_4;
    return val1 < val2;
}
