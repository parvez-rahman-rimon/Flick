use crate::config::{get_config, get_lan_ip, get_subnet_broadcast};
use hmac::{Hmac, Mac};
use serde_json::json;
use sha2::Sha256;
use std::collections::HashMap;
use std::net::UdpSocket;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

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
}

#[allow(dead_code)]
impl BeaconHandle {
    pub fn stop(&mut self) {
        self.running.store(false, Ordering::SeqCst);
        if let Some(th) = self.thread_handle.take() {
            let _ = th.join();
        }
    }
}

pub fn start_discovery_beacon(port: u16) -> BeaconHandle {
    let running = Arc::new(AtomicBool::new(true));
    let r_clone = Arc::clone(&running);

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
                let cfg = get_config();
                let lan_ip = get_lan_ip();
                let subnet_bcast = get_subnet_broadcast(&lan_ip);

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
                    "ip": lan_ip,
                    "http_port": cfg.http_port,
                    "https_port": cfg.http_port,
                    "version": "2.0.0",
                    "timestamp": now_sec,
                    "auth_tag": primary_auth_tag,
                    "auth_tags": auth_tags
                });

                if let Ok(bytes) = serde_json::to_vec(&payload) {
                    let target_global = format!("255.255.255.255:{}", port);
                    let target_subnet = format!("{}:{}", subnet_bcast, port);

                    let _ = socket.send_to(&bytes, &target_global);
                    if subnet_bcast != "255.255.255.255" {
                        let _ = socket.send_to(&bytes, &target_subnet);
                    }
                }

                for _ in 0..20 {
                    if !r_clone.load(Ordering::SeqCst) {
                        break;
                    }
                    thread::sleep(Duration::from_millis(100));
                }
            }
        })
        .expect("Failed to spawn beacon thread");

    BeaconHandle {
        running,
        thread_handle: Some(thread_handle),
    }
}
