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
    #[serde(default)]
    pub paired_devices: Vec<PairedDevice>,
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
                return local_addr.ip().to_string();
            }
        }
    }
    "127.0.0.1".to_string()
}

pub fn get_subnet_broadcast(ip: &str) -> String {
    let parts: Vec<&str> = ip.split('.').collect();
    if parts.len() == 4 {
        format!("{}.{}.{}.255", parts[0], parts[1], parts[2])
    } else {
        "255.255.255.255".to_string()
    }
}
