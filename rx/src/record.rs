use std::collections::HashMap;

use std::sync::mpsc::Sender;
use core::packet::{self, PacketStruct, PacketType};

use crate::statistic::stuttering::Stutter;
#[derive(Default)]
struct RecvOffsets {
    first_link_rx_time: Option<f64>,
    second_link_rx_time: Option<f64>,
}

type IsACK = (bool, bool);

pub struct RecvData{
    pub recv_records: HashMap<u32, RecvRecord>,
    pub last_seq: u32,
    pub recevied: u32,
    pub data_len: u32,
    pub rx_start_time: f64,
    pub stutter: Stutter,
    pub tx: Option<Sender<Vec<u8>>>
}

impl RecvData{
    pub fn new() -> Self{
        Self{
            recv_records: HashMap::new(),
            last_seq: 0,
            recevied: 0,
            data_len: 0,
            rx_start_time: 0.0,
            stutter: Stutter::new(),
            tx: None,
        }
    }
}


pub struct RecvRecord {
    pub packets: HashMap<u16, PacketStruct>, // Use a HashMap to store packets by their offset
    pub is_ack: IsACK,
    offsets: RecvOffsets,
    last_packet_id: Option<u16>,
    pub is_complete: bool,
}

impl RecvRecord {
    pub fn new() -> Self{
        Self{
            packets: HashMap::<u16, PacketStruct>::new(),
            is_ack : (false, false),
            offsets: RecvOffsets::default(),
            last_packet_id: None,
            is_complete: false,
        }
    }
    pub fn record(&mut self, data: &[u8]) {
        let packet = packet::from_buffer(data);
        let offset = Some(packet.offset);
        let rx_time = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs_f64();

        match packet::get_packet_type(packet.indicators) {
            PacketType::FirstLink => self.offsets.first_link_rx_time = Some(rx_time),
            PacketType::SecondLink => self.offsets.second_link_rx_time = Some(rx_time),
            PacketType::LastPacketInFirstLink => {
                self.last_packet_id = offset;
                self.offsets.first_link_rx_time = Some(rx_time);
            },
            PacketType::LastPacketInSecondLink => {
                self.last_packet_id = offset;
                self.offsets.second_link_rx_time = Some(rx_time);
            }
        }

        self.packets.insert(packet.offset as u16, packet);
        self.is_complete = self.determine_complete();
    }

    pub fn delta(&self) -> f64 {
        let first_link_time = self.offsets.first_link_rx_time.unwrap_or(0.0);
        let second_link_time = self.offsets.second_link_rx_time.unwrap_or(0.0);
        second_link_time - first_link_time
    }

    fn determine_complete(&self) -> bool {
        fn is_range_complete(packets: &HashMap<u16, PacketStruct>, mut range: std::ops::RangeInclusive<u16>) -> bool {
            range.all(|i| packets.contains_key(&i))
        }
    
        if let Some(last_id) = self.last_packet_id {
            is_range_complete(&self.packets, 0..=last_id)
        }
        else {
            false
        }
    }
    #[allow(dead_code)]
    pub fn gather(&self) -> Vec<u8>{
        let mut data = Vec::new();
        let num_packets = self.packets.len();
        for i in 0..num_packets{
            let packet = self.packets.get(&(i as u16)).unwrap();
            data.extend_from_slice(&packet.payload[ ..packet.length as usize]);
        }
        return data;
    }
}