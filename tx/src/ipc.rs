use std::{net::UdpSocket, collections::HashMap, time::{Duration, SystemTime}};
use serde::{Serialize, Deserialize};
use crate::{policies::PolicyParameter, source::SourceManager, statistic::mac_queue::MACQueuesSnapshot};

#[derive(Serialize, Deserialize, Debug,Clone)]
pub struct FlowStatistics {
    pub rtt: f64,
    pub outage_rate: f64,
    pub throughput: f64,
    pub throttle: f64,
    pub bitrate: u64,
    pub app_buff: usize,
    pub frame_count: usize,
}

#[derive(Serialize, Deserialize, Debug,Clone)]
pub struct Statistics {
    pub flow_stat: HashMap<String, FlowStatistics>,
    pub device_stat: MACQueuesSnapshot,
}

#[derive(Serialize, Deserialize, Debug,Clone)]
pub struct ControlInfo {
    pub version: u32,
    pub policy_parameters: PolicyParameter,
}

#[derive(Serialize, Deserialize, Debug,Clone)]
enum RequestValue {
    Throttle(HashMap<String, f64>),
    PolicyParameters(HashMap<String, PolicyParameter>),
    Statistics(HashMap<String, f64>),
    Version(HashMap<String, u32>),
    Control(HashMap<String, ControlInfo>),
}

#[derive(Serialize, Deserialize, Debug,Clone)]
enum ResponseValue {
    Statistics(Statistics),
}

#[derive(Serialize, Deserialize, Debug,Clone)]
struct Request {
    cmd: RequestValue,
}

#[derive(Serialize, Deserialize, Debug,Clone)]
struct Response {
    cmd: ResponseValue,
}

pub struct IPCDaemon {
    ipc_port: u16,
    tx_ipaddr: String,
    sources: HashMap<String, SourceManager>
}

impl IPCDaemon {
    pub fn new(sources: HashMap<String, SourceManager>, ipc_port: u16, tx_ipaddr:String) -> Self {
        Self{ sources, ipc_port, tx_ipaddr }
    }

    fn handle_request(&self, req:Request) -> Option<Response> {
        match req.cmd {
            RequestValue::Throttle(data) => {
                let _:Vec<_> = data.iter().map(|(name, value)| {
                    self.sources[name].throttle(*value);
                }).collect();
                return None;
            },

            RequestValue::PolicyParameters(data) => {
                let _:Vec<_> = data.iter().map(|(name, value)| {
                    self.sources[name].set_policy_parameters(*value);
                }).collect();
                return None;
            },

            RequestValue::Version(data) => {
                let _:Vec<_>  = data.iter().map(|(name, value)| {
                    self.sources[name].set_version(*value);
                }).collect();
                return None;
            },

            RequestValue::Control(data) => {
                let _:Vec<_> = data.iter().map(|(name, value)| {
                    self.sources[name].set_version(value.version);
                    self.sources[name].set_policy_parameters(value.policy_parameters);
                }).collect();
                return None;
            },

            RequestValue::Statistics(_)  => {
                let flow_stat = Some( self.sources.iter().filter_map(|(name,src)| {
                    match src.statistics() {
                        Some(stat) => Some(( name.clone(), stat )),
                        None => None
                    }
                }).collect() ).unwrap();

                //get device statistics from only one source
                let first_source = self.sources.values().next().unwrap();
                let device_stat = first_source.device_statistics();

                return Some(Response{ cmd: ResponseValue::Statistics(
                    Statistics{ flow_stat, device_stat }
                ) });
            }
        }
    }

    pub fn start_loop(&self, duration:f64) {
        let deadline = SystemTime::now() + Duration::from_secs_f64(duration);
        let addr = format!("{}:{}",self.tx_ipaddr, self.ipc_port);
        let sock = UdpSocket::bind(&addr).unwrap();
        sock.set_nonblocking(true).unwrap();
        let mut buf = [0; 2048];

        while SystemTime::now() < deadline {
            if let Ok((len, src_addr)) = sock.recv_from(&mut buf) {
                let buf_str = std::str::from_utf8(&buf[..len]).unwrap();
                let req = serde_json::from_str::<Request>(buf_str).unwrap();
                if let Some(res) = self.handle_request(req) {
                    let res = serde_json::to_string(&res).unwrap();
                    sock.send_to(res.as_bytes(), src_addr).unwrap();
                }
            }
            std::thread::sleep( Duration::from_nanos(10_000_000) );
        }
    }
}
