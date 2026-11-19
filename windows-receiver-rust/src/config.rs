use serde::{Deserialize, Serialize};
use std::fs;
use std::net::UdpSocket;
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::time::{SystemTime, UNIX_EPOCH};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PairedDevice {
    pub client_id: String,
    pub client_name: String,
    #[serde(default)]
    pub client_spki_pin: String,
    #[serde(default)]
    pub client_cert_pin: String,
    #[serde(default)]
    pub pair_secret: String,
    #[serde(default)]
    pub paired_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FlickConfig {
    pub pc_id: String,
    pub pc_name: String,
    #[serde(default = "default_http_port")]
    pub http_port: u16,
    #[serde(default = "default_udp_port")]
    pub udp_port: u16,
    #[serde(default)]
    pub auth_token: String,
    #[serde(default = "default_save_directory")]
    pub save_directory: String,
    #[serde(default = "default_clipboard_mode")]
    pub clipboard_mode: String,
    #[serde(default = "default_play_sound")]
    pub play_sound: bool,
    #[serde(default = "default_winv_history_stack")]
    pub winv_history_stack: bool,
    #[serde(default = "default_sound_theme")]
    pub sound_theme: String,
    #[serde(default)]
    pub paired_devices: Vec<PairedDevice>,
}

fn default_sound_theme() -> String {
    "glass".to_string()
}

fn default_http_port() -> u16 {
    45455
}
fn default_udp_port() -> u16 {
    45454
}
fn default_play_sound() -> bool {
    true
}
fn default_winv_history_stack() -> bool {
    true
}
fn default_clipboard_mode() -> String {
    "SMART_MULTI".to_string()
}
fn default_save_directory() -> String {
    let home = get_home_dir();
    home.join("Downloads").join("Flick").to_string_lossy().to_string()
}

pub fn get_home_dir() -> PathBuf {
    if let Ok(prof) = std::env::var("USERPROFILE") {
        PathBuf::from(prof)
    } else if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
    } else {
        PathBuf::from(".")
    }
}

pub fn get_flick_dir() -> PathBuf {
    get_home_dir().join(".flick")
}

pub fn get_config_path() -> PathBuf {
    get_flick_dir().join("config.json")
}

pub fn get_transfers_path() -> PathBuf {
    get_flick_dir().join("pc_recent_transfers.json")
}

pub fn get_pair_secret_path() -> PathBuf {
    get_flick_dir().join("pair_secret.json")
}

static CONFIG_LOCK: Mutex<()> = Mutex::new(());
static CACHED_CONFIG: OnceLock<Mutex<Option<FlickConfig>>> = OnceLock::new();

fn get_config_cache() -> &'static Mutex<Option<FlickConfig>> {
    CACHED_CONFIG.get_or_init(|| Mutex::new(None))
}

fn init_default_config() -> FlickConfig {
    let hostname = std::env::var("COMPUTERNAME").unwrap_or_else(|_| "Flick-PC".to_string());
    let mut rng_bytes = [0u8; 32];
    use rand::RngCore;
    rand::thread_rng().fill_bytes(&mut rng_bytes);
    let token = hex::encode(rng_bytes);

    let default_dir = get_home_dir().join("Downloads").join("Flick");
    let _ = fs::create_dir_all(&default_dir);

    FlickConfig {
        pc_id: Uuid::new_v4().to_string(),
        pc_name: hostname,
        http_port: 45455,
        udp_port: 45454,
        auth_token: token,
        save_directory: default_dir.to_string_lossy().to_string(),
        clipboard_mode: "SMART_MULTI".to_string(),
        play_sound: true,
        winv_history_stack: true,
        sound_theme: "glass".to_string(),
        paired_devices: Vec::new(),
    }
}

pub fn load_config() -> FlickConfig {
    let _guard = CONFIG_LOCK.lock().unwrap();
    let flick_dir = get_flick_dir();
    let _ = fs::create_dir_all(&flick_dir);

    let config_path = get_config_path();
    if !config_path.exists() {
        let cfg = init_default_config();
        if let Ok(json_str) = serde_json::to_string_pretty(&cfg) {
            let _ = fs::write(&config_path, json_str);
        }
        let mut cache = get_config_cache().lock().unwrap();
        *cache = Some(cfg.clone());
        return cfg;
    }

    match fs::read_to_string(&config_path) {
        Ok(content) => match serde_json::from_str::<FlickConfig>(&content) {
            Ok(cfg) => {
                let mut cache = get_config_cache().lock().unwrap();
                *cache = Some(cfg.clone());
                cfg
            }
            Err(_) => {
                let cfg = init_default_config();
                let mut cache = get_config_cache().lock().unwrap();
                *cache = Some(cfg.clone());
                cfg
            }
        },
        Err(_) => {
            let cfg = init_default_config();
            let mut cache = get_config_cache().lock().unwrap();
            *cache = Some(cfg.clone());
            cfg
        }
    }
}

pub fn get_config() -> FlickConfig {
    {
        let cache = get_config_cache().lock().unwrap();
        if let Some(cfg) = &*cache {
            return cfg.clone();
        }
    }
    load_config()
}

pub fn save_config(cfg: &FlickConfig) {
    let _guard = CONFIG_LOCK.lock().unwrap();
    let flick_dir = get_flick_dir();
    let _ = fs::create_dir_all(&flick_dir);
    let config_path = get_config_path();
    if let Ok(json_str) = serde_json::to_string_pretty(cfg) {
        let _ = fs::write(config_path, json_str);
    }
    let mut cache = get_config_cache().lock().unwrap();
    *cache = Some(cfg.clone());
}

pub fn set_clipboard_mode(mode: &str) {
    let mut cfg = get_config();
    cfg.clipboard_mode = mode.to_string();
    save_config(&cfg);
}

pub fn set_save_directory(dir: &str) {
    let mut cfg = get_config();
    cfg.save_directory = dir.to_string();
    save_config(&cfg);
}

pub fn set_winv_history_stack(enabled: bool) {
    let mut cfg = get_config();
    cfg.winv_history_stack = enabled;
    save_config(&cfg);
}

pub fn is_autostart_enabled() -> bool {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegOpenKeyExW, RegQueryValueExW, HKEY_CURRENT_USER, KEY_READ,
    };

    let subkey_wide: Vec<u16> = OsStr::new("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
        .encode_wide()
        .chain(Some(0))
        .collect();
    let val_wide: Vec<u16> = OsStr::new("Flick")
        .encode_wide()
        .chain(Some(0))
        .collect();

    let mut hkey = 0 as isize;
    unsafe {
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey_wide.as_ptr(), 0, KEY_READ, &mut hkey as *mut _ as _) == 0 {
            let mut typ = 0u32;
            let mut size = 0u32;
            let status = RegQueryValueExW(
                hkey as _,
                val_wide.as_ptr(),
                std::ptr::null(),
                &mut typ,
                std::ptr::null_mut(),
                &mut size,
            );
            RegCloseKey(hkey as _);
            return status == 0;
        }
    }
    false
}

pub fn set_autostart_enabled(enable: bool) -> bool {
    use std::ffi::OsStr;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::System::Registry::{
        RegCloseKey, RegDeleteValueW, RegOpenKeyExW, RegSetValueExW, HKEY_CURRENT_USER, KEY_WRITE,
        REG_SZ,
    };

    let subkey_wide: Vec<u16> = OsStr::new("Software\\Microsoft\\Windows\\CurrentVersion\\Run")
        .encode_wide()
        .chain(Some(0))
        .collect();
    let val_wide: Vec<u16> = OsStr::new("Flick")
        .encode_wide()
        .chain(Some(0))
        .collect();

    let mut hkey = 0 as isize;
    unsafe {
        if RegOpenKeyExW(HKEY_CURRENT_USER, subkey_wide.as_ptr(), 0, KEY_WRITE, &mut hkey as *mut _ as _) != 0 {
            return false;
        }

        let success = if enable {
            let current_exe = std::env::current_exe().unwrap_or_else(|_| std::path::PathBuf::from("Flick.exe"));
            let cmd_str = format!("\"{}\" --minimized", current_exe.to_string_lossy());
            let cmd_wide: Vec<u16> = OsStr::new(&cmd_str).encode_wide().chain(Some(0)).collect();
            let size = (cmd_wide.len() * 2) as u32;
            RegSetValueExW(
                hkey as _,
                val_wide.as_ptr(),
                0,
                REG_SZ,
                cmd_wide.as_ptr() as _,
                size,
            ) == 0
        } else {
            let _ = RegDeleteValueW(hkey as _, val_wide.as_ptr());
            true
        };

        RegCloseKey(hkey as _);
        success
    }
}

#[allow(dead_code)]
pub fn set_play_sound(play: bool) {
    let mut cfg = get_config();
    cfg.play_sound = play;
    save_config(&cfg);
}

pub fn add_or_update_paired_device(
    client_id: &str,
    client_name: &str,
    client_spki_pin: &str,
    client_cert_pin: &str,
    pair_secret: &str,
) {
    let mut cfg = get_config();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let mut found = false;
    for dev in &mut cfg.paired_devices {
        if dev.client_id == client_id {
            dev.client_name = client_name.to_string();
            if !client_spki_pin.is_empty() {
                dev.client_spki_pin = client_spki_pin.to_string();
            }
            if !client_cert_pin.is_empty() {
                dev.client_cert_pin = client_cert_pin.to_string();
            }
            if !pair_secret.is_empty() {
                dev.pair_secret = pair_secret.to_string();
            }
            dev.paired_at = now;
            found = true;
            break;
        }
    }

    if !found {
        cfg.paired_devices.push(PairedDevice {
            client_id: client_id.to_string(),
            client_name: client_name.to_string(),
            client_spki_pin: client_spki_pin.to_string(),
            client_cert_pin: client_cert_pin.to_string(),
            pair_secret: pair_secret.to_string(),
            paired_at: now,
        });
    }

    save_config(&cfg);
}

pub fn set_sound_theme(theme: &str) {
    let mut cfg = get_config();
    cfg.sound_theme = theme.to_string();
    let _ = save_config(&cfg);
}

pub fn remove_paired_device(client_id: &str) {
    let mut cfg = get_config();
    cfg.paired_devices.retain(|d| d.client_id != client_id);
    save_config(&cfg);
}

pub fn is_device_paired(spki_pin: &str, cert_pin: &str) -> bool {
    let cfg = get_config();
    for dev in &cfg.paired_devices {
        if !dev.client_spki_pin.is_empty()
            && dev.client_spki_pin.eq_ignore_ascii_case(spki_pin)
        {
            return true;
        }
        if !dev.client_cert_pin.is_empty()
            && dev.client_cert_pin.eq_ignore_ascii_case(cert_pin)
        {
            return true;
        }
    }
    false
}

// ----------------------------------------------------------------------------
// Recent Transfers Persistence
// ----------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransferRecord {
    pub id: String,
    pub file_name: String,
    pub file_size: u64,
    pub client_ip: String,
    pub mode: String,
    pub timestamp: u64,
}

static TRANSFERS_LOCK: Mutex<()> = Mutex::new(());

fn load_transfers_unlocked() -> Vec<TransferRecord> {
    let path = get_transfers_path();
    if !path.exists() {
        return Vec::new();
    }
    match fs::read_to_string(path) {
        Ok(s) => serde_json::from_str::<Vec<TransferRecord>>(&s).unwrap_or_default(),
        Err(_) => Vec::new(),
    }
}

pub fn get_recent_transfers() -> Vec<TransferRecord> {
    let _guard = TRANSFERS_LOCK.lock().unwrap();
    load_transfers_unlocked()
}

pub fn record_transfer(file_name: &str, file_size: u64, client_ip: &str, mode: &str) {
    let _guard = TRANSFERS_LOCK.lock().unwrap();
    let mut transfers = load_transfers_unlocked();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();

    let rec = TransferRecord {
        id: Uuid::new_v4().to_string(),
        file_name: file_name.to_string(),
        file_size,
        client_ip: client_ip.to_string(),
        mode: mode.to_string(),
        timestamp: now,
    };

    transfers.insert(0, rec);
    if transfers.len() > 30 {
        transfers.truncate(30);
    }

    if let Ok(json_str) = serde_json::to_string_pretty(&transfers) {
        let _ = fs::write(get_transfers_path(), json_str);
    }
}

pub fn clear_recent_transfers() {
    let _guard = TRANSFERS_LOCK.lock().unwrap();
    let _ = fs::write(get_transfers_path(), "[]");
}

// ----------------------------------------------------------------------------
// Ephemeral Pair Secret State
// ----------------------------------------------------------------------------

#[derive(Serialize, Deserialize)]
struct PairSecretJson {
    secret: String,
    expiry: f64,
}

static PAIR_SECRET_LOCK: Mutex<()> = Mutex::new(());

pub fn set_active_pair_secret(secret: &str, duration_sec: f64) {
    let _guard = PAIR_SECRET_LOCK.lock().unwrap();
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs_f64();
    let expiry = now + duration_sec;
    let data = PairSecretJson {
        secret: secret.to_string(),
        expiry,
    };
    if let Ok(json_str) = serde_json::to_string(&data) {
        let _ = fs::write(get_pair_secret_path(), json_str);
    }
}

pub fn get_active_pair_secret() -> Option<String> {
    let _guard = PAIR_SECRET_LOCK.lock().unwrap();
    let path = get_pair_secret_path();
    if !path.exists() {
        return None;
    }
    if let Ok(content) = fs::read_to_string(path) {
        if let Ok(data) = serde_json::from_str::<PairSecretJson>(&content) {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs_f64();
            if now <= data.expiry {
                return Some(data.secret);
            }
        }
    }
    None
}

pub fn clear_active_pair_secret() {
    let _guard = PAIR_SECRET_LOCK.lock().unwrap();
    let path = get_pair_secret_path();
    if path.exists() {
        let _ = fs::remove_file(path);
    }
}

// ----------------------------------------------------------------------------
// Network Utilities
// ----------------------------------------------------------------------------

pub fn get_lan_ip() -> String {
    if let Ok(socket) = UdpSocket::bind("0.0.0.0:0") {
        if socket.connect("8.8.8.8:80").is_ok() {
            if let Ok(local_addr) = socket.local_addr() {
                let ip_str = local_addr.ip().to_string();
                if ip_str != "127.0.0.1" && !ip_str.starts_with("169.254.") {
                    return ip_str;
                }
            }
        }
    }
    get_lan_ip_from_adapters().unwrap_or_else(|| "127.0.0.1".to_string())
}

fn is_private_rfc1918(ip: std::net::Ipv4Addr) -> bool {
    let oct = ip.octets();
    oct[0] == 192 && oct[1] == 168
        || oct[0] == 10
        || (oct[0] == 172 && (16..=31).contains(&oct[1]))
}

fn compute_broadcast(ip: std::net::Ipv4Addr, prefix_len: u8) -> std::net::Ipv4Addr {
    let shift = 32u32.saturating_sub(prefix_len.min(32) as u32);
    let mask = if shift == 32 { 0 } else { !0u32 << shift };
    std::net::Ipv4Addr::from(u32::from(ip) | !mask)
}

/// Returns `(ip, subnet_broadcast)` pairs for all active non-loopback, non-APIPA IPv4 interfaces,
/// ordered with physical Wi-Fi/Ethernet private RFC1918 networks first.
pub fn get_active_ipv4_subnets() -> Vec<(std::net::Ipv4Addr, std::net::Ipv4Addr)> {
    use std::net::Ipv4Addr;
    use windows_sys::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_SUCCESS};
    use windows_sys::Win32::NetworkManagement::IpHelper::{
        GetAdaptersAddresses, IP_ADAPTER_ADDRESSES_LH,
    };
    use windows_sys::Win32::NetworkManagement::Ndis::IfOperStatusUp;
    use windows_sys::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};

    let mut buf_len: u32 = 16384;
    let mut buf: Vec<u8> = vec![0u8; buf_len as usize];

    let mut ret = unsafe {
        GetAdaptersAddresses(
            AF_INET as u32,
            0,
            std::ptr::null(),
            buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH,
            &mut buf_len,
        )
    };
    if ret == ERROR_BUFFER_OVERFLOW {
        buf.resize(buf_len as usize, 0);
        ret = unsafe {
            GetAdaptersAddresses(
                AF_INET as u32,
                0,
                std::ptr::null(),
                buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH,
                &mut buf_len,
            )
        };
    }
    if ret != ERROR_SUCCESS {
        return Vec::new();
    }

    let mut scored: Vec<(u8, Ipv4Addr, Ipv4Addr)> = Vec::new();
    let mut cur = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
    while !cur.is_null() {
        let adapter = unsafe { &*cur };
        if adapter.OperStatus == IfOperStatusUp {
            // IF_TYPE_IEEE80211 = 71, IF_TYPE_ETHERNET_CSMACD = 6
            let is_phys_lan = adapter.IfType == 71 || adapter.IfType == 6;
            let mut u_cur = adapter.FirstUnicastAddress;
            while !u_cur.is_null() {
                let u = unsafe { &*u_cur };
                if !u.Address.lpSockaddr.is_null() {
                    let sa = unsafe { &*(u.Address.lpSockaddr as *const SOCKADDR_IN) };
                    if sa.sin_family as u32 == AF_INET as u32 {
                        let raw = unsafe { sa.sin_addr.S_un.S_addr };
                        let ip = Ipv4Addr::from(u32::from_be(raw));
                        let oct = ip.octets();
                        if oct[0] != 127 && !(oct[0] == 169 && oct[1] == 254) && !ip.is_unspecified() {
                            let score = match (is_phys_lan, is_private_rfc1918(ip)) {
                                (true, true) => 4,
                                (false, true) => 3,
                                (true, false) => 2,
                                (false, false) => 1,
                            };
                            let bcast = compute_broadcast(ip, u.OnLinkPrefixLength);
                            scored.push((score, ip, bcast));
                        }
                    }
                }
                u_cur = u.Next;
            }
        }
        cur = adapter.Next;
    }

    scored.sort_by(|a, b| b.0.cmp(&a.0));
    scored.into_iter().map(|(_, ip, bcast)| (ip, bcast)).collect()
}

/// Enumerates active Windows network adapters via `GetAdaptersAddresses` so LAN
/// discovery works even on isolated offline hotspots with no default gateway.
fn get_lan_ip_from_adapters() -> Option<String> {
    get_active_ipv4_subnets()
        .into_iter()
        .next()
        .map(|(ip, _)| ip.to_string())
}

pub fn get_subnet_broadcast(ip: &str) -> String {
    let parts: Vec<&str> = ip.split('.').collect();
    if parts.len() == 4 {
        format!("{}.{}.{}.255", parts[0], parts[1], parts[2])
    } else {
        "255.255.255.255".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lan_ip_adapter_enumeration_returns_non_loopback_when_connected() {
        let ip = get_lan_ip();
        assert!(!ip.is_empty());
        if let Some(adapter_ip) = get_lan_ip_from_adapters() {
            assert_ne!(adapter_ip, "127.0.0.1");
        }
    }
}
