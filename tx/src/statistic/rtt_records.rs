// use std::cmp::Ordering;
#[derive(Debug, Clone)]
struct RTTEntry {
    seq: usize,
    rtt: f64,
    delta: f64,
}

impl RTTEntry {
    fn new(seq: usize) -> Self {
        RTTEntry {
            seq,
            rtt: 0.0,
            delta: 0.0,
        }
    }

    fn update_value(&mut self, value: f64, delta : f64) {
        self.rtt = value;
        self.delta = delta;
    }
}

pub struct RttRecords {
    queue: Vec<Option<RTTEntry>>,
    target_rtt: f64,
    max_length: usize,
    max_links: usize,
}

impl RttRecords {
    pub fn new(max_length: usize, max_links: usize, target_rtt: f64) -> Self {
        RttRecords {
            queue: vec![None; max_length],
            target_rtt,
            max_length,
            max_links,
        }
    }

    pub fn update(&mut self, seq: usize, channel: u8, rtt: f64, delta: f64) {
        let index = seq % self.max_length;
        // If the entry is already present and seq value is the same, update the value
        // Otherwise, create a new entry
        match &mut self.queue[index] {
            Some(entry) => {
                if entry.seq == seq {
                    entry.update_value(rtt, delta);
                } else {
                    self.queue[index] = Some(RTTEntry::new(seq));
                    self.queue[index].as_mut().unwrap().update_value(rtt, delta);
                }
            }
            None => {
                self.queue[index] = Some(RTTEntry::new(seq));
                self.queue[index].as_mut().unwrap().update_value( rtt, delta);
            }
        }
    }

    pub fn statistic(&mut self) -> (f64, f64) {
        // Vectors to store RTT and channel RTT values
    
        let mut outages = 0.0;
        let mut rtts = 0.0;
        let mut count = 0;
    
        for entry in &mut self.queue {
            if let Some(ref mut entry) = entry {
                rtts += entry.rtt;
                if entry.rtt > self.target_rtt {
                    outages += (entry.rtt - self.target_rtt) / self.target_rtt;
                }
                count += 1;
            }
            *entry = None;
        }
    
        let rtt_avg = if count == 0 { 0.0 } else { rtts / count as f64 };
    
        let outage_rate = if count == 0 { 0.0 } else { outages / count as f64 };

        (rtt_avg, outage_rate)
    }
    
}
