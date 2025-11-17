use std::time::SystemTime;

// use std::cmp::Ordering;
#[derive(Debug, Clone)]
struct RTTEntry {
    seq: usize,
    // rtt: f64,
    arrival_time: f64,
    last_outage_time: f64,
    pong_time: Option<f64>,
    delta: f64,
}

impl RTTEntry {
    fn new(seq: usize, arrival_time: f64, last_outage_time: f64) -> Self {
        RTTEntry {
            seq,
            arrival_time: arrival_time,
            last_outage_time: last_outage_time,
            pong_time: None,
            delta: 0.0,
        }
    }

    fn update_value(&mut self, value: f64, delta : f64) -> f64 {
        self.pong_time = Some(value);
        self.delta = delta;
        return value - self.arrival_time;
    }
}

pub struct RttRecords {
    queue: Vec<Option<RTTEntry>>,
    target_rtt: f64,
    max_length: usize,
}

impl RttRecords {
    pub fn new(max_length: usize, target_rtt: f64) -> Self {
        RttRecords {
            queue: vec![None; max_length],
            target_rtt,
            max_length,
        }
    }

    pub fn update_arrival(&mut self, seq: usize, arrival_time: f64){
        let index = seq % self.max_length;
        self.queue[index] = Some(RTTEntry::new(seq, arrival_time, arrival_time + self.target_rtt));
    }

    pub fn update(&mut self, seq: usize, rtt: f64, delta: f64) -> f64 {
        let index = seq % self.max_length;
        match &mut self.queue[index] {
            Some(entry) => {
                if entry.seq == seq {
                    return entry.update_value(rtt, delta);
                } else {
                    panic!();
                }
            }
            None => {
                panic!();
            }
        }
    }

    pub fn statistic(&mut self) -> (f64, f64) {
        // Vectors to store RTT and channel RTT values
    
        let mut outages = 0.0;
        let mut rtts = 0.0;
        let mut count = 0;
        let current_time = SystemTime::now().duration_since(SystemTime::UNIX_EPOCH).unwrap().as_secs_f64();

        for slot in &mut self.queue {
            if let Some(ref mut entry) = slot {
                if let Some(pong_time) = entry.pong_time {
                    rtts += pong_time - entry.arrival_time;
                    count += 1;
                    if pong_time > entry.last_outage_time {
                        outages += pong_time - entry.last_outage_time;
                        entry.last_outage_time = pong_time;
                    }
                    *slot = None;
                }
                else {
                    if current_time > entry.last_outage_time {
                        outages += current_time - entry.last_outage_time;
                        entry.last_outage_time = current_time;
                    }
                }
            }
            
        }
    
        let rtt_avg = if count == 0 { 0.0 } else { rtts / count as f64 };
    
        let outage_rate =  outages / self.target_rtt;

        (rtt_avg, outage_rate)
    }
    
}
