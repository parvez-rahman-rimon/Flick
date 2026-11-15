use crate::clipboard;
use crate::config::{
    add_or_update_paired_device, get_config, is_device_paired, record_transfer,
};
use crate::crypto::{calculate_cert_sha256, calculate_spki_sha256, PcCredentials};

use hmac::{Hmac, Mac};
use serde::Deserialize;
use serde_json::json;
use sha2::Sha256;
use std::collections::HashMap;
use std::ffi::OsStr;
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::os::windows::ffi::OsStrExt;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub fn log_http(msg: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(
            std::path::Path::new(&std::env::var("USERPROFILE").unwrap_or_default())
                .join(".flick")
                .join("http.log"),
        )
    {
        use std::io::Write;
        let _ = writeln!(f, "[{}]", msg);
    }
}
use subtle::ConstantTimeEq;
use unicode_normalization::UnicodeNormalization;
use uuid::Uuid;

use windows_sys::Win32::Foundation::HWND;
use windows_sys::Win32::Media::Audio::{PlaySoundW, SND_ASYNC, SND_MEMORY, SND_NODEFAULT};
use windows_sys::Win32::Storage::FileSystem::GetDiskFreeSpaceExW;
use windows_sys::Win32::UI::WindowsAndMessaging::{
    FindWindowW, MessageBoxW, PostMessageW, MB_ICONQUESTION, MB_SETFOREGROUND, MB_TOPMOST,
    MB_YESNO, IDYES,
};

type HmacSha256 = Hmac<Sha256>;

const CHUNK_SIZE: usize = 65536; // 64 KB read buffer
const MAX_ALLOWED_FILE_BYTES: u64 = 4 * 1024 * 1024 * 1024; // 4 GB max
const SAFETY_DISK_BUFFER: u64 = 500 * 1024 * 1024; // 500 MB minimum remaining free space
const MAX_PAIR_BODY_BYTES: usize = 64 * 1024; // 64 KB max
const MAX_CLIPBOARD_BODY_BYTES: usize = 256 * 1024; // 256 KB max

const DANGEROUS_EXTENSIONS: &[&str] = &[
    ".exe", ".bat", ".cmd", ".vbs", ".ps1", ".scr", ".hta", ".lnk", ".msi", ".dll", ".reg",
    ".com", ".cpl", ".jar", ".pif", ".wsf", ".js", ".jse", ".vbe", ".wsh", ".msc", ".xll",
    ".rdp", ".inf", ".appx", ".msix", ".appinstaller", ".application", ".iso", ".vhd", ".vhdx",
    ".chm", ".docm", ".xlsm", ".pptm", ".url", ".scf", ".library-ms", ".searchconnector-ms",
];

const RESERVED_NAMES: &[&str] = &[
    "CON", "PRN", "AUX", "NUL", "COM1", "COM2", "COM3", "COM4", "COM5", "COM6", "COM7", "COM8",
    "COM9", "LPT1", "LPT2", "LPT3", "LPT4", "LPT5", "LPT6", "LPT7", "LPT8", "LPT9",
];

// ----------------------------------------------------------------------------
// High-Fidelity Spatial Audio Engine
// Professional CC0 1.0 Universal Acoustic Drop, Glass & HUD Themes
// ----------------------------------------------------------------------------

static SOUND_GLASS: &[u8] = include_bytes!("../assets/sounds/glass_receive.wav");
static SOUND_SCIFI: &[u8] = include_bytes!("../assets/sounds/scifi_receive.wav");
static SOUND_CINEMATIC: &[u8] = include_bytes!("../assets/sounds/cinematic_receive.wav");
static SOUND_STUDIO: &[u8] = include_bytes!("../assets/sounds/studio_receive.wav");

pub fn get_sound_bytes(theme: &str) -> &'static [u8] {
    match theme {
        "scifi" => SOUND_SCIFI,
        "cinematic" => SOUND_CINEMATIC,
        "studio" => SOUND_STUDIO,
        _ => SOUND_GLASS,
    }
}

pub fn play_arrival_chime() {
    let cfg = get_config();
    if !cfg.play_sound {
        return;
    }
    let wav = get_sound_bytes(&cfg.sound_theme);
    unsafe {
        PlaySoundW(
            wav.as_ptr() as *const u16,
            std::ptr::null_mut(),
            SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
        );
    }
}

pub fn preview_sound_theme(theme: &str) {
    let wav = get_sound_bytes(theme);
    unsafe {
        PlaySoundW(
            wav.as_ptr() as *const u16,
            std::ptr::null_mut(),
            SND_MEMORY | SND_ASYNC | SND_NODEFAULT,
        );
    }
}

// ----------------------------------------------------------------------------
// Filename Sanitizer
// ----------------------------------------------------------------------------

pub fn sanitize_filename(raw: &str, limit: usize) -> (String, bool) {
    let unquoted = urlencoding::decode(raw).unwrap_or_else(|_| raw.into());
    let normalized: String = unquoted.nfc().collect();
    let basename = Path::new(&normalized)
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    let mut cleaned = String::new();
    for ch in basename.chars() {
        if matches!(
            ch,
            '\0'..='\x1f'
                | '<'
                | '>'
                | ':'
                | '"'
                | '/'
                | '\\'
                | '|'
                | '?'
                | '*'
                | '&'
                | ';'
                | '^'
                | '%'
                | '$'
                | '!'
                | '\''
                | '('
                | ')'
                | ','
                | '='
                | '+'
                | '@'
                | '#'
                | '['
                | ']'
                | '{'
                | '}'
                | '~'
                | '\u{200b}'..='\u{200f}'
                | '\u{202a}'..='\u{202e}'
                | '\u{2066}'..='\u{2069}'
        ) {
            cleaned.push('_');
        } else {
            cleaned.push(ch);
        }
    }

    let trimmed = cleaned.trim_matches(|c| c == ' ' || c == '.');
    let name = if trimmed.is_empty() {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        format!("flick_{}", ts)
    } else {
        trimmed.to_string()
    };

    let p = Path::new(&name);
    let mut stem = p
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("file")
        .to_string();
    let ext = p
        .extension()
        .and_then(|s| s.to_str())
        .map(|s| format!(".{}", s.to_lowercase()))
        .unwrap_or_default();

    if stem.is_empty() || RESERVED_NAMES.iter().any(|r| r.eq_ignore_ascii_case(&stem)) {
        stem = format!("file_{}", stem);
    }

    if stem.len() > limit {
        stem = stem[..limit].to_string();
    }

    let mut final_ext = ext;
    let mut is_quarantined = false;

    for dang in DANGEROUS_EXTENSIONS {
        if final_ext.eq_ignore_ascii_case(dang) {
            final_ext.push_str(".blocked");
            is_quarantined = true;
            break;
        }
    }

    (format!("{}{}", stem, final_ext), is_quarantined)
}

fn set_zone_identifier(path: &Path) {
    let path_str = path.to_string_lossy();
    let ads_path = format!("{}:Zone.Identifier", path_str);
    if let Ok(mut f) = OpenOptions::new()
        .create(true)
        .write(true)
        .truncate(true)
        .open(ads_path)
    {
        let _ = f.write_all(b"[ZoneTransfer]\r\nZoneId=3\r\n");
    }
}

fn check_disk_space(dir: &Path, required_bytes: u64) -> bool {
    let wide_dir: Vec<u16> = OsStr::new(dir).encode_wide().chain(Some(0)).collect();
    let mut free_bytes_avail = 0u64;
    let mut total_bytes = 0u64;
    let mut total_free = 0u64;

    unsafe {
        if GetDiskFreeSpaceExW(
            wide_dir.as_ptr(),
            &mut free_bytes_avail,
            &mut total_bytes,
            &mut total_free,
        ) != 0
        {
            return free_bytes_avail >= required_bytes + SAFETY_DISK_BUFFER;
        }
    }
    true
}

// ----------------------------------------------------------------------------
// Batch Multi-File Transfer Accumulator
// ----------------------------------------------------------------------------

struct BatchEntry {
    total: usize,
    paths: Vec<PathBuf>,
    last_update: Instant,
    mode: String,
}

static BATCH_LOCK: Mutex<()> = Mutex::new(());
static ACTIVE_BATCHES: std::sync::OnceLock<Mutex<HashMap<String, BatchEntry>>> =
    std::sync::OnceLock::new();

fn get_batches() -> &'static Mutex<HashMap<String, BatchEntry>> {
    ACTIVE_BATCHES.get_or_init(|| Mutex::new(HashMap::new()))
}

// ----------------------------------------------------------------------------
// Server Implementation
// ----------------------------------------------------------------------------

#[allow(dead_code)]
pub struct HttpServerHandle {
    running: Arc<AtomicBool>,
}

#[allow(dead_code)]
impl HttpServerHandle {
    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }
}

pub fn start_http_receiver(
    port: u16,
    creds: Arc<PcCredentials>,
    server_config: Arc<rustls::ServerConfig>,
) -> HttpServerHandle {
    let running = Arc::new(AtomicBool::new(true));
    let r_clone = Arc::clone(&running);

    let addr = format!("0.0.0.0:{}", port);
    let listener = TcpListener::bind(&addr).expect("Failed to bind TCP listener on port 45455");
    let _ = listener.set_nonblocking(true);

    thread::Builder::new()
        .name("flick-http-accept".to_string())
        .stack_size(256 * 1024)
        .spawn(move || {
            while r_clone.load(Ordering::SeqCst) {
                match listener.accept() {
                    Ok((stream, peer_addr)) => {
                        let cfg_tls = Arc::clone(&server_config);
                        let c_creds = Arc::clone(&creds);

                        let _ = thread::Builder::new()
                            .name("flick-conn-worker".to_string())
                            .stack_size(256 * 1024)
                            .spawn(move || {
                                handle_connection(stream, peer_addr.ip().to_string(), cfg_tls, c_creds);
                            });
                    }
                    Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(15));
                    }
                    Err(_) => {
                        thread::sleep(Duration::from_millis(50));
                    }
                }
            }
        })
        .expect("Failed to spawn HTTP accept thread");

    HttpServerHandle { running }
}

fn handle_connection(
    stream: TcpStream,
    peer_ip: String,
    server_config: Arc<rustls::ServerConfig>,
    _creds: Arc<PcCredentials>,
) {
    log_http(&format!("Incoming TCP conn from {}", peer_ip));
    let _ = stream.set_nonblocking(false);
    let _ = stream.set_read_timeout(Some(Duration::from_secs(30)));
    let _ = stream.set_write_timeout(Some(Duration::from_secs(30)));

    let server_conn = match rustls::ServerConnection::new(server_config) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("[HTTPS] Failed to create ServerConnection: {:?}", e);
            return;
        }
    };

    let mut tls_stream = rustls::StreamOwned::new(server_conn, stream);

    // Read HTTP Request line and headers
    let mut header_buf = Vec::with_capacity(4096);
    let mut byte_buf = [0u8; 1];

    let mut double_crlf_pos = None;
    while header_buf.len() < 32768 {
        match tls_stream.read(&mut byte_buf) {
            Ok(1) => {
                header_buf.push(byte_buf[0]);
                if header_buf.len() >= 4 {
                    let len = header_buf.len();
                    if &header_buf[len - 4..] == b"\r\n\r\n" {
                        double_crlf_pos = Some(len - 4);
                        break;
                    }
                }
            }
            Ok(0) => {
                eprintln!("[HTTPS] EOF reached before end of headers (read {} bytes)", header_buf.len());
                break;
            }
            Ok(n) => {
                eprintln!("[HTTPS] Unexpected read count {}", n);
                break;
            }
            Err(e) => {
                eprintln!("[HTTPS] TLS read error: {:?}", e);
                break;
            }
        }
    }

    let header_end = match double_crlf_pos {
        Some(pos) => pos,
        None => return,
    };

    let header_str = match std::str::from_utf8(&header_buf[..header_end]) {
        Ok(s) => s,
        Err(_) => return,
    };

    let mut lines = header_str.split("\r\n");
    let req_line = match lines.next() {
        Some(l) => l,
        None => return,
    };

    let parts: Vec<&str> = req_line.split_whitespace().collect();
    if parts.len() < 2 {
        return;
    }
    let method = parts[0];
    let path = parts[1];
    log_http(&format!("HTTP Request: {} {} from {}", method, path, peer_ip));

    let mut headers = HashMap::new();
    for l in lines {
        if let Some((k, v)) = l.split_once(':') {
            headers.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
        }
    }

    // Inspect client peer cert from TLS handshake
    let peer_certs = tls_stream.conn.peer_certificates();
    let mut client_spki_pin = String::new();
    let mut client_cert_pin = String::new();
    let mut client_der_bytes: Option<Vec<u8>> = None;

    if let Some(certs) = peer_certs {
        if !certs.is_empty() {
            let der = certs[0].as_ref();
            client_der_bytes = Some(der.to_vec());
            if let Ok(pin) = calculate_spki_sha256(der) {
                client_spki_pin = pin;
            }
            client_cert_pin = calculate_cert_sha256(der);
        }
    }

    // Authenticate client
    let cfg = get_config();
    let is_cert_authed = !client_spki_pin.is_empty()
        && is_device_paired(&client_spki_pin, &client_cert_pin);

    let is_token_authed = if let Some(hdr_token) = headers.get("x-flick-token") {
        hdr_token.as_bytes().ct_eq(cfg.auth_token.as_bytes()).into()
    } else {
        false
    };

    let is_authenticated = is_cert_authed || is_token_authed;

    // Route dispatch
    match (method, path) {
        ("GET", "/ping") => {
            if !is_authenticated {
                send_response(&mut tls_stream, 401, "Unauthorized", "application/json", b"{\"status\":\"error\",\"message\":\"Unauthorized\"}");
                return;
            }
            let resp = json!({
                "status": "ok",
                "pc_id": cfg.pc_id,
                "pc_name": cfg.pc_name,
                "authenticated": true,
                "clipboard_mode": cfg.clipboard_mode,
                "version": "2.0.1"
            });
            send_json_response(&mut tls_stream, 200, "OK", &resp);
        }

        ("POST", "/v2/pair") => {
            handle_v2_pair(
                &mut tls_stream,
                &headers,
                client_der_bytes.as_deref(),
                &client_spki_pin,
                &client_cert_pin,
            );
        }

        ("POST", "/pair") => {
            handle_legacy_pair(&mut tls_stream, &headers, &peer_ip);
        }

        ("POST", "/upload") => {
            if !is_authenticated {
                send_response(&mut tls_stream, 401, "Unauthorized", "application/json", b"{\"status\":\"error\",\"message\":\"Unauthorized\"}");
                return;
            }
            handle_upload(&mut tls_stream, &headers, &peer_ip);
        }

        ("POST", "/clipboard") => {
            if !is_authenticated {
                send_response(&mut tls_stream, 401, "Unauthorized", "application/json", b"{\"status\":\"error\",\"message\":\"Unauthorized\"}");
                return;
            }
            handle_clipboard(&mut tls_stream, &headers, &peer_ip);
        }

        ("POST", "/v2/open-dashboard") => {
            if peer_ip == "127.0.0.1" || peer_ip == "::1" {
                notify_main_window(101);
                send_json_response(&mut tls_stream, 200, "OK", &json!({"status": "ok"}));
            } else {
                send_response(&mut tls_stream, 403, "Forbidden", "text/plain", b"Localhost only");
            }
        }

        ("POST", "/v2/open-pair") => {
            if peer_ip == "127.0.0.1" || peer_ip == "::1" {
                notify_main_window(102);
                send_json_response(&mut tls_stream, 200, "OK", &json!({"status": "ok"}));
            } else {
                send_response(&mut tls_stream, 403, "Forbidden", "text/plain", b"Localhost only");
            }
        }

        _ => {
            send_response(&mut tls_stream, 404, "Not Found", "text/plain", b"Not Found");
        }
    }
}

// ----------------------------------------------------------------------------
// Handlers
// ----------------------------------------------------------------------------

#[derive(Deserialize)]
struct PairV2Payload {
    client_id: String,
    client_name: String,
    proof: String,
}

fn handle_v2_pair<S: Read + Write>(
    stream: &mut S,
    headers: &HashMap<String, String>,
    client_der_bytes: Option<&[u8]>,
    client_spki_pin: &str,
    client_cert_pin: &str,
) {
    let client_der = match client_der_bytes {
        Some(d) => d,
        None => {
            send_response(stream, 400, "Bad Request", "application/json", b"{\"status\":\"error\",\"message\":\"mTLS client certificate required\"}");
            return;
        }
    };

    let content_len: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    if content_len == 0 || content_len > MAX_PAIR_BODY_BYTES {
        send_response(stream, 400, "Bad Request", "text/plain", b"Invalid Content-Length");
        return;
    }

    let mut body = vec![0u8; content_len];
    if stream.read_exact(&mut body).is_err() {
        send_response(stream, 400, "Bad Request", "text/plain", b"Read error");
        return;
    }

    let payload: PairV2Payload = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(_) => {
            send_response(stream, 400, "Bad Request", "application/json", b"{\"status\":\"error\",\"message\":\"Invalid JSON\"}");
            return;
        }
    };

    let pair_secret = match crate::config::get_active_pair_secret() {
        Some(s) => s,
        None => {
            send_response(stream, 403, "Forbidden", "application/json", b"{\"status\":\"error\",\"message\":\"No active pairing session\"}");
            return;
        }
    };

    // Calculate proof1 = HMAC-SHA256(pair_secret, client_der)
    let mut valid = false;
    if let Ok(mut mac) = HmacSha256::new_from_slice(pair_secret.as_bytes()) {
        mac.update(client_der);
        let expected1 = hex::encode(mac.finalize().into_bytes()).to_lowercase();
        if payload.proof.to_lowercase().as_bytes().ct_eq(expected1.as_bytes()).into() {
            valid = true;
        }
    }

    // Fallback: proof2 = HMAC-SHA256(pair_secret, spki_bytes)
    if !valid {
        if let Ok((_, cert)) = x509_parser::parse_x509_certificate(client_der) {
            let spki_bytes = cert.tbs_certificate.subject_pki.raw;
            if let Ok(mut mac) = HmacSha256::new_from_slice(pair_secret.as_bytes()) {
                mac.update(spki_bytes);
                let expected2 = hex::encode(mac.finalize().into_bytes()).to_lowercase();
                if payload.proof.to_lowercase().as_bytes().ct_eq(expected2.as_bytes()).into() {
                    valid = true;
                }
            }
        }
    }

    if !valid {
        send_response(stream, 403, "Forbidden", "application/json", b"{\"status\":\"error\",\"message\":\"Invalid proof of possession\"}");
        return;
    }

    // Success! Register device
    add_or_update_paired_device(
        &payload.client_id,
        &payload.client_name,
        client_spki_pin,
        client_cert_pin,
        &pair_secret,
    );

    crate::config::clear_active_pair_secret();
    play_arrival_chime();
    crate::tray::show_tray_notification("⚡ Flick Paired", &format!("Successfully paired with {}", payload.client_name));

    let cfg = get_config();
    let resp = json!({
        "status": "paired",
        "pc_id": cfg.pc_id,
        "pc_name": cfg.pc_name
    });
    send_json_response(stream, 200, "OK", &resp);
}

#[derive(Deserialize)]
struct LegacyPairPayload {
    client_id: String,
    client_name: String,
    #[serde(default)]
    token: String,
}

static PAIR_MUTEX: Mutex<()> = Mutex::new(());

fn handle_legacy_pair<S: Read + Write>(
    stream: &mut S,
    headers: &HashMap<String, String>,
    peer_ip: &str,
) {
    let content_len: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    if content_len == 0 || content_len > MAX_PAIR_BODY_BYTES {
        send_response(stream, 400, "Bad Request", "text/plain", b"Invalid Content-Length");
        return;
    }

    let mut body = vec![0u8; content_len];
    if stream.read_exact(&mut body).is_err() {
        send_response(stream, 400, "Bad Request", "text/plain", b"Read error");
        return;
    }

    let payload: LegacyPairPayload = match serde_json::from_slice(&body) {
        Ok(p) => p,
        Err(_) => {
            send_response(stream, 400, "Bad Request", "application/json", b"{\"status\":\"error\",\"message\":\"Invalid JSON\"}");
            return;
        }
    };

    let cfg = get_config();

    // Check token pre-shared key
    if !payload.token.is_empty()
        && payload
            .token
            .as_bytes()
            .ct_eq(cfg.auth_token.as_bytes())
            .into()
    {
        add_or_update_paired_device(
            &payload.client_id,
            &payload.client_name,
            "",
            "",
            &payload.token,
        );
        play_arrival_chime();
        crate::tray::show_tray_notification("⚡ Flick Paired", &format!("Successfully paired with {}", payload.client_name));
        let resp = json!({
            "status": "paired",
            "pc_id": cfg.pc_id,
            "pc_name": cfg.pc_name
        });
        send_json_response(stream, 200, "OK", &resp);
        return;
    }

    // Interactive Win32 prompt with single mutex
    let _lock = match PAIR_MUTEX.try_lock() {
        Ok(l) => l,
        Err(_) => {
            send_response(stream, 429, "Too Many Requests", "application/json", b"{\"status\":\"error\",\"message\":\"Pairing prompt active\"}");
            return;
        }
    };

    let prompt = format!(
        "Pairing request from phone:\r\n\r\nDevice: {}\r\nIP: {}\r\n\r\nAllow connection and file transfers?",
        payload.client_name, peer_ip
    );
    let prompt_wide: Vec<u16> = OsStr::new(&prompt).encode_wide().chain(Some(0)).collect();
    let title_wide: Vec<u16> = OsStr::new("⚡ Flick — Pair Request")
        .encode_wide()
        .chain(Some(0))
        .collect();

    let res = unsafe {
        MessageBoxW(
            0 as HWND,
            prompt_wide.as_ptr(),
            title_wide.as_ptr(),
            MB_YESNO | MB_ICONQUESTION | MB_TOPMOST | MB_SETFOREGROUND,
        )
    };

    if res == IDYES {
        add_or_update_paired_device(
            &payload.client_id,
            &payload.client_name,
            "",
            "",
            &cfg.auth_token,
        );
        play_arrival_chime();
        crate::tray::show_tray_notification("⚡ Flick Paired", &format!("Successfully paired with {}", payload.client_name));
        let resp = json!({
            "status": "paired",
            "pc_id": cfg.pc_id,
            "pc_name": cfg.pc_name
        });
        send_json_response(stream, 200, "OK", &resp);
    } else {
        send_response(stream, 403, "Forbidden", "application/json", b"{\"status\":\"rejected\",\"message\":\"User declined pairing\"}");
    }
}

/// Resolves the effective clipboard mode for an upload, falling back to `config_mode`
/// whenever the `X-Clipboard-Mode` header is absent, blank, or `"DEFAULT"`.
fn resolve_upload_clipboard_mode(header_val: Option<&String>, config_mode: &str) -> String {
    match header_val {
        Some(raw) => {
            let trimmed = raw.trim();
            if trimmed.is_empty() || trimmed.eq_ignore_ascii_case("DEFAULT") {
                clipboard::normalize_clipboard_mode(config_mode).to_string()
            } else {
                clipboard::normalize_clipboard_mode(trimmed).to_string()
            }
        }
        None => clipboard::normalize_clipboard_mode(config_mode).to_string(),
    }
}

fn handle_upload<S: Read + Write>(
    stream: &mut S,
    headers: &HashMap<String, String>,
    peer_ip: &str,
) {
    let content_len: u64 = match headers.get("content-length").and_then(|v| v.parse().ok()) {
        Some(len) if len > 0 && len <= MAX_ALLOWED_FILE_BYTES => len,
        _ => {
            send_response(stream, 400, "Bad Request", "text/plain", b"Invalid or missing Content-Length");
            return;
        }
    };

    let cfg = get_config();
    let save_dir = PathBuf::from(&cfg.save_directory);
    let _ = fs::create_dir_all(&save_dir);

    if !check_disk_space(&save_dir, content_len) {
        send_response(stream, 507, "Insufficient Storage", "application/json", b"{\"status\":\"error\",\"message\":\"Insufficient disk space\"}");
        return;
    }

    let raw_file_name = headers
        .get("x-file-name")
        .cloned()
        .unwrap_or_else(|| format!("flick_{}", SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_millis()));

    let (safe_name, _quarantined) = sanitize_filename(&raw_file_name, 100);

    // Multi-file batch headers
    let batch_id = headers.get("x-batch-id").cloned();
    let batch_index: usize = headers.get("x-batch-index").and_then(|v| v.parse().ok()).unwrap_or(0);
    let batch_total: usize = headers.get("x-batch-total").and_then(|v| v.parse().ok()).unwrap_or(1);

    // Determine target file path with conflict avoidance
    let mut final_path = save_dir.join(&safe_name);
    if final_path.exists() {
        let stem = Path::new(&safe_name)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("file");
        let ext = Path::new(&safe_name)
            .extension()
            .and_then(|s| s.to_str())
            .map(|s| format!(".{}", s))
            .unwrap_or_default();
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        final_path = save_dir.join(format!("{}_{}{}", stem, ts, ext));
    }

    let temp_part_path = save_dir.join(format!("flick_tmp_{}.part", Uuid::new_v4()));

    // Stream 64 KB chunks to disk
    let start_time = Instant::now();
    let mut part_file = match File::create(&temp_part_path) {
        Ok(f) => f,
        Err(e) => {
            send_response(stream, 500, "Internal Server Error", "text/plain", e.to_string().as_bytes());
            return;
        }
    };

    let mut remaining = content_len;
    let mut buf = [0u8; CHUNK_SIZE];

    while remaining > 0 {
        let to_read = (remaining as usize).min(CHUNK_SIZE);
        match stream.read(&mut buf[..to_read]) {
            Ok(0) => {
                let _ = fs::remove_file(&temp_part_path);
                send_response(stream, 400, "Bad Request", "text/plain", b"Unexpected EOF");
                return;
            }
            Ok(n) => {
                if part_file.write_all(&buf[..n]).is_err() {
                    let _ = fs::remove_file(&temp_part_path);
                    send_response(stream, 500, "Internal Server Error", "text/plain", b"Disk write error");
                    return;
                }
                remaining -= n as u64;
            }
            Err(e) => {
                let _ = fs::remove_file(&temp_part_path);
                send_response(stream, 500, "Internal Server Error", "text/plain", e.to_string().as_bytes());
                return;
            }
        }
    }

    let _ = part_file.flush();
    drop(part_file);

    // Atomic rename to final path
    if fs::rename(&temp_part_path, &final_path).is_err() {
        let _ = fs::remove_file(&temp_part_path);
        send_response(stream, 500, "Internal Server Error", "text/plain", b"Atomic rename failed");
        return;
    }

    // Attach NTFS Zone.Identifier
    set_zone_identifier(&final_path);

    let elapsed_ms = start_time.elapsed().as_millis();

    // Mode override (resolve "DEFAULT" / empty header to configured receiver mode)
    let mode = resolve_upload_clipboard_mode(headers.get("x-clipboard-mode"), &cfg.clipboard_mode);

    let mut applied_formats: Vec<&str> = Vec::new();

    if batch_total > 1 {
        let batch_key = batch_id.unwrap_or_else(|| format!("ip:{}", peer_ip));
        let completed_batch: Option<(Vec<PathBuf>, String)> = {
            let _guard = BATCH_LOCK.lock().unwrap();
            let mut batches = get_batches().lock().unwrap();

            // Evict abandoned batches older than 2 minutes
            batches.retain(|_, v| v.last_update.elapsed() <= Duration::from_secs(120));

            let entry = batches.entry(batch_key.clone()).or_insert_with(|| BatchEntry {
                total: batch_total,
                paths: Vec::with_capacity(batch_total),
                last_update: Instant::now(),
                mode: mode.clone(),
            });

            entry.paths.push(final_path.clone());
            entry.last_update = Instant::now();

            if entry.paths.len() >= entry.total || batch_index + 1 >= batch_total {
                batches.remove(&batch_key).map(|e| (e.paths, e.mode))
            } else {
                None
            }
        };

        if let Some((batch_paths, batch_mode)) = completed_batch {
            let total_files = batch_paths.len();
            let has_quarantined = batch_paths
                .iter()
                .any(|p| p.to_string_lossy().ends_with(".blocked"));
            let effective_mode = if has_quarantined {
                "FORCE_PATH"
            } else {
                batch_mode.as_str()
            };
            applied_formats = clipboard::planned_clipboard_formats(&batch_paths, effective_mode);
            log_http(&format!("Batch complete: {} files -> show_landing_files", total_files));
            crate::landing::show_landing_files(batch_paths.clone());
            clipboard::enqueue_files_delivery(
                batch_paths,
                effective_mode.to_string(),
                cfg.winv_history_stack && !has_quarantined,
            );
            play_arrival_chime();
            if has_quarantined {
                crate::tray::show_tray_notification(
                    "File Quarantined",
                    "Batch contained dangerous file(s) saved as .blocked",
                );
            } else {
                crate::tray::show_tray_notification(
                    "Flick Received",
                    &format!("{} files ready to paste", total_files),
                );
            }
        }
    } else {
        // Single file transfer
        let is_quarantined = safe_name.ends_with(".blocked");
        let effective_mode = if is_quarantined { "FORCE_PATH" } else { mode.as_str() };
        let single_paths = vec![final_path.clone()];
        applied_formats = clipboard::planned_clipboard_formats(&single_paths, effective_mode);
        log_http(&format!("Single file complete: {:?} -> show_landing_files", single_paths));
        crate::landing::show_landing_files(single_paths.clone());
        clipboard::enqueue_files_delivery(
            single_paths,
            effective_mode.to_string(),
            cfg.winv_history_stack && !is_quarantined,
        );
        play_arrival_chime();
        if is_quarantined {
            crate::tray::show_tray_notification("File Quarantined", "Saved with .blocked extension for security");
        } else {
            crate::tray::show_tray_notification("Flick Received", &format!("'{}' ready to paste", safe_name));
        }
    }

    record_transfer(&safe_name, content_len, peer_ip, "FILE");

    let resp = json!({
        "status": "ok",
        "saved_path": final_path.to_string_lossy(),
        "bytes_written": content_len,
        "clipboard_formats": applied_formats,
        "elapsed_ms": elapsed_ms
    });

    send_json_response(stream, 200, "OK", &resp);
}

fn handle_clipboard<S: Read + Write>(
    stream: &mut S,
    headers: &HashMap<String, String>,
    peer_ip: &str,
) {
    let content_len: usize = headers
        .get("content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);

    if content_len == 0 || content_len > MAX_CLIPBOARD_BODY_BYTES {
        send_response(stream, 400, "Bad Request", "text/plain", b"Invalid Content-Length");
        return;
    }

    let mut body = vec![0u8; content_len];
    if stream.read_exact(&mut body).is_err() {
        send_response(stream, 400, "Bad Request", "text/plain", b"Read error");
        return;
    }

    let text = match std::str::from_utf8(&body) {
        Ok(s) => s,
        Err(_) => {
            send_response(stream, 400, "Bad Request", "text/plain", b"Invalid UTF-8");
            return;
        }
    };

    clipboard::enqueue_text_delivery(text.to_string());
    crate::landing::show_landing_text(text);
    play_arrival_chime();
    crate::tray::show_tray_notification("Flick Clipboard", &format!("Text copied to clipboard ({} chars)", text.chars().count()));

    let display_preview = if text.chars().count() > 30 {
        let prefix: String = text.chars().take(27).collect();
        format!("{}...", prefix)
    } else {
        text.to_string()
    };
    record_transfer(&display_preview, content_len as u64, peer_ip, "TEXT");

    let resp = json!({
        "status": "ok",
        "characters_copied": text.chars().count(),
        "mode": "CF_UNICODETEXT"
    });

    send_json_response(stream, 200, "OK", &resp);
}

// ----------------------------------------------------------------------------
// Win32 Single-Instance Window Activation
// ----------------------------------------------------------------------------

fn notify_main_window(code: usize) {
    unsafe {
        let class_name: Vec<u16> = OsStr::new("FlickTrayWindowClass")
            .encode_wide()
            .chain(Some(0))
            .collect();
        let hwnd = FindWindowW(class_name.as_ptr(), std::ptr::null());
        if !hwnd.is_null() {
            PostMessageW(hwnd, 0x0400 + code as u32, 0, 0); // WM_USER + code
        }
    }
}

// ----------------------------------------------------------------------------
// HTTP Helper Functions
// ----------------------------------------------------------------------------

fn send_response<S: Write>(
    stream: &mut S,
    status_code: u16,
    reason: &str,
    content_type: &str,
    body: &[u8],
) {
    let resp_hdr = format!(
        "HTTP/1.1 {} {}\r\nContent-Type: {}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
        status_code,
        reason,
        content_type,
        body.len()
    );
    let _ = stream.write_all(resp_hdr.as_bytes());
    let _ = stream.write_all(body);
    let _ = stream.flush();
}

fn send_json_response<S: Write>(
    stream: &mut S,
    status_code: u16,
    reason: &str,
    data: &serde_json::Value,
) {
    if let Ok(bytes) = serde_json::to_vec(data) {
        send_response(stream, status_code, reason, "application/json", &bytes);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_upload_mode_uses_config_when_header_missing_or_default() {
        assert_eq!(resolve_upload_clipboard_mode(None, "FORCE_IMAGE"), "FORCE_IMAGE");
        assert_eq!(
            resolve_upload_clipboard_mode(Some(&"DEFAULT".to_string()), "FORCE_IMAGE"),
            "FORCE_IMAGE"
        );
        assert_eq!(
            resolve_upload_clipboard_mode(Some(&"  ".to_string()), "FORCE_PATH"),
            "FORCE_PATH"
        );
        assert_eq!(
            resolve_upload_clipboard_mode(Some(&"FORCE_FILE".to_string()), "SMART_MULTI"),
            "FORCE_FILE"
        );
        assert_eq!(
            resolve_upload_clipboard_mode(Some(&"DEFAULT".to_string()), ""),
            "SMART_MULTI"
        );
    }
}
