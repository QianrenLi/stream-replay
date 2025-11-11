use crate::policies::{SchedulingMessage, PolicyParameter};
use core::packet::PacketType;


pub fn get_packet_state(params: SchedulingMessage, policy_parameters: &PolicyParameter) -> PacketType {
    let ac1_info = params.ac1_info;
    let mcs_values= params.mcs_values.unwrap();
    let is_last = params.offset == params.num - 1;

    if parameterized_function(ac1_info, policy_parameters, mcs_values) {
        if is_last { return PacketType::LastPacketInFirstLink }
        else { return PacketType::FirstLink }
    } else {
        if is_last { return PacketType::LastPacketInSecondLink }
        else { return PacketType::SecondLink }
    }
}

fn parameterized_function(ac1_info:Vec<usize>, policy_parameters: &PolicyParameter, mcs_values: Vec<f32>) -> bool {
    let val1 = ((1 + ac1_info[0]) as f32)  / (policy_parameters.theta_3 as f32 * mcs_values[0]  + 0.01f32);
    let val2 = ((1 + ac1_info[1]) as f32)  / (policy_parameters.theta_4 as f32 * mcs_values[1]  + 0.01f32);
    return val1 < val2;
}
