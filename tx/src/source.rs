use std::collections::HashMap;
use std::fs::File;
use std::io::Write;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, SystemTime};
use std::net::{UdpSocket};
use std::vec;
use ndarray::prelude::*;
use ndarray_npy::read_npy;

use core::packet::*;
use crate::conf::{StreamParam, ConnParams};
use crate::dispatcher::dispatch;
use crate::statistic::mac_queue::GuardedMACMonitor;
use crate::throttle::RateThrottler;
use crate::rtt::{RttRecorder,RttSender};
use crate::ipc::Statistics;
use crate::policies::{PolicyParameter, SchedulingMessage};
use crate::tx_part_ctl::TxPartCtler;
use crate::utils::trace_reader::read_packets;
use crate::version_manager::VersionManager;

type GuardedThrottler = Arc<Mutex<RateThrottler>>;
type GuardedTxPartCtler = Arc<Mutex<TxPartCtler>>;
type GuardedVersionManager = Arc<Mutex<Option<VersionManager>>>;

pub type SocketInfo = HashMap<usize, (UdpSocket, String)>;

pub const STREAM_PROTO: &str = "stream://";

fn generate_packets(
    _remains: usize, 
    template: &mut PacketWithMeta, 
    buffer: Option<&Vec<u8>>,
) -> Vec<PacketWithMeta> {
    let mut packets = Vec::new();

    let now = SystemTime::now();
    let unix_epoch = SystemTime::UNIX_EPOCH;
    let timestamp = now.duration_since(unix_epoch).unwrap().as_secs_f64();
    template.arrival_time = timestamp;

    for offset in 0..template.num as u16 {
        let length = if offset == (template.num - 1) as u16 {
            _remains as u16
        } else {
            MAX_PAYLOAD_LEN as u16
        };
        template.set_length(length);
        template.set_offset(offset);
        if let Some(buf) = buffer {
            template.set_payload(&buf[(offset as usize * MAX_PAYLOAD_LEN)..(offset as usize * MAX_PAYLOAD_LEN) + length as usize]);
        };
        packets.push(template.clone());
    }
    packets
}

fn process_queue(
    throttler: &GuardedThrottler, 
    tx_part_ctler: &GuardedTxPartCtler, 
    socket_infos: &SocketInfo, 
    stop_time: &SystemTime,
    recorder: Option<&File>,
) { 
    // Precompute unix epoch once
    while SystemTime::now() < *stop_time {
        // Compute current time once per iteration
        if let Some(_) = throttler.lock().unwrap().try_consume(|mut packet| {            
            // Get IP address with minimal lock time
            match tx_part_ctler.lock() {
                Ok(mut controller) => {
                    let schedule_param = SchedulingMessage::new(
                        packet, 
                        SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64(),
                        controller.blocked_signals.clone(),
                        controller.mac_monitor.lock().unwrap().get_ac_queue(1),
                    );
                    if let Some(packet_type) = controller.get_packet_state(schedule_param){
                        packet.set_indicator(packet_type);
                    } else {
                        return false;
                    }
                },
                Err(_) => return false,
            };

            // Lookup socket without holding controller lock
            let sender = match socket_infos.get(&packet.channel) {
                Some(s) => s,
                None => panic!("No socket found for channel {}", packet.channel),
            };

            // Prepare send parameters outside match
            let length = APP_HEADER_LENGTH + packet.length as usize;
            let buf = packet.to_u8_slice();
            let rx_addr = format!("{}:{}", sender.1, packet.port as usize);
            
            // Attempt to send
            match sender.0.send_to(&buf[..length], &rx_addr) {
                Ok(_) => true,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                    tx_part_ctler.lock().unwrap().blocked_signals[packet.channel] = true;
                    false
                },
                Err(_) => { tx_part_ctler.lock().unwrap().blocked_signals[packet.channel] = true; 
                    false }
            }
        }) {
            // Continue processing next packet
        } else {
            break;
        }
    }
    if let Some(mut recorder) = recorder {
        recorder.write_all(tx_part_ctler.lock().unwrap().log_str.as_bytes())
            .expect("Failed to write to recorder file");
    }
    
    tx_part_ctler.lock().unwrap().log_str.clear(); // Using clear() instead of setting to an empty string
    
}

pub fn stream_thread(
    throttler: GuardedThrottler, 
    tx_part_ctler: GuardedTxPartCtler, 
    rtt_tx: Option<RttSender>, 
    params: ConnParams, 
    socket_infos: SocketInfo, 
    dest: BufferReceiver,
) {
    let mut template = PacketWithMeta::new(params.port);
    let stop_time = SystemTime::now().checked_add(Duration::from_secs_f64(params.duration[1])).unwrap();

    while SystemTime::now() <= stop_time {
        // Wait for the next packet
        let buffer = dest.recv().unwrap();
        let size_bytes = buffer.len();
        let (_num, _remains) = (size_bytes / MAX_PAYLOAD_LEN, size_bytes % MAX_PAYLOAD_LEN);
        let num = _num + if _remains > 0 { 1 } else { 0 };
        template.next_seq(num);

        // Generate packets
        let packets = generate_packets( _remains, &mut template, Some(&buffer));

        // Append to application-layer queue
        throttler.lock().unwrap().prepare(packets);

        // Report RTT
        if let Some(ref r_tx) = rtt_tx {
            r_tx.send(template.seq).unwrap();
        }

        // Process queue
        process_queue(&throttler, &tx_part_ctler, &socket_infos, &stop_time, None);
    }

    // Reset throttler
    throttler.lock().unwrap().reset();
}

pub fn video_thread(
    throttler: GuardedThrottler, 
    tx_part_ctler: GuardedTxPartCtler, 
    version_manager: GuardedVersionManager,
    rtt_tx: Option<RttSender>, 
    params: ConnParams, 
    socket_infos: SocketInfo
) {
    let mut trace = read_packets(version_manager.lock().unwrap().as_mut().unwrap().next()).expect("loading failed.");
    let mut reload = false;
    let (start_offset, duration) = (params.start_offset, params.duration);
    let spin_sleeper = spin_sleep::SpinSleeper::new(100_000).with_spin_strategy(spin_sleep::SpinStrategy::YieldThread);
    let stop_time = SystemTime::now().checked_add(Duration::from_secs_f64(duration[1])).unwrap();
    let mut template = PacketWithMeta::new(params.port);
    let recorder = File::create("logs/recorder.txt").expect("Failed to create recorder file");
    let mut idx = start_offset;
    spin_sleeper.sleep(Duration::from_secs_f64(duration[0]));
    while SystemTime::now() <= stop_time {
        if reload {
            reload = false;
            idx = start_offset;
            trace = read_packets(version_manager.lock().unwrap().as_mut().unwrap().next()).expect("loading failed.")
        };

        let deadline = {
            let interval_ns = trace[idx].0;
            let size_bytes = trace[idx].1.len();
            let buffer = &trace[idx].1;

            idx = idx + 1;
            if idx == trace.len() {
                reload = true;
            }

            let (_num, _remains) = (size_bytes / MAX_PAYLOAD_LEN, size_bytes % MAX_PAYLOAD_LEN);
            let num = _num + if _remains > 0 { 1 } else { 0 };
            template.next_seq(num);

            // Generate packets
            let packets = generate_packets(_remains, &mut template,  Some(buffer));

            // Append to application-layer queue
            throttler.lock().unwrap().prepare(packets);

            // Report RTT
            if let Some(ref r_tx) = rtt_tx {
                r_tx.send(template.seq).unwrap();
            }

            SystemTime::now() + Duration::from_nanos(interval_ns)
        };

        // Process queue
        process_queue(&throttler, &tx_part_ctler, &socket_infos, &deadline, Some(&recorder));

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
    socket_infos: SocketInfo
) {
    let trace: Array2<u64> = read_npy(&params.npy_file).expect("loading failed.");
    let (start_offset, duration) = (params.start_offset, params.duration);
    let mut template = PacketWithMeta::new(params.port);
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
            template.next_seq(num);

            // Generate packets
            let packets = generate_packets( _remains, &mut template, None);

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
        process_queue(&throttler, &tx_part_ctler, &socket_infos, &deadline, None);

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
    tx_part_ctler: GuardedTxPartCtler,
    version_manager: GuardedVersionManager,
    //
    socket_infos: Vec<SocketInfo>,
}

impl SourceManager {
    pub fn new(stream: StreamParam, window_size:usize, mac_monitor: GuardedMACMonitor) -> Self {
        let (StreamParam::UDP(ref params) | StreamParam::TCP(ref params)) = stream;
        let mut name = stream.name();

        let socket_infos = vec![dispatch(params.links.clone(), params.tos)].into();
        let link_num = params.links.len();
        let target_rtt = params.target_rtt;

        let throttler = Arc::new(Mutex::new(
            RateThrottler::new(name.clone(), params.throttle, window_size, params.no_logging, false)
        ));
        let tx_part_ctler = Arc::new(Mutex::new(
            TxPartCtler::new(params.policy, params.policy_parameters, mac_monitor)
        ));
        let version_manager = Arc::new(Mutex::new( 
            if params.npy_file.ends_with(".json") {
                Some(VersionManager::new(&params.npy_file))
            } else{
                None
            })
        );

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

        Self{ name, stream, throttler, rtt, tx_part_ctler, version_manager, socket_infos, start_timestamp, stop_timestamp, source, dest }
    }

    pub fn throttle(&self, throttle:f64) {
        if let Ok(ref mut throttler) = self.throttler.lock() {
            throttler.throttle = throttle;
        }
    }

    pub fn set_policy_parameters(&self, parameters: PolicyParameter) {
        if let Ok(ref mut tx_part_ctler) = self.tx_part_ctler.lock() {
            tx_part_ctler.policy_parameters = parameters;
        };
    }

    pub fn set_version(&self, version: u32) {
        if let Ok(ref mut version_manager) = self.version_manager.lock() {
            version_manager.as_mut().unwrap().set_version(version);
        };
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

        let (version, bitrate) = self.version_manager.lock().ok()?.as_ref()?.get_version_bitrate();
    
        Some(Statistics { rtt, channel_rtts, outage_rate, ch_outage_rates, throughput, throttle, version, bitrate })
    }

    pub fn start(&mut self, index:usize, tx_ipaddr:String) -> JoinHandle<()> {
        let throttler = Arc::clone(&self.throttler);
        let tx_part_ctler = Arc::clone(&self.tx_part_ctler);
        let version_manager = Arc::clone(&self.version_manager);

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
            else if params.npy_file.ends_with(".npy") {
                source_thread(throttler, tx_part_ctler, rtt_tx, params, socket_infos); 
            }
            else {
                video_thread(throttler, tx_part_ctler, version_manager, rtt_tx, params, socket_infos);
            }
        });

        println!("{}. {} on ...", index, self.stream);
        source
    }
}
