use core::packet::{self, PacketType};
use crate::link::Link;

#[derive(Debug)]
pub struct TxPartCtler {
    pub tx_part: f64,
    tx_ipaddrs: Vec<String>,
}

impl TxPartCtler {
    pub fn new(tx_part: f64, links: Vec<Link>) -> Self {
        let mut tx_ipaddrs = Vec::new();
        for link in links.iter() {
            tx_ipaddrs.push(link.tx_ipaddr.clone());
        }
        TxPartCtler {
            tx_part,
            tx_ipaddrs,
        }
    }

    pub fn set_tx_part(&mut self, tx_part: f64) {
        self.tx_part = tx_part;
    }

    //  ---------> | Channel 0  
    //  Channel 1  | --------------------->
    // 0, 1, ..., 12, 13, 14, 15,..., 49, 50
    //             ^           
    //             |           
    //     tx_part * num
    pub fn get_packet_state(&self, offset: usize, num: usize) -> PacketType {
        let is_last = offset == num - 1;
        if offset as f64 >= self.tx_part * num as f64 {
            if is_last { PacketType::LastPacketInSecondLink } else { PacketType::SecondLink }
        } else {
            if is_last { PacketType::LastPacketInFirstLink } else { PacketType::FirstLink }
        }
    }

    pub fn packet_to_ipaddr(&self, indicator: u8) -> String {
        self.tx_ipaddrs[packet::channel_info(indicator) as usize].clone()
    }
}