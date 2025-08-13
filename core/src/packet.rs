#![allow(dead_code)]
use std::ops::{Deref, DerefMut};

const IP_HEADER_LENGTH:usize = 20;
const UDP_HEADER_LENGTH:usize = 8;
pub const APP_HEADER_LENGTH:usize = 9;
pub const UDP_MAX_LENGTH:usize = 1500 - IP_HEADER_LENGTH - UDP_HEADER_LENGTH;
pub const MAX_PAYLOAD_LEN:usize = UDP_MAX_LENGTH - APP_HEADER_LENGTH;

pub type PacketSender   = flume::Sender<PacketStruct>;
pub type PacketReceiver = flume::Receiver<PacketStruct>;

pub type BufferSender = flume::Sender<Vec<u8>>;
pub type BufferReceiver = flume::Receiver<Vec<u8>>;

unsafe fn any_as_u8_slice<T: Sized>(p: &T) -> &[u8] {
    ::std::slice::from_raw_parts(
        (p as *const T) as *const u8,
        ::std::mem::size_of::<T>(),
    )
}

#[derive(Clone)]
pub enum PacketType {
    FirstLink,
    SecondLink,
    LastPacketInFirstLink,
    LastPacketInSecondLink,
}

#[repr(C,packed)]
#[derive(Copy, Clone, Debug)]
pub struct PacketStruct {
    pub seq: u32,       //4 Bytes
    pub offset: u16,    //2 Bytes, how much left to send
    pub length: u16,    //2 Bytes
    pub indicators: u8, //1 Byte, 0 - 1 represents the interface id, 10~19 represents the last packet of interface id 
    pub payload: [u8; MAX_PAYLOAD_LEN]
}

#[derive(Copy, Clone, Debug)]
pub struct PacketWithMeta {
    pub packet: PacketStruct,
    pub port: u16,      
    pub num: usize,       // number of packets in the original datagram
    pub arrival_time: f64,
}

impl Deref for PacketWithMeta {
    type Target = PacketStruct;
    fn deref(&self) -> &Self::Target {
        &self.packet
    }
}
impl DerefMut for PacketWithMeta {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.packet
    }
}

impl PacketWithMeta {
    pub fn new( port: u16 ) -> Self {
        PacketWithMeta { 
            packet: PacketStruct::new(), 
            port,
            arrival_time: 0.0,
            num: 0
        }
    }

    pub fn to_u8_slice(&self) -> &[u8] {
        unsafe { any_as_u8_slice(&self.packet) }
    }

    pub fn next_seq(&mut self, num: usize) {
        self.num = num;
        self.packet.next_seq(num);
    }
}

impl PacketStruct {
    pub fn new() -> Self {
        // dummy payload content from 0..MAX_PAYLOAD_LEN
        let mut payload = [0u8; MAX_PAYLOAD_LEN];
        (0..MAX_PAYLOAD_LEN).for_each(|i| payload[i] = i as u8);
        PacketStruct { seq: 0, offset: 0, length: 0, indicators:0 , payload }
    }
    pub fn set_length(&mut self, length: u16) {
        self.length = length;
    }
    pub fn next_seq(&mut self, num: usize) {
        self.seq += 1;
        self.offset = num as u16;
    }
    pub fn set_offset(&mut self, offset: u16) {
        self.offset = offset;
    }

    pub fn set_indicator(&mut self, packet_type: PacketType){
        self.indicators = to_indicator(packet_type);
    }

    pub fn set_payload(&mut self, payload: &[u8]) {
        self.payload[..payload.len()].copy_from_slice(payload);
    }
}

pub fn channel_info(indicator: u8) -> u8{
    indicator & 0b00000001 // Get the last bit of the indicator
}

pub fn from_buffer(buffer: &[u8]) -> PacketStruct {
    // usafe to cast buffer to PacketStruct
    unsafe {
        let mut packet: PacketStruct = std::mem::zeroed();
        let packet_ptr = &mut packet as *mut PacketStruct as *mut u8;
        std::ptr::copy_nonoverlapping(buffer.as_ptr(), packet_ptr, std::mem::size_of::<PacketStruct>());
        packet
    }
}

pub fn to_indicator(packet_type: PacketType) -> u8 {
    match packet_type {
        PacketType::FirstLink      =>  0b00000000,
        PacketType::SecondLink      =>  0b00000001,
        PacketType::LastPacketInFirstLink  =>  0b00000010,
        PacketType::LastPacketInSecondLink  =>  0b00000011,
    }
}
pub fn get_packet_type(indicators: u8) -> PacketType {
    match indicators {
        0b00000000 => PacketType::FirstLink,      // FL
        0b00000001 => PacketType::SecondLink,      // SL
        0b00000010 => PacketType::LastPacketInFirstLink,  // LPinFL
        0b00000011 => PacketType::LastPacketInSecondLink,  // LPinSL
        _ => panic!("Invalid packet type")
    }
}

//Reference: https://wireless.wiki.kernel.org/en/developers/documentation/mac80211/queues
pub fn tos2ac(tos: u8) -> usize {
    let ac_bits = (tos & 0xE0) >> 5;
    match ac_bits {
        0b001 | 0b010 => 3, // AC_BK (AC3)
        0b000 | 0b011 => 2, // AC_BE (AC2)
        0b100 | 0b101 => 1, // AC_VI (AC1)
        0b110 | 0b111 => 0, // AC_VO (AC0)
        _ => { panic!("Impossible ToS value.") }
    }
}
