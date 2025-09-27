use crate::policies::{SchedulingMessage, PolicyParameter};
use core::packet::PacketType;


pub fn get_packet_state(params: SchedulingMessage, policy_parameters: &PolicyParameter) -> PacketType {
    let ac1_info = params.ac1_info;
    let mcs_values= params.mcs_values.unwrap();
    let is_last = params.offset == params.num - 1;

    if parameterized_function(params.num - params.offset, ac1_info, policy_parameters, mcs_values) {
        if is_last { return PacketType::LastPacketInFirstLink }
        else { return PacketType::FirstLink }
    } else {
        if is_last { return PacketType::LastPacketInSecondLink }
        else { return PacketType::SecondLink }
    }
}

fn parameterized_function(left_pkts: usize, ac1_info:Vec<usize>, policy_parameters: &PolicyParameter, mcs_values: Vec<f32>) -> bool {
    let val1 = ((left_pkts + ac1_info[0]) as f64) / (policy_parameters.theta_3 * mcs_values[0] as f64);
    let val2 = ((left_pkts + ac1_info[1]) as f64) / (policy_parameters.theta_4 * mcs_values[1] as f64);
    return val1 < val2;
}
