#![windows_subsystem = "windows"]

mod beacon;
mod clipboard;
mod config;
mod crypto;
mod gui;
mod http_server;
pub mod landing;
mod tray;

use std::ffi::OsStr;
use std::os::windows::ffi::OsStrExt;
use std::sync::Arc;

use windows_sys::Win32::Foundation::{CloseHandle, GetLastError, SetLastError, ERROR_ALREADY_EXISTS};
use windows_sys::Win32::System::Threading::CreateMutexW;
use windows_sys::Win32::UI::WindowsAndMessaging::{FindWindowW, PostMessageW};

pub fn log_boot(step: &str) {
    if let Ok(mut f) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(
            std::path::Path::new(&std::env::var("USERPROFILE").unwrap_or_default())
                .join(".flick")
                .join("boot.log"),
        )
    {
        use std::io::Write;
        let _ = writeln!(f, "[{}]", step);
    }
}

const LOCAL_IPC_PORT: u16 = 45459;

fn send_local_ipc(cmd: &str) {
    if let Ok(socket) = std::net::UdpSocket::bind("127.0.0.1:0") {
        let _ = socket.send_to(cmd.as_bytes(), ("127.0.0.1", LOCAL_IPC_PORT));
    }
}

fn main() {
    log_boot("--- Process Invocation ---");
    log_boot("0. Panic hook setup");
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
        log_boot("Exit: Already running detected!");
        let sound_arg = std::env::args().position(|a| a == "--sound" || a == "-s")
            .and_then(|idx| std::env::args().nth(idx + 1));

        if let Some(ref snd) = sound_arg {
            let cmd = format!("SOUND:{}", snd.to_lowercase());
            send_local_ipc(&cmd);
        } else {
            let is_pair = std::env::args().any(|a| a == "--pair" || a == "-p");
            let is_test_landing = std::env::args().any(|a| a == "--test-landing");
            let is_test_batch = std::env::args().any(|a| a == "--test-batch" || a == "-b");
            let is_test_url = std::env::args().any(|a| a == "--test-url" || a == "-u");
            let is_test_text = std::env::args().any(|a| a == "--test-text" || a == "-t");
            let is_test_pdf = std::env::args().any(|a| a == "--test-pdf");
            let is_test_video = std::env::args().any(|a| a == "--test-video");
            if is_test_pdf {
                send_local_ipc("PDF");
            } else if is_test_video {
                send_local_ipc("VIDEO");
            } else if is_test_url {
                send_local_ipc("URL");
            } else if is_test_text {
                send_local_ipc("TEXT");
            } else if is_test_batch {
                send_local_ipc("BATCH");
            } else if is_test_landing {
                send_local_ipc("LANDING");
            } else if is_pair {
                send_local_ipc("PAIR");
            } else {
                send_local_ipc("DASHBOARD");
            }
        }

        // Also try Win32 PostMessage as secondary fallback if on same desktop
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

        if !hwnd.is_null() {
            log_boot(&format!("Found window hwnd: {:?}, args: {:?}", hwnd, std::env::args().collect::<Vec<_>>()));
            if let Some(snd) = sound_arg {
                let code = match snd.to_lowercase().as_str() {
                    "scifi" => 1,
                    "cinematic" => 2,
                    "studio" => 3,
                    _ => 0, // glass
                };
                unsafe {
                    PostMessageW(hwnd, tray::WM_IPC_SET_SOUND, code, 0);
                }
            } else {
                let is_pair = std::env::args().any(|a| a == "--pair" || a == "-p");
                let is_test_landing = std::env::args().any(|a| a == "--test-landing");
                let is_test_batch = std::env::args().any(|a| a == "--test-batch" || a == "-b");
                let msg = if is_test_batch {
                    tray::WM_IPC_TEST_BATCH
                } else if is_test_landing {
                    tray::WM_IPC_TEST_LANDING
                } else if is_pair {
                    tray::WM_IPC_OPEN_PAIR
                } else {
                    tray::WM_IPC_OPEN_DASHBOARD
                };
                unsafe {
                    PostMessageW(hwnd, msg, 0, 0);
                }
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

    log_boot("Starting daemons...");
    // 3. Start Background Daemons: UDP Beacon, mTLS 1.3 Server & Local IPC Listener
    let _beacon_handle = beacon::start_discovery_beacon(cfg.udp_port);
    let _server_handle = http_server::start_http_receiver(cfg.http_port, creds, server_config);

    let _ipc_handle = std::thread::Builder::new()
        .name("flick-local-ipc".to_string())
        .spawn(move || {
            log_boot("flick-local-ipc thread started");
            match std::net::UdpSocket::bind(("127.0.0.1", LOCAL_IPC_PORT)) {
                Ok(socket) => {
                    log_boot(&format!("Local IPC bound to 127.0.0.1:{}", LOCAL_IPC_PORT));
                    let mut buf = [0u8; 256];
                    while let Ok((amt, _)) = socket.recv_from(&mut buf) {
                        if let Ok(cmd) = std::str::from_utf8(&buf[..amt]) {
                            log_boot(&format!("Local IPC received command: {}", cmd));
                        match cmd {
                            "BATCH" => tray::trigger_sample_batch_landing(),
                            "LANDING" => tray::trigger_sample_landing(),
                            "URL" => landing::show_landing_url("https://github.com/torvalds/linux".to_string()),
                            "TEXT" => landing::show_landing_text("Flick Studio Card: Clean high-DPI typography for your clipboard!"),
                            "PDF" => {
                                if let Some(user_dirs) = std::env::var_os("USERPROFILE") {
                                    let dl = std::path::Path::new(&user_dirs).join("Downloads");
                                    if let Ok(entries) = std::fs::read_dir(dl) {
                                        for entry in entries.flatten() {
                                            let p = entry.path();
                                            if p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase() == "pdf" {
                                                landing::show_landing_files(vec![p]);
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                            "VIDEO" => {
                                if let Some(user_dirs) = std::env::var_os("USERPROFILE") {
                                    let dl = std::path::Path::new(&user_dirs).join("Downloads");
                                    if let Ok(entries) = std::fs::read_dir(dl) {
                                        for entry in entries.flatten() {
                                            let p = entry.path();
                                            let ext = p.extension().and_then(|e| e.to_str()).unwrap_or("").to_lowercase();
                                            if matches!(ext.as_str(), "mp4" | "mkv" | "mov" | "avi" | "webm") {
                                                landing::show_landing_files(vec![p]);
                                                break;
                                            }
                                        }
                                    }
                                }
                            }
                            "DASHBOARD" => gui::open_or_show_dashboard(false),
                            "PAIR" => gui::open_or_show_dashboard(true),
                            c if c.starts_with("SOUND:") => {
                                let theme = &c[6..];
                                config::set_sound_theme(theme);
                                http_server::preview_sound_theme(theme);
                            }
                            _ => {}
                        }
                    }
                }
            }
            Err(e) => {
                log_boot(&format!("Failed to bind local IPC on port {}: {}", LOCAL_IPC_PORT, e));
            }
        }
    });

    log_boot("Daemons started, starting tray loop...");

    // 4. Parse Startup Flags
    let is_pair = std::env::args().any(|a| a == "--pair" || a == "-p");
    let is_minimized = std::env::args().any(|a| a == "--minimized" || a == "-m");
    let is_test_landing = std::env::args().any(|a| a == "--test-landing");
    let is_test_batch = std::env::args().any(|a| a == "--test-batch" || a == "-b");

    if is_test_batch {
        tray::trigger_sample_batch_landing();
    } else if is_test_landing {
        tray::trigger_sample_landing();
    }

    // 5. Enter Win32 Tray & GDI Message Loop on Main Thread
    tray::run_tray_message_loop(is_minimized, is_pair);
    log_boot("Tray loop exited! Process ending.");

    // Clean up mutex on shutdown
    if !h_mutex.is_null() {
        unsafe { CloseHandle(h_mutex) };
    }
}
