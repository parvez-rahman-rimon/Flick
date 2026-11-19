use crate::config::{get_active_ipv4_subnets, get_config, get_lan_ip, get_subnet_broadcast};
use hmac::{Hmac, Mac};
use serde_json::json;
use sha2::Sha256;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, UdpSocket};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const DISCOVERY_PROBE_PORT: u16 = 45457;

type HmacSha256 = Hmac<Sha256>;

pub fn compute_rolling_auth_tag(pair_secret: &str, pc_id: &str) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let time_step = now / 30;
    let msg = format!("{}{}", time_step, pc_id);

    if let Ok(mut mac) = HmacSha256::new_from_slice(pair_secret.as_bytes()) {
        mac.update(msg.as_bytes());
        let result = mac.finalize();
        hex::encode(result.into_bytes()).to_lowercase()
    } else {
        String::new()
    }
}

#[allow(dead_code)]
pub struct BeaconHandle {
    running: Arc<AtomicBool>,
    thread_handle: Option<thread::JoinHandle<()>>,
    probe_handle: Option<thread::JoinHandle<()>>,
}

#[allow(dead_code)]
impl BeaconHandle {
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(th) = self.thread_handle.take() {
            let _ = th.join();
        }
        if let Some(ph) = self.probe_handle.take() {
            let _ = ph.join();
        }
    }
}

fn build_discovery_payload(ip: &str) -> Option<Vec<u8>> {
    let cfg = get_config();
    let now_sec = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut auth_tags: HashMap<String, String> = HashMap::new();
    let mut primary_auth_tag = String::new();

    for dev in &cfg.paired_devices {
        if !dev.pair_secret.is_empty() {
            let tag = compute_rolling_auth_tag(&dev.pair_secret, &cfg.pc_id);
            if primary_auth_tag.is_empty() {
                primary_auth_tag = tag.clone();
            }
            auth_tags.insert(dev.client_id.clone(), tag);
        }
    }

    let payload = json!({
        "protocol": "FLICK_DISCOVERY_V2",
        "pc_id": cfg.pc_id,
        "pc_name": cfg.pc_name,
        "ip": ip,
        "http_port": cfg.http_port,
        "https_port": cfg.http_port,
        "version": "2.0.1",
        "timestamp": now_sec,
        "auth_tag": primary_auth_tag,
        "auth_tags": auth_tags
    });

    serde_json::to_vec(&payload).ok()
}

pub fn start_discovery_beacon(port: u16) -> BeaconHandle {
    let running = Arc::new(AtomicBool::new(true));
    let r_clone = Arc::clone(&running);
    let r_probe = Arc::clone(&running);

    let thread_handle = thread::Builder::new()
        .name("flick-beacon".to_string())
        .stack_size(256 * 1024)
        .spawn(move || {
            let socket = match UdpSocket::bind("0.0.0.0:0") {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[Beacon] Failed to bind UDP socket: {:?}", e);
                    return;
                }
            };
            let _ = socket.set_broadcast(true);

            while r_clone.load(Ordering::SeqCst) {
                let lan_ip = get_lan_ip();
                let subnets = get_active_ipv4_subnets();

                if let Some(bytes) = build_discovery_payload(&lan_ip) {
                    let target_global = format!("255.255.255.255:{}", port);
                    let _ = socket.send_to(&bytes, &target_global);

                    let subnet_bcast = get_subnet_broadcast(&lan_ip);
                    if subnet_bcast != "255.255.255.255" {
                        let _ = socket.send_to(&bytes, format!("{}:{}", subnet_bcast, port));
                    }
                }

                // Also broadcast bound to each active adapter IP so Hotspot / multi-homed
                // adapters always receive discovery beacons even when another NIC is primary.
                for (iface_ip, bcast_ip) in subnets {
                    if let Ok(iface_sock) = UdpSocket::bind((iface_ip, 0)) {
                        let _ = iface_sock.set_broadcast(true);
                        if let Some(bytes) = build_discovery_payload(&iface_ip.to_string()) {
                            let _ = iface_sock.send_to(&bytes, (bcast_ip, port));
                        }
                    }
                }

                // 1.0s periodic cadence (matches Android's discovery window cleanly)
                for _ in 0..10 {
                    if !r_clone.load(Ordering::SeqCst) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
            }
        })
        .expect("Failed to spawn beacon thread");

    // Instant Probe Responder on UDP :45457 for < 50ms Offline/Hotspot discovery
    let probe_handle = thread::Builder::new()
        .name("flick-beacon-probe".to_string())
        .stack_size(256 * 1024)
        .spawn(move || {
            let sock = match UdpSocket::bind(("0.0.0.0", DISCOVERY_PROBE_PORT)) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[BeaconProbe] Failed to bind probe socket: {:?}", e);
                    return;
                }
            };
            let _ = sock.set_read_timeout(Some(Duration::from_millis(500)));
            let mut buf = [0u8; 512];

            while r_probe.load(Ordering::SeqCst) {
                if let Ok((len, src_addr)) = sock.recv_from(&mut buf) {
                    if !buf[..len].starts_with(b"FLICK_PROBE") {
                        continue;
                    }
                    let reply_ip = match src_addr.ip() {
                        IpAddr::V4(peer_v4) => {
                            let peer_oct = peer_v4.octets();
                            get_active_ipv4_subnets()
                                .into_iter()
                                .find(|(ip, _)| {
                                    let o = ip.octets();
                                    o[0] == peer_oct[0] && o[1] == peer_oct[1] && o[2] == peer_oct[2]
                                })
                                .map(|(ip, _)| ip.to_string())
                                .unwrap_or_else(get_lan_ip)
                        }
                        IpAddr::V6(_) => get_lan_ip(),
                    };

                    if let Some(bytes) = build_discovery_payload(&reply_ip) {
                        let _ = sock.send_to(&bytes, src_addr);
                        if let Ok(ip_parsed) = reply_ip.parse::<Ipv4Addr>() {
                            if let Ok(reply_sock) = UdpSocket::bind((ip_parsed, 0)) {
                                let _ = reply_sock.send_to(&bytes, (src_addr.ip(), port));
                            }
                        } else {
                            let _ = sock.send_to(&bytes, (src_addr.ip(), port));
                        }
                    }
                }
            }
        })
        .expect("Failed to spawn beacon probe thread");

    BeaconHandle {
        running,
        thread_handle: Some(thread_handle),
        probe_handle: Some(probe_handle),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discovery_payload_contains_protocol_v2_and_ip() {
        let bytes = build_discovery_payload("192.168.137.42").expect("payload json");
        let parsed: serde_json::Value = serde_json::from_slice(&bytes).expect("valid json");
        assert_eq!(parsed["protocol"], "FLICK_DISCOVERY_V2");
        assert_eq!(parsed["ip"], "192.168.137.42");
    }
}
