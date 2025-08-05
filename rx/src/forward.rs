use core::socket::*;
use std::sync::mpsc::Receiver;


pub fn forward_thread(forward_port: u16, rx: Receiver<Vec<u8>>){
    let target_addr = format!("127.0.0.1:{}", forward_port);
    let forward_sock = create_udp_socket(128, format!("0.0.0.0"))
        .expect("Failed to create UDP socket for forwarding");
    loop {
        // Receive data from the channel
        match rx.recv() {
            Ok(data) => {
                // Send the data to the target address
                if let Err(e) = forward_sock.send_to(&data, &target_addr) {
                    eprintln!("Error forwarding data: {}", e);
                }
            }
            Err(e) => {
                eprintln!("Error receiving from channel: {}", e);
                // Optionally, handle the error (retry, exit, etc.)
                break;
            }
        }
    }
}