use std::collections::HashMap;
use core::socket::{*};

use crate::link::Link;
use crate::source::SocketInfo;

pub fn dispatch(links: Vec<Link>, tos:u8) -> SocketInfo {
    // create Hashmap for each tx_ipaddr and set each non blocking
    let mut socket_infos = HashMap::new();

    for link in links.iter() {
        let tx_ipaddr = link.tx_ipaddr.clone();
        let rx_addr =  format!("{}",link.rx_ipaddr.clone());
        let socket = create_udp_socket(tos, tx_ipaddr.clone());
        if let Some(socket) = socket {
            socket.set_nonblocking(true).unwrap();
            socket_infos.insert(tx_ipaddr.clone(),  (socket, rx_addr));
        }
        else{
            eprintln!("Socket creation failure: ip_addr {} tos {}.", tx_ipaddr, tos);
            break;
        }
    }
    socket_infos
}