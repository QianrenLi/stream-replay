use std::process::Command;

pub fn get_dev_from_ip(ip: &str) -> Option<String> {
    let output = Command::new("ip")
        .arg("addr")
        .arg("show")
        .output()
        .ok()?;

    let output_str = String::from_utf8_lossy(&output.stdout);

    let mut current_dev = None;

    for line in output_str.lines() {
        // Header line: "2: wlx081f7165e561: <...>"
        if let Some(idx) = line.find(": ") {
            let dev_part = &line[idx + 2..];
            if let Some(end_idx) = dev_part.find(':') {
                current_dev = Some(dev_part[..end_idx].to_string());
            }
        }

        // Look for the IP address
        if line.contains(ip) {
            println!("Device for IP {}: {:?}", ip, current_dev);
            return current_dev;
        }
    }

    None
}