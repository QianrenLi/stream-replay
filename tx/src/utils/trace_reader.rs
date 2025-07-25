use std::fs::File;
use std::io::{self, Read};
use std::vec::Vec;

pub fn read_packets(packets_file: &str) -> io::Result<Vec<(u64, Vec<u8>)>> {
    let mut stored_packets: Vec<(u64, Vec<u8>)> = Vec::new();
    let mut file = File::open(packets_file)?;

    loop {
        // Read metadata (interval_ns and data length)
        let mut metadata = [0u8; 16]; // 16 bytes for two u64 values
        let bytes_read = file.read(&mut metadata)?;

        if bytes_read == 0 {
            break; // End of file
        }

        // Unpack the metadata: the first 8 bytes are interval_ns, the next 8 bytes are data_length
        let interval_ns = u64::from_be_bytes(metadata[0..8].try_into().unwrap());
        let data_length = u64::from_be_bytes(metadata[8..16].try_into().unwrap());

        // Read the actual data
        let mut data = vec![0u8; data_length as usize];
        file.read_exact(&mut data)?;

        // Store the packet with interval_ns and data
        stored_packets.push((interval_ns, data));
    }

    Ok(stored_packets)
}