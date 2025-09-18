use regex::Regex;
use std::process::Command;

pub fn get_dev_from_ip(ip: &str) -> Option<String> {
    // One line per address → easier to parse
    let output = Command::new("ip")
        .arg("-o")
        .arg("addr")
        .arg("show")
        .output()
        .ok()?;

    let s = String::from_utf8_lossy(&output.stdout);

    let re = Regex::new(r#"^\d+:\s+(\S+)\s+(inet6?|inet)\s+([0-9A-Fa-f\.:]+)(?:/\d+)"#).unwrap();

    let want_v6 = ip.contains(':');

    for line in s.lines() {
        if let Some(c) = re.captures(line) {
            let dev = &c[1];
            let fam = &c[2]; // "inet" or "inet6"
            let addr = &c[3];

            let fam_match = (want_v6 && fam == "inet6") || (!want_v6 && fam == "inet");
            if fam_match && addr.eq_ignore_ascii_case(ip) {
                return Some(dev.to_string());
            }
        }
    }
    None
}
