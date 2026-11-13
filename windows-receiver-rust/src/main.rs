#![windows_subsystem = "windows"]

mod beacon;
mod clipboard;
mod config;
mod crypto;
mod gui;
mod http_server;
mod tray;

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::sync::Arc;

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, SetLastError, ERROR_ALREADY_EXISTS};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW};

fn main() {
    // 0. Global Panic Hook: Log panic to ~/.flick/panic.log and show Win32 MessageBox
    std::panic::set_hook(Box::new(|info| {
        let msg = format!("Flick Receiver encountered an unexpected error:\n{}\n", info);
        eprintln!("{}", msg);
        if let Some(user_dirs) = std::env::var_os("USERPROFILE") {
            let log_path = std::path::Path::new(&user_dirs).join(".flick").join("panic.log");
            let _ = std::fs::write(&log_path, &msg);
        }
        unsafe {
            use windows_sys::Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK};
            let text: Vec<u16> = OsStr::new(&msg).encode_wide().chain(Some(0)).collect();
            let caption: Vec<u16> = OsStr::new("Flick Windows Receiver").encode_wide().chain(Some(0)).collect();
            MessageBoxW(0 as _, text.as_ptr(), caption.as_ptr(), MB_ICONERROR | MB_OK);
        }
    }));

    // Clear any residual error
    unsafe { SetLastError(0); }

    // 1. Single-Instance Guard: Named Win32 Mutex
    let mutex_name: Vec<u16> = OsStr::new("Local\\Flick_Windows_Receiver_SingleInstance_Mutex")
        .encode_wide()
        .chain(Some(0))
        .collect();

    let h_mutex = unsafe { CreateMutexW(std::ptr::null(), 0, mutex_name.as_ptr()) };
    let last_err = unsafe { GetLastError() };
    let already_running = h_mutex.is_null() || last_err == ERROR_ALREADY_EXISTS;

    if already_running {
        // Zero-Bloat Win32 IPC: Wake up existing instance via FindWindowW + PostMessageW
        let tray_class: Vec<u16> = OsStr::new("FlickTrayWindowClass")
            .encode_wide()
            .chain(Some(0))
            .collect();
        let dash_class: Vec<u16> = OsStr::new("FlickDashboardWindowClass")
            .encode_wide()
            .chain(Some(0))
            .collect();

        let mut hwnd = unsafe { FindWindowW(tray_class.as_ptr(), std::ptr::null()) };
        if hwnd.is_null() {
            hwnd = unsafe { FindWindowW(dash_class.as_ptr(), std::ptr::null()) };
        }

        // Retry for up to 1 second in case primary instance is actively initializing
        for _ in 0..5 {
            if !hwnd.is_null() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(150));
            hwnd = unsafe { FindWindowW(tray_class.as_ptr(), std::ptr::null()) };
            if hwnd.is_null() {
                hwnd = unsafe { FindWindowW(dash_class.as_ptr(), std::ptr::null()) };
            }
        }

        if !hwnd.is_null() {
            let is_pair = std::env::args().any(|a| a == "--pair" || a == "-p");
            let msg = if is_pair {
                tray::WM_IPC_OPEN_PAIR
            } else {
                tray::WM_IPC_OPEN_DASHBOARD
            };
            unsafe {
                PostMessageW(hwnd, msg, 0, 0);
            }
        }

        if !h_mutex.is_null() {
            unsafe { CloseHandle(h_mutex) };
        }
        return;
    }

    // 2. Load Configuration and Cryptographic Credentials
    let cfg = config::get_config();

    let creds = match crypto::get_or_create_pc_credentials(&cfg) {
        Ok(c) => Arc::new(c),
        Err(e) => {
            eprintln!("Fatal error creating PC credentials: {}", e);
            std::process::exit(1);
        }
    };

    let server_config = match crypto::create_rustls_server_config(&creds) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("Fatal error configuring TLS server: {}", e);
            std::process::exit(1);
        }
    };

    // 3. Start Background Daemons: UDP Beacon & mTLS 1.3 HTTP Server
    let _beacon_handle = beacon::start_discovery_beacon(cfg.udp_port);
    let _server_handle = http_server::start_http_receiver(cfg.http_port, creds, server_config);

    // 4. Parse Startup Flags
    let is_pair = std::env::args().any(|a| a == "--pair" || a == "-p");
    let is_minimized = std::env::args().any(|a| a == "--minimized" || a == "-m");

    // 5. Enter Win32 Tray & GDI Message Loop on Main Thread
    tray::run_tray_message_loop(is_minimized, is_pair);

    // Clean up mutex on shutdown
    if !h_mutex.is_null() {
        unsafe { CloseHandle(h_mutex) };
    }
}
