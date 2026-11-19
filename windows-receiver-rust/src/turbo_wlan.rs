//! Flick Turbo Link — Windows Native Wi-Fi (`wlanapi.dll`) Controller (`turbo_wlan.rs`)
//!
//! Manages autonomous, zero-touch switching to an Android `LocalOnlyHotspot` 5GHz AP
//! for large payloads (> 400 MB), L3 IPv4 lease detection via `iphlpapi.dll`,
//! `FLICK_TURBO_READY` UDP signaling, and deterministic restoration of the primary Wi-Fi profile.

use std::ffi::{CStr, OsStr};
use std::net::{Ipv4Addr, UdpSocket};
use std::os::windows::ffi::OsStrExt;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::json;
use windows_sys::core::GUID;
use windows_sys::Win32::Foundation::{ERROR_BUFFER_OVERFLOW, ERROR_SUCCESS, HANDLE};
use windows_sys::Win32::NetworkManagement::IpHelper::{
    GetAdaptersAddresses, GAA_FLAG_INCLUDE_GATEWAYS, IP_ADAPTER_ADDRESSES_LH,
};
use windows_sys::Win32::NetworkManagement::WiFi::{
    dot11_BSS_type_infrastructure, wlan_connection_mode_profile, wlan_interface_state_connected,
    wlan_intf_opcode_current_connection, WlanCloseHandle, WlanConnect, WlanDeleteProfile,
    WlanDisconnect, WlanEnumInterfaces, WlanFreeMemory, WlanOpenHandle, WlanQueryInterface,
    WlanSetProfile, WLAN_CONNECTION_ATTRIBUTES, WLAN_CONNECTION_PARAMETERS,
    WLAN_INTERFACE_INFO_LIST,
};
use windows_sys::Win32::Networking::WinSock::{AF_INET, SOCKADDR_IN};

const TURBO_UDP_PORT: u16 = 45456;
const TURBO_UPLOAD_WAIT_TIMEOUT: Duration = Duration::from_secs(5);
const TURBO_POST_UPLOAD_GRACE: Duration = Duration::from_secs(2);

static TURBO_ACTIVE: AtomicBool = AtomicBool::new(false);
static ACTIVE_TURBO_UPLOADS: AtomicUsize = AtomicUsize::new(0);
static SESSION_GEN: AtomicUsize = AtomicUsize::new(0);

#[derive(Clone)]
struct TurboSession {
    gen_id: usize,
    session_id: String,
    interface_guid: GUID,
    temp_profile_name: String,
    original_profile_name: Option<String>,
    turbo_ip: Option<String>,
    turbo_prefix: Option<[u8; 3]>,
}

static TURBO_SESSION: OnceLock<Mutex<Option<TurboSession>>> = OnceLock::new();

fn session_lock() -> &'static Mutex<Option<TurboSession>> {
    TURBO_SESSION.get_or_init(|| Mutex::new(None))
}

pub struct TurboUploadGuard {
    is_turbo: bool,
}

impl TurboUploadGuard {
    pub fn is_turbo(&self) -> bool {
        self.is_turbo
    }
}

impl Drop for TurboUploadGuard {
    fn drop(&mut self) {
        if self.is_turbo {
            ACTIVE_TURBO_UPLOADS.fetch_sub(1, Ordering::SeqCst);
        }
    }
}

pub fn is_turbo_active() -> bool {
    TURBO_ACTIVE.load(Ordering::SeqCst)
}

/// Checks if an incoming connection `peer_ip` is arriving over the active Turbo subnet
/// and acquires a guard preventing the watchdog from tearing down Wi-Fi mid-transfer.
pub fn begin_upload_guard(peer_ip: &str) -> TurboUploadGuard {
    if !TURBO_ACTIVE.load(Ordering::SeqCst) {
        return TurboUploadGuard { is_turbo: false };
    }

    let is_turbo_peer = {
        let guard = session_lock().lock().unwrap();
        if let Some(ref s) = *guard {
            if let (Some(ref prefix), Ok(ip)) = (s.turbo_prefix, peer_ip.parse::<Ipv4Addr>()) {
                let octets = ip.octets();
                octets[0] == prefix[0] && octets[1] == prefix[1] && octets[2] == prefix[2]
            } else {
                false
            }
        } else {
            false
        }
    };

    if is_turbo_peer {
        ACTIVE_TURBO_UPLOADS.fetch_add(1, Ordering::SeqCst);
    }

    TurboUploadGuard {
        is_turbo: is_turbo_peer,
    }
}

fn to_wide(s: &str) -> Vec<u16> {
    OsStr::new(s).encode_wide().chain(Some(0)).collect()
}

fn from_wide_buf(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

fn escape_xml(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            _ => out.push(ch),
        }
    }
    out
}

fn guid_to_braced_string(g: &GUID) -> String {
    format!(
        "{{{:08X}-{:04X}-{:04X}-{:02X}{:02X}-{:02X}{:02X}{:02X}{:02X}{:02X}{:02X}}}",
        g.data1,
        g.data2,
        g.data3,
        g.data4[0],
        g.data4[1],
        g.data4[2],
        g.data4[3],
        g.data4[4],
        g.data4[5],
        g.data4[6],
        g.data4[7]
    )
}

fn open_wlan_handle() -> Result<HANDLE, String> {
    let mut neg_ver: u32 = 0;
    let mut handle: HANDLE = std::ptr::null_mut();
    let status = unsafe { WlanOpenHandle(2, std::ptr::null(), &mut neg_ver, &mut handle) };
    if status != ERROR_SUCCESS || handle.is_null() {
        return Err(format!("WlanOpenHandle failed (err={})", status));
    }
    Ok(handle)
}

struct WlanHandleGuard(HANDLE);
impl Drop for WlanHandleGuard {
    fn drop(&mut self) {
        if !self.0.is_null() {
            unsafe {
                WlanCloseHandle(self.0, std::ptr::null());
            }
        }
    }
}

/// Finds the primary WLAN interface GUID and its currently connected profile name (if connected).
fn query_primary_wlan_interface(handle: HANDLE) -> Result<(GUID, Option<String>), String> {
    let mut list_ptr: *mut WLAN_INTERFACE_INFO_LIST = std::ptr::null_mut();
    let status = unsafe { WlanEnumInterfaces(handle, std::ptr::null(), &mut list_ptr) };
    if status != ERROR_SUCCESS || list_ptr.is_null() {
        return Err(format!("WlanEnumInterfaces failed (err={})", status));
    }

    let count = unsafe { (*list_ptr).dwNumberOfItems };
    if count == 0 {
        unsafe { WlanFreeMemory(list_ptr as *const _) };
        return Err("No Wi-Fi adapter detected on this PC".to_string());
    }

    let interfaces = unsafe {
        std::slice::from_raw_parts((*list_ptr).InterfaceInfo.as_ptr(), count as usize)
    };

    // Prefer an interface that is currently connected, else take the first interface
    let selected = interfaces
        .iter()
        .find(|i| i.isState == wlan_interface_state_connected)
        .unwrap_or(&interfaces[0]);

    let guid = selected.InterfaceGuid;
    let is_connected = selected.isState == wlan_interface_state_connected;
    unsafe { WlanFreeMemory(list_ptr as *const _) };

    let mut current_profile = None;
    if is_connected {
        let mut data_size: u32 = 0;
        let mut data_ptr: *mut core::ffi::c_void = std::ptr::null_mut();
        let q_status = unsafe {
            WlanQueryInterface(
                handle,
                &guid,
                wlan_intf_opcode_current_connection,
                std::ptr::null(),
                &mut data_size,
                &mut data_ptr,
                std::ptr::null_mut(),
            )
        };
        if q_status == ERROR_SUCCESS && !data_ptr.is_null() {
            let attrs = unsafe { &*(data_ptr as *const WLAN_CONNECTION_ATTRIBUTES) };
            let name = from_wide_buf(&attrs.strProfileName);
            if !name.is_empty() {
                current_profile = Some(name);
            }
            unsafe { WlanFreeMemory(data_ptr) };
        }
    }

    Ok((guid, current_profile))
}

fn is_wlan_connected_to_profile(handle: HANDLE, guid: &GUID, target_profile: &str) -> bool {
    let mut data_size: u32 = 0;
    let mut data_ptr: *mut core::ffi::c_void = std::ptr::null_mut();
    let status = unsafe {
        WlanQueryInterface(
            handle,
            guid,
            wlan_intf_opcode_current_connection,
            std::ptr::null(),
            &mut data_size,
            &mut data_ptr,
            std::ptr::null_mut(),
        )
    };
    if status != ERROR_SUCCESS || data_ptr.is_null() {
        return false;
    }
    let attrs = unsafe { &*(data_ptr as *const WLAN_CONNECTION_ATTRIBUTES) };
    let connected = attrs.isState == wlan_interface_state_connected;
    let name = from_wide_buf(&attrs.strProfileName);
    unsafe { WlanFreeMemory(data_ptr) };
    connected && name == target_profile
}

#[derive(Debug, Clone)]
struct AdapterIpv4Info {
    ip: Ipv4Addr,
    prefix_len: u8,
    gateway: Option<Ipv4Addr>,
}

/// Queries the exact IPv4 address, prefix length, and default gateway assigned to `interface_guid`.
fn get_adapter_ipv4(interface_guid: &GUID) -> Option<AdapterIpv4Info> {
    let target_name = guid_to_braced_string(interface_guid);
    let flags = GAA_FLAG_INCLUDE_GATEWAYS;
    let mut buf_len: u32 = 16384;
    let mut buf: Vec<u8> = vec![0u8; buf_len as usize];

    let mut ret = unsafe {
        GetAdaptersAddresses(
            AF_INET as u32,
            flags,
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
                flags,
                std::ptr::null(),
                buf.as_mut_ptr() as *mut IP_ADAPTER_ADDRESSES_LH,
                &mut buf_len,
            )
        };
    }
    if ret != ERROR_SUCCESS {
        return None;
    }

    let mut cur = buf.as_ptr() as *const IP_ADAPTER_ADDRESSES_LH;
    while !cur.is_null() {
        let adapter = unsafe { &*cur };
        if !adapter.AdapterName.is_null() {
            let name = unsafe { CStr::from_ptr(adapter.AdapterName as *const i8) }
                .to_string_lossy();
            if name.eq_ignore_ascii_case(&target_name) {
                // Walk unicast IPv4 addresses
                let mut chosen_ip: Option<(Ipv4Addr, u8)> = None;
                let mut u_cur = adapter.FirstUnicastAddress;
                while !u_cur.is_null() {
                    let u = unsafe { &*u_cur };
                    if !u.Address.lpSockaddr.is_null() {
                        let sa = unsafe { &*(u.Address.lpSockaddr as *const SOCKADDR_IN) };
                        if sa.sin_family as u32 == AF_INET as u32 {
                            let raw = unsafe { sa.sin_addr.S_un.S_addr };
                            let ip = Ipv4Addr::from(u32::from_be(raw));
                            let oct = ip.octets();
                            // Skip loopback and APIPA (169.254.x.x)
                            if oct[0] != 127 && !(oct[0] == 169 && oct[1] == 254) {
                                chosen_ip = Some((ip, u.OnLinkPrefixLength));
                                break;
                            }
                        }
                    }
                    u_cur = u.Next;
                }

                // Walk gateway IPv4 addresses
                let mut gateway: Option<Ipv4Addr> = None;
                let mut g_cur = adapter.FirstGatewayAddress;
                while !g_cur.is_null() {
                    let g = unsafe { &*g_cur };
                    if !g.Address.lpSockaddr.is_null() {
                        let sa = unsafe { &*(g.Address.lpSockaddr as *const SOCKADDR_IN) };
                        if sa.sin_family as u32 == AF_INET as u32 {
                            let raw = unsafe { sa.sin_addr.S_un.S_addr };
                            let gw = Ipv4Addr::from(u32::from_be(raw));
                            if !gw.is_unspecified() {
                                gateway = Some(gw);
                                break;
                            }
                        }
                    }
                    g_cur = g.Next;
                }

                if let Some((ip, prefix_len)) = chosen_ip {
                    return Some(AdapterIpv4Info {
                        ip,
                        prefix_len,
                        gateway,
                    });
                }
            }
        }
        cur = adapter.Next;
    }
    None
}

fn compute_subnet_broadcast(ip: Ipv4Addr, prefix_len: u8) -> Ipv4Addr {
    let shift = 32u32.saturating_sub(prefix_len.min(32) as u32);
    let mask = if shift == 32 { 0 } else { !0u32 << shift };
    let ip_u32 = u32::from(ip);
    Ipv4Addr::from(ip_u32 | !mask)
}

fn build_wpa2_profile_xml(ssid: &str, key: &str) -> String {
    let esc_ssid = escape_xml(ssid);
    let esc_key = escape_xml(key);
    format!(
        r#"<?xml version="1.0"?>
<WLANProfile xmlns="http://www.microsoft.com/networking/WLAN/profile/v1">
    <name>{ssid}</name>
    <SSIDConfig>
        <SSID>
            <name>{ssid}</name>
        </SSID>
    </SSIDConfig>
    <connectionType>ESS</connectionType>
    <connectionMode>manual</connectionMode>
    <MSM>
        <security>
            <authEncryption>
                <authentication>WPA2PSK</authentication>
                <encryption>AES</encryption>
                <useOneX>false</useOneX>
            </authEncryption>
            <sharedKey>
                <keyType>passPhrase</keyType>
                <protected>false</protected>
                <keyMaterial>{key}</keyMaterial>
            </sharedKey>
        </security>
    </MSM>
</WLANProfile>"#,
        ssid = esc_ssid,
        key = esc_key
    )
}

fn connect_profile(handle: HANDLE, guid: &GUID, profile_name: &str) -> Result<(), String> {
    let profile_w = to_wide(profile_name);
    let params = WLAN_CONNECTION_PARAMETERS {
        wlanConnectionMode: wlan_connection_mode_profile,
        strProfile: profile_w.as_ptr(),
        pDot11Ssid: std::ptr::null_mut(),
        pDesiredBssidList: std::ptr::null_mut(),
        dot11BssType: dot11_BSS_type_infrastructure,
        dwFlags: 0,
    };
    let status = unsafe { WlanConnect(handle, guid, &params, std::ptr::null()) };
    if status != ERROR_SUCCESS {
        return Err(format!("WlanConnect({}) failed (err={})", profile_name, status));
    }
    Ok(())
}

/// Pre-flight check + background switch orchestration for `POST /turbo/initiate`.
/// Returns `Ok(())` immediately so the HTTP 200 response can be flushed over the LAN
/// socket before the single Wi-Fi radio disassociates from the home router.
pub fn initiate_turbo(
    ssid: String,
    key: String,
    session_id: String,
    pc_id: String,
    http_port: u16,
) -> Result<(), String> {
    if ssid.is_empty() || ssid.len() > 32 {
        return Err("Invalid hotspot SSID length".to_string());
    }
    if key.len() < 8 || key.len() > 63 {
        return Err("Invalid WPA2 passphrase length".to_string());
    }

    let handle = open_wlan_handle()?;
    let _guard = WlanHandleGuard(handle);
    let (guid, original_profile) = query_primary_wlan_interface(handle)?;

    // If a previous Turbo profile was still active, clean its state cleanly
    let old_temp_to_delete: Option<(GUID, String)> = {
        let mut lock = session_lock().lock().unwrap();
        let prev = lock.take();
        prev.map(|s| (s.interface_guid, s.temp_profile_name))
    };
    if let Some((old_guid, old_profile)) = old_temp_to_delete {
        if old_profile != ssid {
            let old_w = to_wide(&old_profile);
            unsafe {
                WlanDeleteProfile(handle, &old_guid, old_w.as_ptr(), std::ptr::null());
            }
        }
    }

    let old_ipv4 = get_adapter_ipv4(&guid).map(|a| a.ip);
    let gen_id = SESSION_GEN.fetch_add(1, Ordering::SeqCst) + 1;

    // Preserve original_profile if we're somehow re-initiating while still on a non-temp profile
    let orig_to_save = match original_profile {
        Some(ref p) if p == &ssid => None,
        other => other,
    };

    {
        let mut lock = session_lock().lock().unwrap();
        *lock = Some(TurboSession {
            gen_id,
            session_id: session_id.clone(),
            interface_guid: guid,
            temp_profile_name: ssid.clone(),
            original_profile_name: orig_to_save.clone(),
            turbo_ip: None,
            turbo_prefix: None,
        });
    }

    crate::http_server::log_http(&format!(
        "[Turbo] Initiating P2P switch gen={} ssid='{}' saved_profile={:?} old_ip={:?}",
        gen_id, ssid, orig_to_save, old_ipv4
    ));

    std::thread::Builder::new()
        .name("flick-turbo-worker".to_string())
        .stack_size(512 * 1024)
        .spawn(move || {
            run_turbo_worker(
                gen_id,
                guid,
                ssid,
                key,
                session_id,
                pc_id,
                http_port,
                old_ipv4,
            );
        })
        .map_err(|e| format!("Failed to spawn turbo worker: {}", e))?;

    Ok(())
}

fn is_session_current(gen_id: usize) -> bool {
    let lock = session_lock().lock().unwrap();
    lock.as_ref().map(|s| s.gen_id == gen_id).unwrap_or(false)
}

#[allow(clippy::too_many_arguments)]
fn run_turbo_worker(
    gen_id: usize,
    guid: GUID,
    ssid: String,
    key: String,
    session_id: String,
    pc_id: String,
    http_port: u16,
    old_ipv4: Option<Ipv4Addr>,
) {
    // 1. Wait 3650ms while still on the home router so Android hostapd completes ACS + 5GHz CSA
    //    (5745 MHz) before Windows disconnects from Reemo and scans for the Turbo SSID.
    let pre_wait_end = Instant::now() + Duration::from_millis(3650);
    while Instant::now() < pre_wait_end {
        if !is_session_current(gen_id) {
            return;
        }
        std::thread::sleep(Duration::from_millis(50));
    }

    let handle = match open_wlan_handle() {
        Ok(h) => WlanHandleGuard(h),
        Err(e) => {
            crate::http_server::log_http(&format!("[Turbo] {}", e));
            restore_primary_wifi_for_gen(gen_id, "wlan_handle_failed");
            return;
        }
    };

    // 2. Inject temporary WPA2-PSK profile and connect (try all-user flag=0, fallback to per-user flag=2)
    let xml = build_wpa2_profile_xml(&ssid, &key);
    let xml_w = to_wide(&xml);
    let mut reason_code: u32 = 0;
    let mut set_status = unsafe {
        WlanSetProfile(
            handle.0,
            &guid,
            0,
            xml_w.as_ptr(),
            std::ptr::null(),
            1, // overwrite
            std::ptr::null(),
            &mut reason_code,
        )
    };
    if set_status == 5 {
        // ERROR_ACCESS_DENIED -> fallback to WLAN_PROFILE_USER (2)
        set_status = unsafe {
            WlanSetProfile(
                handle.0,
                &guid,
                2,
                xml_w.as_ptr(),
                std::ptr::null(),
                1,
                std::ptr::null(),
                &mut reason_code,
            )
        };
    }
    if set_status != ERROR_SUCCESS {
        crate::http_server::log_http(&format!(
            "[Turbo] WlanSetProfile failed err={} reason={}",
            set_status, reason_code
        ));
        restore_primary_wifi_for_gen(gen_id, "set_profile_failed");
        return;
    }

    if let Err(e) = connect_profile(handle.0, &guid, &ssid) {
        crate::http_server::log_http(&format!("[Turbo] {}", e));
        restore_primary_wifi_for_gen(gen_id, "wlan_connect_failed");
        return;
    }

    // 3. Poll for L2 association + L3 DHCP IPv4 lease within 6.5s budget (without interrupting WlanConnect)
    let t_start = Instant::now();
    let deadline = t_start + Duration::from_millis(6500);
    let mut l3_info: Option<AdapterIpv4Info> = None;
    let mut l2_logged = false;

    while Instant::now() < deadline {
        if !is_session_current(gen_id) {
            return;
        }
        if is_wlan_connected_to_profile(handle.0, &guid, &ssid) {
            if !l2_logged {
                l2_logged = true;
                crate::http_server::log_http(&format!(
                    "[Turbo] L2 associated to '{}' in {}ms",
                    ssid,
                    t_start.elapsed().as_millis()
                ));
            }
            if let Some(info) = get_adapter_ipv4(&guid) {
                let is_fresh_ip = match old_ipv4 {
                    Some(prev) => info.ip != prev,
                    None => true,
                };
                if is_fresh_ip {
                    l3_info = Some(info);
                    break;
                }
            }
        }
        std::thread::sleep(Duration::from_millis(25));
    }

    let info = match l3_info {
        Some(i) => i,
        None => {
            crate::http_server::log_http("[Turbo] Timed out waiting for L2/L3 hotspot link");
            restore_primary_wifi_for_gen(gen_id, "l3_timeout");
            return;
        }
    };

    let oct = info.ip.octets();
    {
        let mut lock = session_lock().lock().unwrap();
        if let Some(ref mut s) = *lock {
            if s.gen_id != gen_id {
                return;
            }
            s.turbo_ip = Some(info.ip.to_string());
            s.turbo_prefix = Some([oct[0], oct[1], oct[2]]);
        } else {
            return;
        }
    }
    TURBO_ACTIVE.store(true, Ordering::SeqCst);

    crate::http_server::log_http(&format!(
        "[Turbo] Connected! turbo_ip={} gw={:?} prefix=/{}",
        info.ip, info.gateway, info.prefix_len
    ));

    // 4. Blast FLICK_TURBO_READY UDP signal directly from the Turbo IP
    let wait_start = Instant::now();
    let bcast_ip = compute_subnet_broadcast(info.ip, info.prefix_len);
    let ready_payload = json!({
        "protocol": "FLICK_TURBO_READY",
        "pc_id": pc_id,
        "session_id": session_id,
        "turbo_ip": info.ip.to_string(),
        "port": http_port,
    });
    if let (Ok(sock), Ok(bytes)) = (
        UdpSocket::bind((info.ip, 0)),
        serde_json::to_vec(&ready_payload),
    ) {
        let _ = sock.set_broadcast(true);
        let burst_end = Instant::now() + Duration::from_millis(1200);
        while Instant::now() < burst_end && is_session_current(gen_id) {
            if ACTIVE_TURBO_UPLOADS.load(Ordering::SeqCst) > 0 {
                break;
            }
            let _ = sock.send_to(&bytes, (bcast_ip, TURBO_UDP_PORT));
            if let Some(gw) = info.gateway {
                let _ = sock.send_to(&bytes, (gw, TURBO_UDP_PORT));
            }
            std::thread::sleep(Duration::from_millis(35));
        }
    }

    // 5. Self-healing watchdog: guarantees PC never stays stranded on an offline hotspot
    let mut saw_upload = ACTIVE_TURBO_UPLOADS.load(Ordering::SeqCst) > 0;
    let mut last_upload_end: Option<Instant> = None;

    loop {
        std::thread::sleep(Duration::from_millis(150));
        if !is_session_current(gen_id) {
            return;
        }

        // Also check if phone closed the hotspot (WLAN disconnected)
        if !is_wlan_connected_to_profile(handle.0, &guid, &ssid) {
            crate::http_server::log_http("[Turbo] Watchdog detected AP closure -> restoring home Wi-Fi");
            restore_primary_wifi_for_gen(gen_id, "ap_closed");
            return;
        }

        let active_cnt = ACTIVE_TURBO_UPLOADS.load(Ordering::SeqCst);
        if active_cnt > 0 {
            saw_upload = true;
            last_upload_end = None;
            continue;
        }

        if !saw_upload {
            if wait_start.elapsed() > TURBO_UPLOAD_WAIT_TIMEOUT {
                crate::http_server::log_http("[Turbo] Watchdog: no upload started within 5s -> restoring");
                restore_primary_wifi_for_gen(gen_id, "no_upload_timeout");
                return;
            }
        } else {
            let ended_at = *last_upload_end.get_or_insert_with(Instant::now);
            if ended_at.elapsed() > TURBO_POST_UPLOAD_GRACE {
                crate::http_server::log_http("[Turbo] Watchdog: post-upload idle grace elapsed -> restoring");
                restore_primary_wifi_for_gen(gen_id, "post_upload_grace");
                return;
            }
        }
    }
}

pub fn restore_primary_wifi(reason: &str) {
    let gen_id = {
        let lock = session_lock().lock().unwrap();
        match *lock {
            Some(ref s) => s.gen_id,
            None => return,
        }
    };
    restore_primary_wifi_for_gen(gen_id, reason);
}

pub fn restore_primary_wifi_for_session(session_id: Option<&str>, reason: &str) {
    let gen_id = {
        let lock = session_lock().lock().unwrap();
        match *lock {
            Some(ref s) => {
                if let Some(expected) = session_id {
                    if !expected.is_empty() && s.session_id != expected {
                        return;
                    }
                }
                s.gen_id
            }
            None => return,
        }
    };
    restore_primary_wifi_for_gen(gen_id, reason);
}

fn restore_primary_wifi_for_gen(gen_id: usize, reason: &str) {
    let session = {
        let mut lock = session_lock().lock().unwrap();
        match *lock {
            Some(ref s) if s.gen_id == gen_id => lock.take(),
            _ => None,
        }
    };

    let s = match session {
        Some(s) => s,
        None => return,
    };

    TURBO_ACTIVE.store(false, Ordering::SeqCst);
    crate::http_server::log_http(&format!(
        "[Turbo] Restoring primary Wi-Fi (reason={}, orig={:?}, del={})",
        reason, s.original_profile_name, s.temp_profile_name
    ));

    if let Ok(handle) = open_wlan_handle() {
        let _guard = WlanHandleGuard(handle);
        if let Some(ref orig) = s.original_profile_name {
            let _ = connect_profile(handle, &s.interface_guid, orig);
        } else {
            unsafe {
                WlanDisconnect(handle, &s.interface_guid, std::ptr::null());
            }
        }
        let temp_w = to_wide(&s.temp_profile_name);
        unsafe {
            WlanDeleteProfile(handle, &s.interface_guid, temp_w.as_ptr(), std::ptr::null());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_escape_sanitizes_wlan_profile_special_chars() {
        assert_eq!(escape_xml("A&B<C>\"'"), "A&amp;B&lt;C&gt;&quot;&apos;");
    }

    #[test]
    fn subnet_broadcast_computes_slash24_correctly() {
        let ip = Ipv4Addr::new(192, 168, 173, 42);
        assert_eq!(compute_subnet_broadcast(ip, 24), Ipv4Addr::new(192, 168, 173, 255));
    }
}
