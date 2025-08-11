use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};
use ndarray::prelude::*;
use ndarray_npy::read_npy;
use log::trace;

use rand::seq::SliceRandom;
use rand::thread_rng;

use core::packet::*;
use crate::conf::{StreamParam, ConnParams};
use crate::dispatcher::dispatch;
use crate::throttle::RateThrottler;
use crate::rtt::{RttRecorder,RttSender};
use crate::ipc::Statistics;
use crate::tx_part_ctl::TxPartCtler;
use crate::utils::trace_reader::read_packets;

type GuardedThrottler = Arc<Mutex<RateThrottler>>;
type GuardedTxPartCtler = Arc<Mutex<TxPartCtler>>;
type SokcetInfo = HashMap<String, flume::Sender<PacketStruct>>;

pub const STREAM_PROTO: &str = "stream://";

fn generate_packets(
    num: usize, 
    _remains: usize, 
    template: &mut PacketStruct, 
    tx_part_ctler: &GuardedTxPartCtler,
    buffer: Option<&Vec<u8>>,
) -> Vec<PacketStruct> {
    let mut packets = Vec::new();
    let mut packet_states = tx_part_ctler.lock().unwrap().get_packet_states(num);
    let mut rng = thread_rng();
    packet_states.shuffle(&mut rng);

    for packet_state in packet_states {
        for (offset, packet_type) in packet_state {
            let length = if offset == (num - 1) as u16 {
                _remains as u16
            } else {
                MAX_PAYLOAD_LEN as u16
            };

            template.set_length(length);
            template.set_offset(offset);
            template.set_indicator(packet_type);
            if let Some(buf) = buffer {
                template.set_payload(&buf[(offset as usize * MAX_PAYLOAD_LEN)..(offset as usize * MAX_PAYLOAD_LEN) + length as usize]);
            };
            packets.push(template.clone());
        }
    }
    packets
}

fn process_queue(
    throttler: &GuardedThrottler, 
    tx_part_ctler: &GuardedTxPartCtler, 
    socket_infos: &SokcetInfo, 
    stop_time: &SystemTime
) {
    while SystemTime::now() < *stop_time {
        if let Some(_) = throttler.lock().unwrap().try_consume(|mut packet| {
            packet.timestamp = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64();
            match tx_part_ctler.lock() {
                Ok(controller) => {
                    let ip_addr = controller.packet_to_ipaddr(packet.indicators.clone());
                    if let Some(sender) = socket_infos.get(&ip_addr) {
                        let _ = sender.try_send(packet);
                    }
                }
                Err(_) => (),
            }
            true
        }) {
            continue;
        } else {
            break;
        }
    }
}

pub fn stream_thread(
    throttler: GuardedThrottler, 
    tx_part_ctler: GuardedTxPartCtler, 
    rtt_tx: Option<RttSender>, 
    params: ConnParams, 
    socket_infos: SokcetInfo, 
    dest: BufferReceiver
) {
    let mut template = PacketStruct::new(params.port);
    let stop_time = SystemTime::now().checked_add(Duration::from_secs_f64(params.duration[1])).unwrap();

    while SystemTime::now() <= stop_time {
        // Wait for the next packet
        let buffer = dest.recv().unwrap();
        let size_bytes = buffer.len();
        let (_num, _remains) = (size_bytes / MAX_PAYLOAD_LEN, size_bytes % MAX_PAYLOAD_LEN);
        let num = _num + if _remains > 0 { 1 } else { 0 };
        template.next_seq(_num, _remains);

        // Generate packets
        let packets = generate_packets( num, _remains, &mut template, &tx_part_ctler, Some(&buffer));

        // Append to application-layer queue
        throttler.lock().unwrap().prepare(packets);

        // Report RTT
        if let Some(ref r_tx) = rtt_tx {
            r_tx.send(template.seq).unwrap();
        }

        // Process queue
        process_queue(&throttler, &tx_part_ctler, &socket_infos, &stop_time);
    }

    // Reset throttler
    throttler.lock().unwrap().reset();
}

pub fn video_thread(
    throttler: GuardedThrottler, 
    tx_part_ctler: GuardedTxPartCtler, 
    rtt_tx: Option<RttSender>, 
    params: ConnParams, 
    socket_infos: SokcetInfo
) {
    let trace: Vec<(u64, Vec<u8>)> = read_packets(&params.npy_file).expect("loading failed.");
    let (start_offset, duration) = (params.start_offset, params.duration);
    let mut template = PacketStruct::new(params.port);
    let spin_sleeper = spin_sleep::SpinSleeper::new(100_000).with_spin_strategy(spin_sleep::SpinStrategy::YieldThread);

    let mut loops = 0;
    let mut idx = start_offset;
    let stop_time = SystemTime::now().checked_add(Duration::from_secs_f64(duration[1])).unwrap();

    spin_sleeper.sleep(Duration::from_secs_f64(duration[0]));
    while SystemTime::now() <= stop_time {
        loops += 1;

        let deadline = if loops < params.loops {
            let interval_ns = trace[idx].0;
            let size_bytes = trace[idx].1.len();
            let (_num, _remains) = (size_bytes / MAX_PAYLOAD_LEN, size_bytes % MAX_PAYLOAD_LEN);
            let num = _num + if _remains > 0 { 1 } else { 0 };
            template.next_seq(_num, _remains);

            // Generate packets
            let packets = generate_packets(num, _remains, &mut template, &tx_part_ctler, Some(&trace[idx].1));

            // Append to application-layer queue
            throttler.lock().unwrap().prepare(packets);

            // Report RTT
            if let Some(ref r_tx) = rtt_tx {
                r_tx.send(template.seq).unwrap();
            }

            // Next iteration
            idx = (idx + 1) % trace.len();
            SystemTime::now() + Duration::from_nanos(interval_ns)
        } else {
            stop_time
        };

        // Process queue
        process_queue(&throttler, &tx_part_ctler, &socket_infos, &deadline);

        // Sleep until next arrival
        if let Ok(remaining_time) = deadline.duration_since(SystemTime::now()) {
            spin_sleeper.sleep(remaining_time);
        }
    }

    // Reset throttler
    throttler.lock().unwrap().reset();
}

pub fn source_thread(
    throttler: GuardedThrottler, 
    tx_part_ctler: GuardedTxPartCtler, 
    rtt_tx: Option<RttSender>, 
    params: ConnParams, 
    socket_infos: SokcetInfo
) {
    let trace: Array2<u64> = read_npy(&params.npy_file).expect("loading failed.");
    let (start_offset, duration) = (params.start_offset, params.duration);
    let mut template = PacketStruct::new(params.port);
    let spin_sleeper = spin_sleep::SpinSleeper::new(100_000).with_spin_strategy(spin_sleep::SpinStrategy::YieldThread);

    let mut loops = 0;
    let mut idx = start_offset;
    let stop_time = SystemTime::now().checked_add(Duration::from_secs_f64(duration[1])).unwrap();

    spin_sleeper.sleep(Duration::from_secs_f64(duration[0]));
    while SystemTime::now() <= stop_time {
        loops += 1;

        let deadline = if loops < params.loops {
            idx = (idx + 1) % trace.shape()[0];
            let size_bytes = trace[[idx, 1]] as usize;
            let interval_ns = trace[[idx, 0]];

            // Generate packets
            let (_num, _remains) = (size_bytes / MAX_PAYLOAD_LEN, size_bytes % MAX_PAYLOAD_LEN);
            let num = _num + if _remains > 0 { 1 } else { 0 };
            template.next_seq(_num, _remains);

            // Generate packets
            let packets = generate_packets(num, _remains, &mut template, &tx_part_ctler, None);

            // Append to application-layer queue
            throttler.lock().unwrap().prepare(packets);

            // Report RTT
            if let Some(ref r_tx) = rtt_tx {
                r_tx.send(template.seq).unwrap();
            }

            // Next iteration
            SystemTime::now() + Duration::from_nanos(interval_ns)
        } else {
            stop_time
        };

        // Process queue
        process_queue(&throttler, &tx_part_ctler, &socket_infos, &deadline);

        // Sleep until next arrival
        if let Ok(remaining_time) = deadline.duration_since(SystemTime::now()) {
            spin_sleeper.sleep(remaining_time);
        }
    }

    // Reset throttler
    throttler.lock().unwrap().reset();
}

pub struct SourceManager{
    pub name: String,
    stream: StreamParam,
    #[allow(dead_code)]
    pub source: Vec<BufferSender>,
    dest: Vec<BufferReceiver>,
    //
    start_timestamp: SystemTime,
    stop_timestamp: SystemTime,
    //
    throttler: GuardedThrottler,
    rtt: Option<RttRecorder>,
    tx_part_ctler: Arc<Mutex<TxPartCtler>>,
    //
    socket_infos: Vec<SokcetInfo>,
}

impl SourceManager {
    pub fn new(stream: StreamParam, window_size:usize) -> Self {
        let (StreamParam::UDP(ref params) | StreamParam::TCP(ref params)) = stream;
        let mut name = stream.name();

        let socket_infos = vec![dispatch(params.links.clone(), params.tos)].into();
        let link_num = params.links.len();
        let target_rtt = params.target_rtt;

        let throttler = Arc::new(Mutex::new(
            RateThrottler::new(name.clone(), params.throttle, window_size, params.no_logging, false)
        ));
        let tx_part_ctler = Arc::new(Mutex::new(
            TxPartCtler::new(params.tx_parts.clone(), params.links.clone())
        ));

        let rtt =  match params.calc_rtt {
            false => None,
            true => Some( RttRecorder::new( &name, params.port, link_num, target_rtt) )
        };

        let start_timestamp = SystemTime::now();
        let stop_timestamp = SystemTime::now();

        let (source, dest) = if params.npy_file.starts_with(STREAM_PROTO) {
            name = params.npy_file.clone();
            let (tx, rx) = flume::unbounded();
            (vec![tx], vec![rx])
        } else {
            (vec![], vec![])
        };

        Self{ name, stream, throttler, rtt, tx_part_ctler, socket_infos, start_timestamp, stop_timestamp, source, dest }
    }

    pub fn throttle(&self, throttle:f64) {
        if let Ok(ref mut throttler) = self.throttler.lock() {
            throttler.throttle = throttle;
        }
    }

    pub fn set_tx_parts(&self, tx_parts:Vec<f64>) {
        if let Ok(ref mut tx_part_ctler) = self.tx_part_ctler.lock() {
            if tx_parts.len() != tx_part_ctler.tx_parts.len() {
                return;
            }
            tx_part_ctler.set_tx_parts(tx_parts);
        }
    }

    pub fn statistics(&self) -> Option<Statistics> {
        let now = SystemTime::now();
        if now < self.start_timestamp || now > self.stop_timestamp {
            return None;
        }
    
        let throughput = self.throttler.lock().ok()?.last_rate;
        let throttle = self.throttler.lock().ok()?.throttle;
    
        let (rtt, channel_rtts, outage_rate, ch_outage_rates) = if let Some(ref rtt) = self.rtt {
            let stats = rtt.rtt_records.lock().unwrap().statistic();
            (Some(stats.0), Some(stats.1), Some(stats.2), Some(stats.3))
        } else {
            (None, None, None, None)
        };
    
        let tx_parts = self.tx_part_ctler.lock().ok()?.tx_parts.clone();

        
    
        Some(Statistics { rtt, channel_rtts, outage_rate, ch_outage_rates, throughput, tx_parts, throttle })
    }

    pub fn start(&mut self, index:usize, tx_ipaddr:String) -> JoinHandle<()> {
        let throttler = Arc::clone(&self.throttler);
        let tx_part_ctler = Arc::clone(&self.tx_part_ctler);
        let rtt_tx = match self.rtt {
            Some(ref mut rtt) => Some( rtt.start(tx_ipaddr) ),
            None => None
        };
        let (StreamParam::UDP(ref params) | StreamParam::TCP(ref params)) = self.stream;
        let params = params.clone();

        let _now = SystemTime::now();
        self.start_timestamp = _now + Duration::from_secs_f64( params.duration[0] );
        self.stop_timestamp = _now + Duration::from_secs_f64( params.duration[1] );

        let dest = self.dest.pop();
        let socket_infos = self.socket_infos.pop().unwrap();
        let source = thread::spawn(move || {
            if params.npy_file.starts_with(STREAM_PROTO) {
                let dest = dest.unwrap();
                stream_thread(throttler, tx_part_ctler, rtt_tx, params, socket_infos, dest)
            }
            else if params.npy_file.ends_with(".bin") {
                video_thread(throttler, tx_part_ctler, rtt_tx, params, socket_infos);
            }
            else {
                source_thread(throttler, tx_part_ctler, rtt_tx, params, socket_infos);
            }
        });

        println!("{}. {} on ...", index, self.stream);
        source
    }
}
