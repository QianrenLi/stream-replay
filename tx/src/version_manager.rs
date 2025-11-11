use std::fs::File;
use std::io::BufReader;

use serde::Deserialize;

/// One segment file inside a version.
#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
struct FileEntry {
    pub slot_index: usize,
    pub start_frame: u64,
    pub end_frame: u64,
    pub path: String,
}

/// A specific encoded version (typically tied to a bitrate).
#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
struct Version {
    pub label: String,
    pub bitrate_bps: u64,
    pub dir: String,
    pub files: Vec<FileEntry>,
}

/// Top-level JSON schema.
#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
struct Config {
    pub input: String,
    pub width: u32,
    pub height: u32,
    pub fps: u32,
    pub slot_seconds: u32,
    pub frames_per_slot: u32,
    pub bitrates_bps: Vec<u64>,
    pub slots: usize,
    pub versions: Vec<Version>,
}

/// Manages the loaded config and a selected "version" (by bitrate).
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct VersionManager {
    cfg: Config,
    pub current_version: u32,
    pub actual_bitrate: u64,
    current_bitrate: u64,
    current_slot: u32,
}

#[allow(dead_code)]
impl VersionManager {
    /// Load the JSON config from a file path.
    pub fn new(path: &String) -> Self {
        let file = File::open(path).unwrap();
        let reader = BufReader::new(file);
        let cfg: Config = serde_json::from_reader(reader).unwrap();
        let initial_version = 0;
        let bitrate = cfg.bitrates_bps[initial_version];
        Self {
            cfg,
            current_version: initial_version as u32,
            current_bitrate: bitrate,
            actual_bitrate: bitrate,
            current_slot: 0,
        }
    }

    pub fn next(&mut self) -> &String{
        let slot = self.current_slot as usize;
        self.current_slot += 1;
        // println!("Switching to slot {} (version {})\n", slot, self.current_version);
        if self.current_slot >= self.cfg.slots as u32 {
            self.current_slot = 0;
        }
        self.actual_bitrate = self.cfg.bitrates_bps[self.current_version as usize];
        &self.cfg.versions[self.current_version as usize].files[slot].path
    }

    pub fn available_bitrates(&self) -> &[u64] {
        &self.cfg.bitrates_bps
    }

    pub fn set_version(&mut self, version_index: u32) {
        if (version_index as usize) < self.cfg.versions.len() {
            self.current_version = version_index;
            self.current_bitrate = self.cfg.versions[version_index as usize].bitrate_bps;
        }
    }

    pub fn get_bitrate(&self) -> u64 {
        self.actual_bitrate
    }

}