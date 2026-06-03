// MicKey
// Copyright (c) 2026 ForFunGplDev
// Licensed under the GNU General Public License v3.0
// See LICENSE for details.

#![windows_subsystem = "windows"]

use std::cell::RefCell;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use windows::core::*;
use windows::Win32::Foundation::*;
use windows::Win32::Graphics::Gdi::*;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::*;
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::Win32::UI::Input::KeyboardAndMouse::*;

// Pull button state constants from the UI Controls module
use windows::Win32::UI::Controls::{BST_CHECKED, BST_UNCHECKED, DRAWITEMSTRUCT, TBM_SETRANGE, TBM_SETPOS, TBS_HORZ, TBS_NOTICKS, UDS_AUTOBUDDY, UDS_SETBUDDYINT, UDS_ALIGNRIGHT, UDS_ARROWKEYS};
const TBM_GETPOS: u32 = 0x0400;
use windows::Win32::UI::Controls::Dialogs::{ChooseColorW, CHOOSECOLORW, CC_FULLOPEN, CC_RGBINIT};

// Core Audio & COM imports
use windows::Win32::Media::Audio::*;
use windows::Win32::Media::Audio::Endpoints::*;
use windows::Win32::System::Com::*;
use windows::Win32::UI::Shell::PropertiesSystem::*;

// Define PKEY_Device_FriendlyName directly using its official Windows SDK GUID & PID
#[allow(non_upper_case_globals)]
const PKEY_Device_FriendlyName: PROPERTYKEY = PROPERTYKEY {
    fmtid: GUID::from_values(
        0xa45c254e,
        0xdf1c,
        0x4efd,
        [0x80, 0x20, 0x67, 0xd1, 0x46, 0xa8, 0x50, 0xe0],
    ),
    pid: 14,
};

const WM_TRAYICON: u32 = WM_USER + 1;
const WM_MUTE_CHANGED: u32 = WM_USER + 2;
const ID_TRAYICON: u32 = 1;

// Menu IDs
const ID_MENU_SETTINGS: u32 = 1001;
const ID_MENU_EXIT: u32 = 1002;
const ID_MENU_TOGGLE_TRAY: u32 = 1003;
const ID_MENU_TOGGLE_OVERLAY: u32 = 1004;

// Control IDs for the Settings Dialog
const ID_CTRL_DEVICE_COMBO: isize = 2000;
const ID_CTRL_OVERLAY_CHECK: isize = 2001;
const ID_CTRL_TRAY_CHECK: isize = 2002;
const ID_CTRL_HOTKEY_MODE_COMBO: isize = 2003;
const ID_CTRL_SIZE_EDIT: isize = 2004;
const ID_CTRL_CORNER_COMBO: isize = 2005;
const ID_CTRL_OFFSET_X_EDIT: isize = 2006;
const ID_CTRL_OFFSET_Y_EDIT: isize = 2007;
const ID_CTRL_SHAPE_COMBO: isize = 2008;
const ID_CTRL_SAVE_BTN: isize = 2009;
const ID_CTRL_DISCARD_BTN: isize = 2010;
const ID_CTRL_HOTKEY_BTN: isize = 2011;
const ID_CTRL_HOTKEY_DISPLAY: isize = 2012;

// Color picker button IDs
const ID_CTRL_COLOR_MUTED: isize       = 2013;
const ID_CTRL_COLOR_UNMUTED: isize     = 2014;
const ID_CTRL_COLOR_ERROR: isize       = 2015;

// Opacity edit field IDs (0–100%)
const ID_CTRL_OPACITY_MUTED: isize    = 2016;
const ID_CTRL_OPACITY_UNMUTED: isize  = 2017;
const ID_CTRL_OPACITY_ERROR: isize    = 2018;

// Opacity slider IDs
const ID_CTRL_SLIDER_MUTED: isize     = 2019;
const ID_CTRL_SLIDER_UNMUTED: isize   = 2020;
const ID_CTRL_SLIDER_ERROR: isize     = 2021;

// Spin control IDs
const ID_CTRL_SPIN_SIZE: isize        = 2022;
const ID_CTRL_SPIN_X: isize           = 2023;
const ID_CTRL_SPIN_Y: isize           = 2024;
const ID_CTRL_COPYRIGHT: isize        = 2025;

const HOTKEY_RECORD_TIMER_ID: usize = 9001;

thread_local! {
    // Flag: currently capturing a hotkey in the settings window
    static RECORDING_HOTKEY: RefCell<bool> = RefCell::new(false);
    // Settings window handle (invalid when closed)
    static SETTINGS_HWND: RefCell<HWND> = RefCell::new(HWND::default());
    // Device IDs parallel to the combo box (index 0 = default, entries start at 1)
    static DEVICE_LIST: RefCell<Vec<String>> = RefCell::new(Vec::new());
    // Tracks whether a momentary hotkey is currently held
    static HOTKEY_HELD: RefCell<bool> = RefCell::new(false);
    // Snapshot of AppState taken when settings opens, used to restore on Discard
    static SETTINGS_SNAPSHOT: RefCell<Option<AppState>> = RefCell::new(None);
}

// Icons embedded at compile time — no assets folder needed at runtime
static ICO_MICKEY:       &[u8] = include_bytes!("../assets/MicKey.ico");
static ICO_MICKEY_MUTED: &[u8] = include_bytes!("../assets/MicKeyMuted.ico");

// Generates pre-multiplied BGRA pixels for a filled circle with smooth edges.
fn make_circle_pixels(size: i32, color: [u8; 3], opacity_pct: u8) -> Vec<u32> {
    let global_alpha = opacity_pct as f32 / 100.0;
    let r = size as f32 / 2.0;
    let cx = r;
    let cy = r;
    let mut out = vec![0u32; (size * size) as usize];
    for y in 0..size {
        for x in 0..size {
            let dx = x as f32 + 0.5 - cx;
            let dy = y as f32 + 0.5 - cy;
            let dist = (dx * dx + dy * dy).sqrt();
            let coverage = (r - dist).clamp(0.0, 1.0);
            let a = (coverage * global_alpha * 255.0).round() as u32;
            if a == 0 {
                out[(y * size + x) as usize] = 0;
            } else {
                let pr = (color[0] as u32 * a / 255) as u32;
                let pg = (color[1] as u32 * a / 255) as u32;
                let pb = (color[2] as u32 * a / 255) as u32;
                out[(y * size + x) as usize] = (a << 24) | (pr << 16) | (pg << 8) | pb;
            }
        }
    }
    out
}

// Low-level keyboard hook handle — stored so we can unhook cleanly on exit
static KEYBOARD_HOOK: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

// Main window handle stored as an atomic so the COM callback thread can read it safely.
static MAIN_HWND_ATOMIC: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

// COM implementation of IAudioEndpointVolumeCallback.
// Windows calls OnNotify whenever mute or volume changes on the endpoint.
#[windows::core::implement(IAudioEndpointVolumeCallback)]
struct MuteCallback;

impl IAudioEndpointVolumeCallback_Impl for MuteCallback_Impl {
    fn OnNotify(&self, pnotify: *mut AUDIO_VOLUME_NOTIFICATION_DATA) -> windows::core::Result<()> {
        if pnotify.is_null() { return Ok(()); }
        unsafe {
            let data = &*pnotify;
            let muted = data.bMuted.as_bool();

            // Read the main HWND from the atomic; Safe from any thread.
            let raw = MAIN_HWND_ATOMIC.load(std::sync::atomic::Ordering::SeqCst);
            if raw != 0 {
                let main_hwnd = HWND(raw as *mut core::ffi::c_void);
                let _ = PostMessageW(
                    main_hwnd,
                    WM_MUTE_CHANGED,
                    WPARAM(if muted { 1 } else { 0 }),
                    LPARAM(0),
                );
            }
        }
        Ok(())
    }
}

// Holds the registered callback and endpoint so we can unregister cleanly.
struct VolumeWatcher {
    endpoint: IAudioEndpointVolume,
    callback: IAudioEndpointVolumeCallback,
}

thread_local! {
    static VOLUME_WATCHER: RefCell<Option<VolumeWatcher>> = RefCell::new(None);
}

// Registers the volume callback on the currently selected device.
// Drops any previous registration first.
unsafe fn register_volume_callback() {
    VOLUME_WATCHER.with(|w| {
        if let Some(old) = w.borrow_mut().take() {
            let _ = old.endpoint.UnregisterControlChangeNotify(&old.callback);
        }
    });

    let Some(endpoint) = get_audio_endpoint() else { return };

    let callback: IAudioEndpointVolumeCallback = MuteCallback.into();
    if endpoint.RegisterControlChangeNotify(&callback).is_ok() {
        VOLUME_WATCHER.with(|w| {
            *w.borrow_mut() = Some(VolumeWatcher { endpoint, callback });
        });
    }
}

unsafe extern "system" fn ll_keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if code >= 0 {
        let kb = &*(lparam.0 as *const KBDLLHOOKSTRUCT);
        let is_keyup = wparam.0 as u32 == WM_KEYUP || wparam.0 as u32 == WM_SYSKEYUP;

        if is_keyup && HOTKEY_HELD.with(|h| *h.borrow()) {
            let hotkey_str = STATE.with(|s| s.borrow().hotkey_str.clone());
            if let Some((_mods, vk)) = parse_hotkey_str(&hotkey_str) {
                if kb.vkCode == vk {
                    let main_hwnd = STATE.with(|s| s.borrow().main_hwnd);
                    if !main_hwnd.is_invalid() {
                        let _ = PostMessageW(main_hwnd, WM_APP, WPARAM(1), LPARAM(0));
                    }
                }
            }
        }
    }
    let hook_raw = KEYBOARD_HOOK.load(std::sync::atomic::Ordering::SeqCst);
    CallNextHookEx(HHOOK(hook_raw as *mut core::ffi::c_void), code, wparam, lparam)
}

#[derive(Clone)]
struct AudioDevice {
    name: String,
    id: String,
}

#[derive(Debug, Clone, PartialEq)]
enum MuteState { Unmuted, Muted, Error }

// Global application state matching your build spec
#[derive(Debug, Clone)]
struct AppState {
    mute_state: MuteState,
    show_tray_icon: bool,
    show_overlay: bool,
    overlay_hwnd: HWND,
    main_hwnd: HWND,

    // Configurable Settings
    glyph_size: i32,     // 1 to 64 px
    glyph_corner: u32,   // 0: Top-Left, 1: Top-Right, 2: Bottom-Left, 3: Bottom-Right
    offset_x: i32,       // -9999 to 9999 px (negative values reach monitors left/above primary)
    offset_y: i32,       // -9999 to 9999 px (negative values reach monitors above primary)
    glyph_shape: u32,    // 0: MicKey Icon, 1: Circle, 2: Square

    // Device ID override — empty string means use system default communication device
    device_override: String,

    // Hotkey
    hotkey_str: String,  // Human-readable combo, e.g. "Ctrl+Alt+M"
    hotkey_mode: u32,    // 0: Toggle, 1: Push-to-talk, 2: Push-to-mute

    // Colors stored as [r, g, b] and opacity stored as 0–100%
    color_muted:      [u8; 3],
    color_unmuted:    [u8; 3],
    color_error:      [u8; 3],
    opacity_muted:    u8,   // 0–100
    opacity_unmuted:  u8,   // 0–100
    opacity_error:    u8,   // 0–100
}

// Thread-local storage to safely manage state inside window callbacks
thread_local! {
    static STATE: RefCell<AppState> = RefCell::new(AppState {
        mute_state: MuteState::Unmuted,
        show_tray_icon: true,
        show_overlay: true,
        overlay_hwnd: HWND::default(),
        main_hwnd: HWND::default(),

        // Defaults
        glyph_size: 24,
        glyph_corner: 0,     // Top-Left
        offset_x: 8,
        offset_y: 8,
        glyph_shape: 0,      // MicKey Icon

        device_override: String::new(),
        hotkey_str: String::new(),
        hotkey_mode: 0,      // Toggle

        color_muted:     [255, 0,   0  ],
        color_unmuted:   [0,   255, 0  ],
        color_error:     [128, 128, 128],
        opacity_muted:   100,
        opacity_unmuted: 100,
        opacity_error:   100,
    });
}

// Returns the path to mickey.ini next to the executable.
fn get_ini_path() -> PathBuf {
    if let Ok(mut exe_path) = std::env::current_exe() {
        exe_path.set_file_name("mickey.ini");
        exe_path
    } else {
        PathBuf::from("mickey.ini")
    }
}

// Loads settings from mickey.ini; creates it with defaults if absent.
fn load_settings_from_ini() {
    let path = get_ini_path();
    if !path.exists() {
        save_settings_to_ini();
        return;
    }

    if let Ok(file) = File::open(path) {
        let reader = BufReader::new(file);
        STATE.with(|s| {
            let mut state = s.borrow_mut();
            for line in reader.lines().map_while(std::result::Result::ok) {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with(';') || trimmed.starts_with('#') {
                    continue;
                }
                if let Some(idx) = trimmed.find('=') {
                    let key = trimmed[..idx].trim();
                    let val = trimmed[idx + 1..].trim();

                    match key {
                        "show_tray_icon" => if let Ok(v) = val.parse::<bool>() { state.show_tray_icon = v; },
                        "show_overlay"   => if let Ok(v) = val.parse::<bool>() { state.show_overlay = v; },
                        "glyph_size"     => if let Ok(v) = val.parse::<i32>()  { state.glyph_size = v.clamp(1, 64); },
                        "glyph_corner" => {
                            state.glyph_corner = match val {
                                "top-left"     | "0" => 0,
                                "top-right"    | "1" => 1,
                                "bottom-left"  | "2" => 2,
                                "bottom-right" | "3" => 3,
                                _ => 0,
                            };
                        }
                        "offset_x"  => if let Ok(v) = val.parse::<i32>() { state.offset_x = v.clamp(-9999, 9999); },
                        "offset_y"  => if let Ok(v) = val.parse::<i32>() { state.offset_y = v.clamp(-9999, 9999); },
                        "glyph_shape" => {
                            state.glyph_shape = match val {
                                "icon"   | "0" => 0,
                                "circle" | "1" => 1,
                                "square" | "2" => 2,
                                _ => 0,
                            };
                        }
                        "device_override"    => { state.device_override = if val == "default" { String::new() } else { val.to_string() }; }
                        "selected_device_id" => { state.device_override = if val.is_empty() { String::new() } else { val.to_string() }; }
                        "hotkey_str"  => { state.hotkey_str = val.to_string(); }
                        "hotkey_mode" => {
                            state.hotkey_mode = match val {
                                "toggle" | "0" => 0,
                                "ptt"    | "1" => 1,
                                "ptm"    | "2" => 2,
                                _ => 0,
                            };
                        }
                        "color_muted"    => { if let Some(c) = parse_color_hex(val) { state.color_muted = c; } }
                        "color_unmuted"  => { if let Some(c) = parse_color_hex(val) { state.color_unmuted = c; } }
                        "color_error"    => { if let Some(c) = parse_color_hex(val) { state.color_error = c; } }
                        "opacity_muted"    => if let Ok(v) = val.parse::<u8>() { state.opacity_muted   = v.min(100); }
                        "opacity_unmuted"  => if let Ok(v) = val.parse::<u8>() { state.opacity_unmuted = v.min(100); }
                        "opacity_error"    => if let Ok(v) = val.parse::<u8>() { state.opacity_error   = v.min(100); }
                        _ => {}
                    }
                }
            }
        });
    }
}

// Writes the current AppState to mickey.ini.
fn save_settings_to_ini() {
    let path = get_ini_path();
    if let Ok(mut file) = File::create(path) {
        STATE.with(|s| {
            let state = s.borrow();
            let _ = writeln!(file, "[Settings]");
            let _ = writeln!(file, "show_tray_icon={}", state.show_tray_icon);
            let _ = writeln!(file, "show_overlay={}", state.show_overlay);
            let _ = writeln!(file, "glyph_size={}", state.glyph_size);
            let corner_str = match state.glyph_corner {
                0 => "top-left", 1 => "top-right", 2 => "bottom-left", _ => "bottom-right",
            };
            let shape_str = match state.glyph_shape {
                0 => "icon", 1 => "circle", _ => "square",
            };
            let mode_str = match state.hotkey_mode {
                0 => "toggle", 1 => "ptt", _ => "ptm",
            };
            let _ = writeln!(file, "glyph_corner={}", corner_str);
            let _ = writeln!(file, "offset_x={}", state.offset_x);
            let _ = writeln!(file, "offset_y={}", state.offset_y);
            let _ = writeln!(file, "glyph_shape={}", shape_str);
            let _ = writeln!(file, "device_override={}", state.device_override);
            let _ = writeln!(file, "hotkey_str={}", state.hotkey_str);
            let _ = writeln!(file, "hotkey_mode={}", mode_str);
            let _ = writeln!(file, "color_muted={}", fmt_color_hex(state.color_muted));
            let _ = writeln!(file, "opacity_muted={}", state.opacity_muted);
            let _ = writeln!(file, "color_unmuted={}", fmt_color_hex(state.color_unmuted));
            let _ = writeln!(file, "opacity_unmuted={}", state.opacity_unmuted);
            let _ = writeln!(file, "color_error={}", fmt_color_hex(state.color_error));
            let _ = writeln!(file, "opacity_error={}", state.opacity_error);
        });
    }
}

fn main() -> Result<()> {
    unsafe {
        // Initialize COM for Core Audio APIs
        let _ = CoInitializeEx(None, COINIT_APARTMENTTHREADED);

        load_settings_from_ini();
        if let Ok(hook) = SetWindowsHookExW(WH_KEYBOARD_LL, Some(ll_keyboard_hook), None, 0) {
            KEYBOARD_HOOK.store(hook.0 as isize, std::sync::atomic::Ordering::SeqCst);
        }

        let instance = GetModuleHandleW(None)?;
        let window_class = w!("MicKeyClass");

        let wc = WNDCLASSW {
            lpfnWndProc: Some(wnd_proc),
            hInstance: instance.into(),
            lpszClassName: window_class,
            ..Default::default()
        };

        RegisterClassW(&wc);

        let hwnd = CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            window_class,
            w!("MicKey"),
            WINDOW_STYLE::default(),
            CW_USEDEFAULT, CW_USEDEFAULT, CW_USEDEFAULT, CW_USEDEFAULT,
            HWND_MESSAGE, None, instance, None,
        )?;

        STATE.with(|s| s.borrow_mut().main_hwnd = hwnd);
        MAIN_HWND_ATOMIC.store(hwnd.0 as isize, std::sync::atomic::Ordering::SeqCst);
        register_app_hotkey();

        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: hwnd,
            uID: ID_TRAYICON,
            uFlags: NIF_MESSAGE | NIF_TIP | NIF_ICON,
            uCallbackMessage: WM_TRAYICON,
            hIcon: create_tray_icon_for_state().unwrap_or(HICON::default()),
            ..Default::default()
        };
        
        let tip = to_wide("MicKey Mute");
        nid.szTip[..tip.len()].copy_from_slice(&tip);

        if STATE.with(|s| s.borrow().show_tray_icon) {
            let _ = Shell_NotifyIconW(NIM_ADD, &nid);
        }

        if let Ok(overlay_hwnd) = create_overlay_window() {
            STATE.with(|s| s.borrow_mut().overlay_hwnd = overlay_hwnd);
        }

        sync_mute_state();
        register_volume_callback();

        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let settings_hwnd = SETTINGS_HWND.with(|s| *s.borrow());
            if !settings_hwnd.is_invalid() && IsDialogMessageW(settings_hwnd, &msg).as_bool() {
                continue;
            }
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }

        let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
        let main_hwnd = STATE.with(|s| s.borrow().main_hwnd);
        let _ = UnregisterHotKey(main_hwnd, 1);
        let hook_raw = KEYBOARD_HOOK.load(std::sync::atomic::Ordering::SeqCst);
        if hook_raw != 0 {
            let _ = UnhookWindowsHookEx(HHOOK(hook_raw as *mut core::ffi::c_void));
        }
        // Unregister volume callback before COM uninitialize
        VOLUME_WATCHER.with(|w| {
            if let Some(old) = w.borrow_mut().take() {
                let _ = old.endpoint.UnregisterControlChangeNotify(&old.callback);
            }
        });
        CoUninitialize();
    }
    Ok(())
}

unsafe fn enumerate_microphones() -> (Option<String>, Vec<AudioDevice>) {
    let mut default_name_out: Option<String> = None;
    let mut devices_list = Vec::new();

    let enumerator_result: Result<IMMDeviceEnumerator> = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL);
    if let Ok(enumerator) = enumerator_result {
        if let Ok(default_device) = enumerator.GetDefaultAudioEndpoint(eCapture, eCommunications) {
            if let Ok(prop_store) = default_device.OpenPropertyStore(STGM_READ) {
                if let Ok(var) = prop_store.GetValue(&PKEY_Device_FriendlyName) {
                    let raw_name = var.to_string();
                    if !raw_name.is_empty() {
                        default_name_out = Some(raw_name);
                    }
                }
            }
        }

        if let Ok(collection) = enumerator.EnumAudioEndpoints(eCapture, DEVICE_STATE_ACTIVE) {
            if let Ok(count) = collection.GetCount() {
                for i in 0..count {
                    if let Ok(device) = collection.Item(i) {
                        if let (Ok(id_ptr), Ok(prop_store)) = (device.GetId(), device.OpenPropertyStore(STGM_READ)) {
                            let id_str = id_ptr.to_string().unwrap_or_default();
                            if let Ok(var) = prop_store.GetValue(&PKEY_Device_FriendlyName) {
                                let friendly_name = var.to_string();
                                devices_list.push(AudioDevice {
                                    name: friendly_name,
                                    id: id_str,
                                });
                            }
                        }
                    }
                }
            }
        }
    }
    (default_name_out, devices_list)
}

// Returns colorized RGBA pixels (pre-multiplied, BGRA u32) for use with UpdateLayeredWindow.
// Also returns an HICON built from those pixels for use with the tray.
unsafe fn load_and_colorize_icon_pixels(ico_bytes: &[u8], size: i32, color: [u8; 3]) -> Option<Vec<u32>> {
    let hicon_src = hicon_from_bytes(ico_bytes, size)?;

    let mut ii = ICONINFO::default();
    if GetIconInfo(hicon_src, &mut ii).is_err() {
        let _ = DestroyIcon(hicon_src);
        return None;
    }
    let _ = DestroyIcon(hicon_src);

    let hdc_screen = GetDC(HWND::default());
    let hdc = CreateCompatibleDC(hdc_screen);

    let mut bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size, biHeight: -size,
            biPlanes: 1, biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let pixel_count = (size * size) as usize;
    let mut color_pixels: Vec<u32> = vec![0u32; pixel_count];

    if !ii.hbmColor.is_invalid() {
        GetDIBits(hdc, ii.hbmColor, 0, size as u32,
            Some(color_pixels.as_mut_ptr() as *mut _), &mut bmi, DIB_RGB_COLORS);
    }

    let row_stride = ((size as usize + 31) / 32) * 4;
    let mut mask_pixels: Vec<u8> = vec![0u8; row_stride * size as usize];
    if !ii.hbmMask.is_invalid() {
        let mut mask_bmi = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: size, biHeight: size,
                biPlanes: 1, biBitCount: 1,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        GetDIBits(hdc, ii.hbmMask, 0, size as u32,
            Some(mask_pixels.as_mut_ptr() as *mut _), &mut mask_bmi, DIB_RGB_COLORS);
    }

    if !ii.hbmColor.is_invalid() { let _ = DeleteObject(ii.hbmColor); }
    if !ii.hbmMask.is_invalid()  { let _ = DeleteObject(ii.hbmMask); }
    let _ = DeleteDC(hdc);
    ReleaseDC(HWND::default(), hdc_screen);

    let has_alpha = color_pixels.iter().any(|&p| (p >> 24) != 0);
    let mut out: Vec<u32> = vec![0u32; pixel_count];

    for y in 0..size as usize {
        for x in 0..size as usize {
            let idx = y * size as usize + x;
            let src = color_pixels[idx];
            let alpha: u8 = if has_alpha {
                (src >> 24) as u8
            } else {
                let mask_row = (size as usize - 1) - y;
                let byte_idx = mask_row * row_stride + x / 8;
                let bit_idx  = 7 - (x % 8);
                let masked   = (mask_pixels[byte_idx] >> bit_idx) & 1;
                if masked == 0 { 255 } else { 0 }
            };
            if alpha == 0 {
                out[idx] = 0;
            } else {
                // Use only alpha as the shape mask; source RGB ignored entirely.
                out[idx] = (alpha as u32) << 24
                    | (color[0] as u32) << 16
                    | (color[1] as u32) << 8
                    | color[2] as u32;
            }
        }
    }
    Some(out)
}

// Builds an HICON from a pixel buffer (for tray use).
unsafe fn pixels_to_hicon(pixels: &[u32], size: i32) -> Option<HICON> {
    let hdc_screen = GetDC(HWND::default());
    let hdc = CreateCompatibleDC(hdc_screen);
    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size, biHeight: -size,
            biPlanes: 1, biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };
    let mut out_bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let hbm_out = CreateDIBSection(hdc, &bmi, DIB_RGB_COLORS, &mut out_bits, None, 0).ok()?;
    let out_slice = std::slice::from_raw_parts_mut(out_bits as *mut u32, pixels.len());
    out_slice.copy_from_slice(pixels);

    let hbm_mask = CreateBitmap(size, size, 1, 1, None);
    let hdc_mask = CreateCompatibleDC(hdc_screen);
    let old_mask = SelectObject(hdc_mask, hbm_mask);
    let mask_rect = RECT { left: 0, top: 0, right: size, bottom: size };
    FillRect(hdc_mask, &mask_rect, HBRUSH(GetStockObject(WHITE_BRUSH).0 as *mut core::ffi::c_void));

    let icon_info = ICONINFO {
        fIcon: BOOL(1), xHotspot: 0, yHotspot: 0,
        hbmMask: hbm_mask, hbmColor: hbm_out,
    };
    let result = CreateIconIndirect(&icon_info).ok();

    SelectObject(hdc_mask, old_mask);
    let _ = DeleteObject(hbm_out);
    let _ = DeleteObject(hbm_mask);
    let _ = DeleteDC(hdc);
    let _ = DeleteDC(hdc_mask);
    ReleaseDC(HWND::default(), hdc_screen);
    result
}

unsafe fn load_and_colorize_icon(ico_bytes: &[u8], size: i32, color: [u8; 3]) -> Option<HICON> {
    let pixels = load_and_colorize_icon_pixels(ico_bytes, size, color)?;
    pixels_to_hicon(&pixels, size)
}

// Loads an HICON from embedded bytes using CreateIconFromResourceEx.
// The ICO format starts with a directory; we find the best-matching entry for `size`.
unsafe fn hicon_from_bytes(ico_bytes: &[u8], size: i32) -> Option<HICON> {
    // ICO directory: 6-byte header, then 16-byte entries
    if ico_bytes.len() < 6 { return None; }
    let count = u16::from_le_bytes([ico_bytes[4], ico_bytes[5]]) as usize;

    let mut best_offset: usize = 0;
    let mut best_size: i32 = 0;

    for i in 0..count {
        let entry = 6 + i * 16;
        if entry + 16 > ico_bytes.len() { break; }
        let w = ico_bytes[entry] as i32;
        let img_size = u32::from_le_bytes([
            ico_bytes[entry + 8], ico_bytes[entry + 9],
            ico_bytes[entry + 10], ico_bytes[entry + 11],
        ]) as usize;
        let offset = u32::from_le_bytes([
            ico_bytes[entry + 12], ico_bytes[entry + 13],
            ico_bytes[entry + 14], ico_bytes[entry + 15],
        ]) as usize;

        if offset + img_size > ico_bytes.len() { continue; }

        // Pick the entry whose width is closest to requested size
        if best_size == 0 || (w - size).abs() < (best_size - size).abs() {
            best_size = w;
            best_offset = offset;
        }
    }

    if best_offset == 0 { return None; }

    // Find image size for best entry
    let mut img_size = 0usize;
    for i in 0..count {
        let entry = 6 + i * 16;
        let offset = u32::from_le_bytes([
            ico_bytes[entry + 12], ico_bytes[entry + 13],
            ico_bytes[entry + 14], ico_bytes[entry + 15],
        ]) as usize;
        if offset == best_offset {
            img_size = u32::from_le_bytes([
                ico_bytes[entry + 8], ico_bytes[entry + 9],
                ico_bytes[entry + 10], ico_bytes[entry + 11],
            ]) as usize;
            break;
        }
    }

    if img_size == 0 { return None; }

    CreateIconFromResourceEx(
        &ico_bytes[best_offset..best_offset + img_size],
        BOOL(1),
        0x00030000,
        size,
        size,
        LR_DEFAULTCOLOR,
    ).ok()
}

// Creates a tray-sized (16x16) HICON for the current mute state + shape.
unsafe fn create_tray_icon_for_state() -> Option<HICON> {
    let (c, shape) = STATE.with(|s| {
        let st = s.borrow();
        let c = match st.mute_state {
            MuteState::Muted   => st.color_muted,
            MuteState::Unmuted => st.color_unmuted,
            MuteState::Error   => st.color_error,
        };
        (c, st.glyph_shape)
    });

    match shape {
        0 => {
            let is_muted = STATE.with(|s| s.borrow().mute_state == MuteState::Muted);
            let bytes = if is_muted { ICO_MICKEY_MUTED } else { ICO_MICKEY };
            load_and_colorize_icon(bytes, 16, c)
        }
        1 => {
            let pixels = make_circle_pixels(16, c, 100);
            pixels_to_hicon(&pixels, 16)
        }
        _ => {
            // Square: draw programmatically
            let size = 16i32;
            let hdc_screen = GetDC(HWND::default());
            let hdc = CreateCompatibleDC(hdc_screen);
            let hbm = CreateCompatibleBitmap(hdc_screen, size, size);
            let old_bm = SelectObject(hdc, hbm);
            let brush = CreateSolidBrush(rgb_to_colorref(c));
            let rect = RECT { left: 0, top: 0, right: size, bottom: size };
            FillRect(hdc, &rect, brush);
            let _ = DeleteObject(brush);
            let hbm_mask = CreateCompatibleBitmap(hdc_screen, size, size);
            let hdc_mask = CreateCompatibleDC(hdc_screen);
            let old_mask = SelectObject(hdc_mask, hbm_mask);
            FillRect(hdc_mask, &rect, HBRUSH(GetStockObject(BLACK_BRUSH).0 as *mut core::ffi::c_void));
            let ii = ICONINFO { fIcon: BOOL(1), xHotspot: 0, yHotspot: 0, hbmMask: hbm_mask, hbmColor: hbm };
            let hicon = CreateIconIndirect(&ii).ok();
            SelectObject(hdc, old_bm);
            SelectObject(hdc_mask, old_mask);
            let _ = DeleteObject(hbm);
            let _ = DeleteObject(hbm_mask);
            let _ = DeleteDC(hdc);
            let _ = DeleteDC(hdc_mask);
            ReleaseDC(HWND::default(), hdc_screen);
            hicon
        }
    }
}

// Updates the tray icon to reflect the current mute state.
unsafe fn update_tray_icon_color() {
    let (main_hwnd, show_tray, mute_state) = STATE.with(|s| {
        let st = s.borrow();
        (st.main_hwnd, st.show_tray_icon, st.mute_state.clone())
    });
    if main_hwnd.is_invalid() || !show_tray { return; }

    if let Some(hicon) = create_tray_icon_for_state() {
        let tip_str = match mute_state {
            MuteState::Muted   => "MicKey - Muted",
            MuteState::Unmuted => "MicKey - Active",
            MuteState::Error   => "MicKey - Error",
        };
        let mut nid = NOTIFYICONDATAW {
            cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
            hWnd: main_hwnd,
            uID: ID_TRAYICON,
            uFlags: NIF_ICON | NIF_TIP,
            hIcon: hicon,
            ..Default::default()
        };
        let tip = to_wide(tip_str);
        nid.szTip[..tip.len()].copy_from_slice(&tip);
        let _ = Shell_NotifyIconW(NIM_MODIFY, &nid);
        let _ = DestroyIcon(hicon);
    }
}

// Composites the current shape onto the layered window surface with per-pixel alpha.
unsafe fn update_overlay_layered(overlay_hwnd: HWND, size: i32, shape: u32, c: [u8; 3], opacity_pct: u8) {
    let ex_style = GetWindowLongW(overlay_hwnd, GWL_EXSTYLE);
    SetWindowLongW(overlay_hwnd, GWL_EXSTYLE, ex_style & !(WS_EX_LAYERED.0 as i32));
    SetWindowLongW(overlay_hwnd, GWL_EXSTYLE, ex_style | WS_EX_LAYERED.0 as i32);

    let hdc_screen = GetDC(HWND::default());
    let hdc_mem = CreateCompatibleDC(hdc_screen);

    let bmi = BITMAPINFO {
        bmiHeader: BITMAPINFOHEADER {
            biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
            biWidth: size,
            biHeight: -size,
            biPlanes: 1,
            biBitCount: 32,
            biCompression: BI_RGB.0,
            ..Default::default()
        },
        ..Default::default()
    };

    let mut bits: *mut core::ffi::c_void = std::ptr::null_mut();
    let hbm = match CreateDIBSection(hdc_mem, &bmi, DIB_RGB_COLORS, &mut bits, None, 0) {
        Ok(b) => b,
        Err(_) => {
            let _ = DeleteDC(hdc_mem);
            ReleaseDC(HWND::default(), hdc_screen);
            return;
        }
    };
    let old_bm = SelectObject(hdc_mem, hbm);

    let pixel_count = (size * size) as usize;
    let pixels = std::slice::from_raw_parts_mut(bits as *mut u32, pixel_count);

    for p in pixels.iter_mut() { *p = 0; }

    let is_muted = STATE.with(|s| s.borrow().mute_state == MuteState::Muted);

    if shape == 1 {
        // Circle: generated programmatically, alpha already pre-multiplied
        let circle_pixels = make_circle_pixels(size, c, opacity_pct);
        for (i, p) in pixels.iter_mut().enumerate() {
            *p = circle_pixels[i];
        }
    } else {
        let ico_bytes: &[u8] = if is_muted { ICO_MICKEY_MUTED } else { ICO_MICKEY };
        if let Some(icon_pixels) = load_and_colorize_icon_pixels(ico_bytes, size, c) {
            let global_alpha = opacity_pct as u32 * 255 / 100;
            for (i, p) in pixels.iter_mut().enumerate() {
                let src = icon_pixels[i];
                let a_raw = (src >> 24) as u32;
                let a_final = a_raw * global_alpha / 255;
                if a_final == 0 {
                    *p = 0;
                } else {
                    // Pre-multiply RGB by final alpha for UpdateLayeredWindow
                    let r = ((src >> 16) & 0xFF) as u32;
                    let g = ((src >> 8) & 0xFF) as u32;
                    let b = (src & 0xFF) as u32;
                    *p = (a_final << 24)
                        | ((r * a_final / 255) << 16)
                        | ((g * a_final / 255) << 8)
                        | (b * a_final / 255);
                }
            }
        }
    }

    let mut wnd_rect = RECT::default();
    let _ = GetWindowRect(overlay_hwnd, &mut wnd_rect);
    let wnd_pos = POINT { x: wnd_rect.left, y: wnd_rect.top };
    let src_pos = POINT { x: 0, y: 0 };
    let wnd_size = SIZE { cx: size, cy: size };

    let blend = BLENDFUNCTION {
        BlendOp: 0,              // AC_SRC_OVER
        BlendFlags: 0,
        SourceConstantAlpha: 255, // opacity baked into per-pixel alpha
        AlphaFormat: 1,           // AC_SRC_ALPHA
    };

    let _ = UpdateLayeredWindow(
        overlay_hwnd,
        hdc_screen,
        Some(&wnd_pos),
        Some(&wnd_size),
        hdc_mem,
        Some(&src_pos),
        COLORREF(0),
        Some(&blend),
        ULW_ALPHA,
    );

    SelectObject(hdc_mem, old_bm);
    let _ = DeleteObject(hbm);
    let _ = DeleteDC(hdc_mem);
    ReleaseDC(HWND::default(), hdc_screen);
}

unsafe fn redraw_overlay() {
    let info = STATE.with(|s| {
        let st = s.borrow();
        if st.overlay_hwnd.is_invalid() { return None; }
        let primary_w = GetSystemMetrics(SM_CXSCREEN);
        let primary_h = GetSystemMetrics(SM_CYSCREEN);
        let win_x = match st.glyph_corner {
            0 | 2 => st.offset_x,
            _ => primary_w - st.glyph_size - st.offset_x,
        };
        let win_y = match st.glyph_corner {
            0 | 1 => st.offset_y,
            _ => primary_h - st.glyph_size - st.offset_y,
        };
        let opacity_pct = match st.mute_state {
            MuteState::Muted   => st.opacity_muted,
            MuteState::Unmuted => st.opacity_unmuted,
            MuteState::Error   => st.opacity_error,
        };
        let c = match st.mute_state {
            MuteState::Muted   => st.color_muted,
            MuteState::Unmuted => st.color_unmuted,
            MuteState::Error   => st.color_error,
        };
        Some((st.overlay_hwnd, win_x, win_y, st.glyph_size, st.glyph_shape, opacity_pct, c))
    });

    if let Some((overlay_hwnd, win_x, win_y, glyph_size, shape, opacity_pct, c)) = info {
        let _ = SetWindowPos(
            overlay_hwnd, HWND_TOPMOST,
            win_x, win_y, glyph_size, glyph_size,
            SWP_NOACTIVATE,
        );

        if shape < 2 {
            // MicKey or Circle: per-pixel alpha via UpdateLayeredWindow; no halo
            update_overlay_layered(overlay_hwnd, glyph_size, shape, c, opacity_pct);
        } else {
            // Square: whole-window alpha via SetLayeredWindowAttributes + WM_PAINT
            let alpha = (opacity_pct as u32 * 255 / 100) as u8;
            let _ = SetLayeredWindowAttributes(overlay_hwnd, COLORREF(0), alpha, LWA_ALPHA);
            let _ = InvalidateRect(overlay_hwnd, None, BOOL(1));
        }
    }

    update_tray_icon_color();
}

unsafe fn sync_visibilities() {
    STATE.with(|s| {
        let state = s.borrow();
        if !state.overlay_hwnd.is_invalid() {
            let cmd = if state.show_overlay { SW_SHOWNOACTIVATE } else { SW_HIDE };
            let _ = ShowWindow(state.overlay_hwnd, cmd);
        }
        if !state.main_hwnd.is_invalid() {
            let mut nid = NOTIFYICONDATAW {
                cbSize: std::mem::size_of::<NOTIFYICONDATAW>() as u32,
                hWnd: state.main_hwnd,
                uID: ID_TRAYICON,
                ..Default::default()
            };
            if state.show_tray_icon {
                if let Some(hicon) = create_tray_icon_for_state() {
                    nid.uFlags = NIF_MESSAGE | NIF_TIP | NIF_ICON;
                    nid.uCallbackMessage = WM_TRAYICON;
                    nid.hIcon = hicon;
                    let tip = to_wide("MicKey Mute");
                    nid.szTip[..tip.len()].copy_from_slice(&tip);
                    let _ = Shell_NotifyIconW(NIM_ADD, &nid);
                    let _ = DestroyIcon(hicon);
                }
            } else {
                let _ = Shell_NotifyIconW(NIM_DELETE, &nid);
            }
        }
    });
}

unsafe extern "system" fn wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_TRAYICON => {
            match lparam.0 as u32 {
                WM_LBUTTONUP => {
                    let is_muted = STATE.with(|s| s.borrow().mute_state == MuteState::Muted);
                    set_mic_mute(!is_muted);
                }
                WM_RBUTTONUP => {
                    let _ = SetForegroundWindow(hwnd);
                    if let Ok(hmenu) = CreatePopupMenu() {
                        let _ = AppendMenuW(hmenu, MF_STRING, ID_MENU_SETTINGS as usize, w!("Settings"));
                        let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, None);

                        let (tray_checked, overlay_checked) = STATE.with(|s| {
                            let state = s.borrow();
                            (
                                if state.show_tray_icon { MF_CHECKED } else { MF_UNCHECKED },
                                if state.show_overlay { MF_CHECKED } else { MF_UNCHECKED },
                            )
                        });

                        let _ = AppendMenuW(hmenu, tray_checked | MF_STRING, ID_MENU_TOGGLE_TRAY as usize, w!("Toggle Tray Icon"));
                        let _ = AppendMenuW(hmenu, overlay_checked | MF_STRING, ID_MENU_TOGGLE_OVERLAY as usize, w!("Toggle Overlay"));
                        let _ = AppendMenuW(hmenu, MF_SEPARATOR, 0, None);
                        let _ = AppendMenuW(hmenu, MF_STRING, ID_MENU_EXIT as usize, w!("Exit"));

                        let mut pt = POINT::default();
                        let _ = GetCursorPos(&mut pt);
                        let _ = TrackPopupMenu(hmenu, TPM_RIGHTALIGN | TPM_BOTTOMALIGN, pt.x, pt.y, 0, hwnd, None);
                    }
                }
                _ => {}
            }
            LRESULT(0)
        }
        WM_COMMAND => {
            match wparam.0 as u32 {
                ID_MENU_SETTINGS => show_settings_dialog(),
                ID_MENU_TOGGLE_TRAY => {
                    STATE.with(|s| {
                        let mut state = s.borrow_mut();
                        state.show_tray_icon = !state.show_tray_icon;
                    });
                    save_settings_to_ini();
                    sync_visibilities();
                }
                ID_MENU_TOGGLE_OVERLAY => {
                    STATE.with(|s| {
                        let mut state = s.borrow_mut();
                        state.show_overlay = !state.show_overlay;
                    });
                    save_settings_to_ini();
                    sync_visibilities();
                }
                ID_MENU_EXIT => PostQuitMessage(0),
                _ => {}
            }
            LRESULT(0)
        }
        WM_HOTKEY => {
            if wparam.0 == 1 {
                // If the settings window is recording a new hotkey, swallow the WM_HOTKEY entirely so it doesn't toggle mute mid-recording.
                if RECORDING_HOTKEY.with(|r| *r.borrow()) {
                    return LRESULT(0);
                }
                let (mode, is_muted) = STATE.with(|s| {
                    let st = s.borrow();
                    (st.hotkey_mode, st.mute_state == MuteState::Muted)
                });
                match mode {
                    0 => {
                        // Toggle: flip mute state on each press
                        set_mic_mute(!is_muted);
                    }
                    1 => {
                        // Push-to-talk: unmute on keydown, re-mute on keyup
                        set_mic_mute(false);
                        HOTKEY_HELD.with(|h| *h.borrow_mut() = true);
                    }
                    2 => {
                        // Push-to-mute: mute on keydown, unmute on keyup
                        set_mic_mute(true);
                        HOTKEY_HELD.with(|h| *h.borrow_mut() = true);
                    }
                    _ => {}
                }
            }
            LRESULT(0)
        }

        WM_MUTE_CHANGED => {
            let hw_muted = wparam.0 == 1;
            STATE.with(|s| {
                s.borrow_mut().mute_state = if hw_muted {
                    MuteState::Muted
                } else {
                    MuteState::Unmuted
                };
            });
            redraw_overlay();
            LRESULT(0)
        }

        WM_APP => {
            // Keyboard hook sends this when the hotkey is released (PTT/PTM modes)
            if wparam.0 == 1 {
                let mode = STATE.with(|s| s.borrow().hotkey_mode);
                let held = HOTKEY_HELD.with(|h| {
                    let v = *h.borrow();
                    *h.borrow_mut() = false;
                    v
                });
                if held {
                    match mode {
                        1 => { set_mic_mute(true);  } // PTT release → mute
                        2 => { set_mic_mute(false); } // PTM release → unmute
                        _ => {}
                    }
                }
            }
            LRESULT(0)
        }

        WM_DESTROY => {
            PostQuitMessage(0);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe fn show_settings_dialog() {
    sync_mute_state();
    // Snapshot current state so Discard can restore it exactly
    SETTINGS_SNAPSHOT.with(|s| *s.borrow_mut() = Some(STATE.with(|st| st.borrow().clone())));
    let instance = GetModuleHandleW(None).unwrap();
    let class_name = w!("SettingsWindowClass");
    let wc = WNDCLASSW {
        lpfnWndProc: Some(settings_wnd_proc),
        hInstance: instance.into(),
        lpszClassName: class_name,
        hbrBackground: HBRUSH((COLOR_BTNFACE.0 + 1) as *mut core::ffi::c_void),
        ..Default::default()
    };
    RegisterClassW(&wc);

    if let Ok(settings_hwnd) = CreateWindowExW(
        WINDOW_EX_STYLE::default(),
        class_name,
        w!("MicKey Settings"),
        WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_VISIBLE,
        CW_USEDEFAULT, CW_USEDEFAULT, 520, 520,
        None, None, instance, None,
    ) {
        if let Some(hicon_big) = hicon_from_bytes(ICO_MICKEY, 32) {
            let _ = SendMessageW(settings_hwnd, WM_SETICON, WPARAM(1), LPARAM(hicon_big.0 as isize));
        }
        if let Some(hicon_small) = hicon_from_bytes(ICO_MICKEY, 16) {
            let _ = SendMessageW(settings_hwnd, WM_SETICON, WPARAM(0), LPARAM(hicon_small.0 as isize));
        }
        SETTINGS_HWND.with(|s| *s.borrow_mut() = settings_hwnd);
    }
}

unsafe fn apply_modern_font(hwnd_control: HWND) {
    let mut ncm = NONCLIENTMETRICSW {
        cbSize: std::mem::size_of::<NONCLIENTMETRICSW>() as u32,
        ..Default::default()
    };
    
    if SystemParametersInfoW(
        SPI_GETNONCLIENTMETRICS,
        ncm.cbSize,
        Some(&mut ncm as *mut _ as *mut std::ffi::c_void),
        SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
    ).is_ok() {
        let h_font = CreateFontIndirectW(&ncm.lfMessageFont);
        if !h_font.is_invalid() {
            let _ = SendMessageW(hwnd_control, WM_SETFONT, WPARAM(h_font.0 as usize), LPARAM(1));
        }
    }
}

unsafe fn create_overlay_window() -> Result<HWND> {
    let instance = GetModuleHandleW(None).unwrap();
    let class_name = w!("MicKeyOverlayClass");

    let wc = WNDCLASSW {
        lpfnWndProc: Some(overlay_wnd_proc),
        hInstance: instance.into(),
        lpszClassName: class_name,
        hbrBackground: HBRUSH(GetStockObject(HOLLOW_BRUSH).0),
        ..Default::default()
    };
    RegisterClassW(&wc);

    let (x, y, size, should_show) = STATE.with(|s| {
        let state = s.borrow();
        let primary_w = GetSystemMetrics(SM_CXSCREEN);
        let primary_h = GetSystemMetrics(SM_CYSCREEN);

        let win_x = match state.glyph_corner {
            0 | 2 => state.offset_x,
            _ => primary_w - state.glyph_size - state.offset_x,
        };
        let win_y = match state.glyph_corner {
            0 | 1 => state.offset_y,
            _ => primary_h - state.glyph_size - state.offset_y,
        };
        (win_x, win_y, state.glyph_size, state.show_overlay)
    });

    let style = if should_show { WS_POPUP | WS_VISIBLE } else { WS_POPUP };

    let hwnd = CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TRANSPARENT | WS_EX_NOACTIVATE,
        class_name,
        w!("MicKey Overlay"),
        style,
        x, y, size, size,
        None, None, instance, None,
    )?;

    STATE.with(|s| {
        let state = s.borrow();
        let opacity_pct = match state.mute_state {
            MuteState::Muted   => state.opacity_muted,
            MuteState::Unmuted => state.opacity_unmuted,
            MuteState::Error   => state.opacity_error,
        };
        let alpha = (opacity_pct as u32 * 255 / 100) as u8;
        let _ = SetLayeredWindowAttributes(hwnd, COLORREF(0), alpha, LWA_ALPHA);
    });

    Ok(hwnd)
}

unsafe extern "system" fn overlay_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_PAINT => {
            let mut ps = PAINTSTRUCT::default();
            let hdc = BeginPaint(hwnd, &mut ps);

            let (shape, c, size) = STATE.with(|s| {
                let st = s.borrow();
                let c = match st.mute_state {
                    MuteState::Muted   => st.color_muted,
                    MuteState::Unmuted => st.color_unmuted,
                    MuteState::Error   => st.color_error,
                };
                (st.glyph_shape, c, st.glyph_size)
            });

            if shape >= 2 {
                // Square: solid fill via normal GDI paint
                let brush = CreateSolidBrush(rgb_to_colorref(c));
                let rect = RECT { left: 0, top: 0, right: size, bottom: size };
                FillRect(hdc, &rect, brush);
                let _ = DeleteObject(brush);
            }

            let _ = EndPaint(hwnd, &ps);
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}

unsafe extern "system" fn edit_subclass_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM, _uid: usize, dw_ref_data: usize) -> LRESULT {
    if msg == WM_LBUTTONDOWN {
        if GetFocus() != hwnd {
            let _ = SetFocus(hwnd);
            let _ = SendMessageW(hwnd, 0x00B1u32, WPARAM(0), LPARAM(-1));
            return LRESULT(0); // eat the click so it doesn't position the cursor
        }
    }
    // For signed offset fields (dw_ref_data == 1): allow digits, '-', backspace only
    if msg == WM_CHAR && dw_ref_data == 1 {
        let ch = wparam.0 as u8;
        let is_digit     = ch.is_ascii_digit();
        let is_backspace = ch == 8;
        let is_minus     = ch == b'-';
        if !is_digit && !is_backspace && !is_minus {
            return LRESULT(0); // block everything else
        }
        if is_minus {
            let sel = SendMessageW(hwnd, 0x00B0u32, WPARAM(0), LPARAM(0)); // EM_GETSEL
            let sel_start = (sel.0 & 0xFFFF) as usize;
            let sel_end   = ((sel.0 >> 16) & 0xFFFF) as usize;
            let mut buf = [0u16; 16];
            GetWindowTextW(hwnd, &mut buf);
            let text = String::from_utf16_lossy(&buf);
            let text = text.trim_matches('\0');
            let text_len = text.len();
            let already_negative = text.starts_with('-');
            // Allow '-' if cursor/selection starts at 0 AND either:
            // - there is no existing '-', OR
            // - the selection covers the whole field (replacing everything)
            let full_selection = sel_start == 0 && sel_end >= text_len && text_len > 0;
            if sel_start != 0 || (already_negative && !full_selection) {
                return LRESULT(0);
            }
        }
    }
    DefSubclassProc(hwnd, msg, wparam, lparam)
}

// Resolves the currently selected capture device and returns its volume endpoint.
// Returns None and sets Error state if the device is unavailable.
unsafe fn get_audio_endpoint() -> Option<IAudioEndpointVolume> {
    let enumerator: Result<IMMDeviceEnumerator> = CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL);
    let Ok(enumerator) = enumerator else { return None; };
    let device_id = STATE.with(|s| s.borrow().device_override.clone());
    let device = if device_id.is_empty() {
        enumerator.GetDefaultAudioEndpoint(eCapture, eCommunications).ok()?
    } else {
        let id_wide = to_wide(&device_id);
        enumerator.GetDevice(PCWSTR(id_wide.as_ptr())).ok()?
    };
    device.Activate(CLSCTX_ALL, None).ok()
}

// Sets the hardware mute state on the selected (or default) capture device.
unsafe fn set_mic_mute(muted: bool) {
    let Some(endpoint) = get_audio_endpoint() else {
        STATE.with(|s| s.borrow_mut().mute_state = MuteState::Error);
        redraw_overlay();
        return;
    };
    if endpoint.SetMute(muted, std::ptr::null()).is_err() {
        STATE.with(|s| s.borrow_mut().mute_state = MuteState::Error);
        redraw_overlay();
    }
    // Do NOT update mute_state or redraw here on success. The IAudioEndpointVolumeCallback will fire OnNotify, which posts M_MUTE_CHANGED, which updates state and redraws. That is the single source of truth for color, including external mute changes.
}

// Queries the current hardware mute state without changing it.
// Sets MuteState::Error if the device is unavailable.
unsafe fn sync_mute_state() {
    let Some(endpoint) = get_audio_endpoint() else {
        STATE.with(|s| s.borrow_mut().mute_state = MuteState::Error);
        redraw_overlay();
        return;
    };
    match endpoint.GetMute() {
        Ok(muted) => {
            STATE.with(|s| {
                s.borrow_mut().mute_state = if muted.as_bool() {
                    MuteState::Muted
                } else {
                    MuteState::Unmuted
                };
            });
            redraw_overlay();
        }
        Err(_) => {
            STATE.with(|s| s.borrow_mut().mute_state = MuteState::Error);
            redraw_overlay();
        }
    }
}

// Parses a hotkey string like "Ctrl+Alt+M" into (modifiers_u32, vk_u32).
// Returns None if the string is empty or unparseable.
fn parse_hotkey_str(s: &str) -> Option<(u32, u32)> {
    if s.is_empty() { return None; }
    let parts: Vec<&str> = s.split('+').collect();
    if parts.is_empty() { return None; }

    let mut mods: u32 = 0;
    let key_name = parts.last()?;

    for part in &parts[..parts.len() - 1] {
        match *part {
            "Ctrl"  => mods |= 0x0002, // MOD_CONTROL
            "Shift" => mods |= 0x0004, // MOD_SHIFT
            "Alt"   => mods |= 0x0001, // MOD_ALT
            "Win"   => mods |= 0x0008, // MOD_WIN
            _ => {}
        }
    }

    // Map the key name back to a VK using VkKeyScanW for single chars, and a lookup table for named keys.
    let vk: u32 = match *key_name {
        "F1"  => 0x70, "F2"  => 0x71, "F3"  => 0x72, "F4"  => 0x73,
        "F5"  => 0x74, "F6"  => 0x75, "F7"  => 0x76, "F8"  => 0x77,
        "F9"  => 0x78, "F10" => 0x79, "F11" => 0x7A, "F12" => 0x7B,
        "Insert" => 0x2D, "Delete" => 0x2E, "Home" => 0x24,
        "End"    => 0x23, "Page Up" => 0x21, "Page Down" => 0x22,
        "Left"   => 0x25, "Up"   => 0x26, "Right" => 0x27, "Down" => 0x28,
        "Space"  => 0x20, "Tab"  => 0x09, "Enter" => 0x0D,
        "Backspace" => 0x08, "Escape" => 0x1B,
        "Num Lock" => 0x90, "Scroll Lock" => 0x91, "Pause" => 0x13,
        k if k.len() == 1 => {
            let ch = k.chars().next().unwrap().to_uppercase().next().unwrap();
            unsafe { (VkKeyScanW(ch as u16) & 0xFF) as u32 }
        }
        _ => return None,
    };

    Some((mods, vk))
}

// Registers (or re-registers) the global hotkey from current AppState.
// Call this after settings are saved or on startup.
unsafe fn register_app_hotkey() {
    let main_hwnd = STATE.with(|s| s.borrow().main_hwnd);

    let _ = UnregisterHotKey(main_hwnd, 1);

    let hotkey_str = STATE.with(|s| s.borrow().hotkey_str.clone());

    if let Some((mods, vk)) = parse_hotkey_str(&hotkey_str) {
        let _ = RegisterHotKey(main_hwnd, 1, HOT_KEY_MODIFIERS(mods | 0x4000), vk);
    }
}

#[inline(always)]
fn rgb_to_colorref(c: [u8; 3]) -> COLORREF {
    COLORREF(c[0] as u32 | ((c[1] as u32) << 8) | ((c[2] as u32) << 16))
}

fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

fn fmt_color_hex(c: [u8; 3]) -> String {
    format!("{:02X}{:02X}{:02X}", c[0], c[1], c[2])
}

fn parse_color_hex(s: &str) -> Option<[u8; 3]> {
    let s = s.trim().trim_start_matches('#');
    if s.len() == 6 {
        let r = u8::from_str_radix(&s[0..2], 16).ok()?;
        let g = u8::from_str_radix(&s[2..4], 16).ok()?;
        let b = u8::from_str_radix(&s[4..6], 16).ok()?;
        Some([r, g, b])
    } else {
        None
    }
}

// Opens the native Windows ChooseColor dialog and returns [r,g,b] if accepted.
// Alpha is handled separately via the opacity field.
unsafe fn show_native_color_picker(parent: HWND, initial: [u8; 3]) -> Option<[u8; 3]> {
    // ChooseColor requires a static array of 16 custom colors; Mutex ensures safe interior mutability without runtime cost on the single UI thread.
    static CUSTOM_COLORS: std::sync::Mutex<[COLORREF; 16]> =
        std::sync::Mutex::new([COLORREF(0x00FFFFFF); 16]);

    let mut colors = CUSTOM_COLORS.lock().unwrap();
    let mut cc = CHOOSECOLORW {
        lStructSize: std::mem::size_of::<CHOOSECOLORW>() as u32,
        hwndOwner: parent,
        rgbResult: rgb_to_colorref(initial),
        lpCustColors: colors.as_mut_ptr(),
        Flags: CC_FULLOPEN | CC_RGBINIT,
        ..Default::default()
    };

    if ChooseColorW(&mut cc).as_bool() {
        let rgb = cc.rgbResult.0;
        Some([(rgb & 0xFF) as u8, ((rgb >> 8) & 0xFF) as u8, ((rgb >> 16) & 0xFF) as u8])
    } else {
        None
    }
}

fn get_modifiers() -> String {
    let mut m = Vec::new();
    unsafe {
        if GetKeyState(VK_CONTROL.0 as i32) < 0 { m.push("Ctrl"); }
        if GetKeyState(VK_SHIFT.0 as i32) < 0 { m.push("Shift"); }
        if GetKeyState(VK_MENU.0 as i32) < 0 { m.push("Alt"); }
        if GetKeyState(VK_LWIN.0 as i32) < 0 || GetKeyState(VK_RWIN.0 as i32) < 0 { m.push("Win"); }
    }
    m.join("+")
}

unsafe fn build_hotkey_string(vk: u32) -> String {
    // Get the key name using the Windows API
    let mut name = [0u16; 32];
    let scan_code = MapVirtualKeyW(vk, MAPVK_VK_TO_VSC);
    // Extended keys (nav cluster, arrows) need bit 24 set so GetKeyNameTextW distinguishes them from their numpad twins.
    let is_extended = matches!(vk,
        0x21..=0x28 |  // PgUp, PgDn, End, Home, arrow keys
        0x2D | 0x2E |  // Insert, Delete
        0x2F |         // Help (rarely used but extended)
        0x6F           // Numpad Divide (also extended)
    );
    let lparam_val = if is_extended {
        ((scan_code << 16) | (1 << 24)) as i32
    } else {
        (scan_code << 16) as i32
    };
    GetKeyNameTextW(lparam_val, &mut name);
    let key_str = String::from_utf16_lossy(&name).trim_matches(char::from(0)).to_string();

    let mods = get_modifiers();
    if mods.is_empty() {
        key_str
    } else {
        format!("{}+{}", mods, key_str)
    }
}

// Reads all settings controls into STATE and redraws; called for live preview and on Save.
unsafe fn apply_settings_from_controls(hwnd: HWND) {
    let h_overlay = match GetDlgItem(hwnd, ID_CTRL_OVERLAY_CHECK as i32) { Ok(h) => h, Err(_) => return };
    let show_overlay = SendMessageW(h_overlay, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == 1;

    let h_tray = match GetDlgItem(hwnd, ID_CTRL_TRAY_CHECK as i32) { Ok(h) => h, Err(_) => return };
    let show_tray_icon = SendMessageW(h_tray, BM_GETCHECK, WPARAM(0), LPARAM(0)).0 == 1;

    let h_corner = match GetDlgItem(hwnd, ID_CTRL_CORNER_COMBO as i32) { Ok(h) => h, Err(_) => return };
    let corner_sel = SendMessageW(h_corner, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 as u32;

    let h_shape = match GetDlgItem(hwnd, ID_CTRL_SHAPE_COMBO as i32) { Ok(h) => h, Err(_) => return };
    let shape_sel = SendMessageW(h_shape, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 as u32;

    let h_mode = match GetDlgItem(hwnd, ID_CTRL_HOTKEY_MODE_COMBO as i32) { Ok(h) => h, Err(_) => return };
    let mode_sel = SendMessageW(h_mode, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 as u32;

    let mut read_buf = [0u16; 16];

    let h_size = match GetDlgItem(hwnd, ID_CTRL_SIZE_EDIT as i32) { Ok(h) => h, Err(_) => return };
    let _ = GetWindowTextW(h_size, &mut read_buf);
    let size_val = String::from_utf16_lossy(&read_buf).trim_matches('\0').trim().parse::<i32>().unwrap_or(32).clamp(1, 64);

    let h_x = match GetDlgItem(hwnd, ID_CTRL_OFFSET_X_EDIT as i32) { Ok(h) => h, Err(_) => return };
    read_buf = [0u16; 16];
    let _ = GetWindowTextW(h_x, &mut read_buf);
    let x_str = String::from_utf16_lossy(&read_buf).trim_matches('\0').trim().to_string();
    let x_val = if x_str.is_empty() || x_str == "-" { 0 } else { x_str.parse::<i32>().unwrap_or(0).clamp(-9999, 9999) };

    let h_y = match GetDlgItem(hwnd, ID_CTRL_OFFSET_Y_EDIT as i32) { Ok(h) => h, Err(_) => return };
    read_buf = [0u16; 16];
    let _ = GetWindowTextW(h_y, &mut read_buf);
    let y_str = String::from_utf16_lossy(&read_buf).trim_matches('\0').trim().to_string();
    let y_val = if y_str.is_empty() || y_str == "-" { 0 } else { y_str.parse::<i32>().unwrap_or(0).clamp(-9999, 9999) };

    let read_opacity = |id: isize| -> u8 {
        let mut buf = [0u16; 8];
        if let Ok(h) = GetDlgItem(hwnd, id as i32) {
            let _ = GetWindowTextW(h, &mut buf);
            String::from_utf16_lossy(&buf).trim_matches('\0').trim()
                .parse::<u8>().unwrap_or(100).min(100)
        } else { 100 }
    };
    let alpha_muted   = read_opacity(ID_CTRL_OPACITY_MUTED);
    let alpha_unmuted = read_opacity(ID_CTRL_OPACITY_UNMUTED);
    let alpha_error   = read_opacity(ID_CTRL_OPACITY_ERROR);

    let h_device = match GetDlgItem(hwnd, ID_CTRL_DEVICE_COMBO as i32) { Ok(h) => h, Err(_) => return };
    let device_sel = SendMessageW(h_device, CB_GETCURSEL, WPARAM(0), LPARAM(0)).0 as usize;
    let device_override = if device_sel == 0 {
        String::new()
    } else {
        DEVICE_LIST.with(|d| d.borrow().get(device_sel - 1).cloned().unwrap_or_default())
    };

    STATE.with(|s| {
        let mut state = s.borrow_mut();
        state.show_overlay      = show_overlay;
        state.show_tray_icon    = show_tray_icon;
        state.glyph_size        = size_val;
        state.glyph_corner      = corner_sel;
        state.offset_x          = x_val;
        state.offset_y          = y_val;
        state.glyph_shape       = shape_sel;
        // hotkey_str not written here — only committed on Save to avoid hook limbo
        state.hotkey_mode       = mode_sel;
        state.opacity_muted     = alpha_muted;
        state.opacity_unmuted   = alpha_unmuted;
        state.opacity_error     = alpha_error;
        state.device_override = device_override;
    });

    redraw_overlay();
    sync_visibilities();
}

// Settings window procedure
unsafe extern "system" fn settings_wnd_proc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    match msg {
        WM_CREATE => {
            let lbl_style  = WINDOW_STYLE(WS_VISIBLE.0 | WS_CHILD.0 | 0x00000002u32); // SS_RIGHT
            let static_style = WS_VISIBLE | WS_CHILD; // left-aligned, no tabstop
            let combo_style  = WINDOW_STYLE(WS_VISIBLE.0 | WS_CHILD.0 | CBS_DROPDOWNLIST as u32);
            let check_style  = WINDOW_STYLE(WS_VISIBLE.0 | WS_CHILD.0 | BS_AUTOCHECKBOX as u32);
            let edit_num     = WINDOW_STYLE(WS_VISIBLE.0 | WS_CHILD.0 | ES_NUMBER as u32);
            let edit_signed  = WINDOW_STYLE(WS_VISIBLE.0 | WS_CHILD.0); // allows '-' for offset fields
            let divider_style = WINDOW_STYLE(WS_VISIBLE.0 | WS_CHILD.0 | 0x00000010u32); // SS_ETCHEDHORZ
            let color_btn_style = WINDOW_STYLE(WS_VISIBLE.0 | WS_CHILD.0 | BS_OWNERDRAW as u32);

            let current = STATE.with(|s| s.borrow().clone());

            // Layout constants
            let lw = 490i32;   // usable width
            let lx = 12i32;    // left margin for labels
            let cx = 150i32;   // left edge of controls
            let cw = lw - cx - lx; // control width to right edge
            let mut y = 14i32;
            let row = 28i32;   // row height
            let gap = 8i32;    // gap before divider

            // Section 1: Input Device 
            if let Ok(ctrl) = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("STATIC"), w!("Input Device"), lbl_style, lx, y + 3, cx - lx - 8, 16, hwnd, HMENU::default(), None, None) { apply_modern_font(ctrl); }
            if let Ok(ctrl) = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("COMBOBOX"), None, combo_style, cx, y, cw, 200, hwnd, HMENU(ID_CTRL_DEVICE_COMBO as *mut core::ffi::c_void), None, None) {
                apply_modern_font(ctrl);
                let (default_mic, mic_list) = enumerate_microphones();
                let default_string = match default_mic {
                    Some(name) => format!("Default ({})", name),
                    None => "Default Communication Device".to_string(),
                };
                let mut default_w = to_wide(&default_string);
                let _ = SendMessageW(ctrl, CB_ADDSTRING, WPARAM(0), LPARAM(default_w.as_mut_ptr() as _));
                let _ = SendMessageW(ctrl, CB_SETITEMDATA, WPARAM(0), LPARAM(0));
                let current_device_id = STATE.with(|s| s.borrow().device_override.clone());
                let mut sel_idx: usize = 0;
                for (i, mic) in mic_list.iter().enumerate() {
                    let mut mic_w = to_wide(&mic.name);
                    let _ = SendMessageW(ctrl, CB_ADDSTRING, WPARAM(0), LPARAM(mic_w.as_mut_ptr() as _));
                    if mic.id == current_device_id { sel_idx = i + 1; }
                }
                DEVICE_LIST.with(|d| {
                    let mut list = d.borrow_mut();
                    list.clear();
                    list.extend(mic_list.iter().map(|m| m.id.clone()));
                });
                let _ = SendMessageW(ctrl, CB_SETCURSEL, WPARAM(sel_idx), LPARAM(0));
            }
            y += row;

            // Show Overlay + Show Tray Icon on same row
            let check_state_overlay = if current.show_overlay { BST_CHECKED } else { BST_UNCHECKED };
            if let Ok(ctrl) = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("BUTTON"), w!("Show Overlay"), check_style, cx, y, 130, 22, hwnd, HMENU(ID_CTRL_OVERLAY_CHECK as *mut core::ffi::c_void), None, None) {
                apply_modern_font(ctrl);
                let _ = SendMessageW(ctrl, BM_SETCHECK, WPARAM(check_state_overlay.0 as usize), LPARAM(0));
            }
            let check_state_tray = if current.show_tray_icon { BST_CHECKED } else { BST_UNCHECKED };
            if let Ok(ctrl) = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("BUTTON"), w!("Show Tray Icon"), check_style, cx + 145, y, 130, 22, hwnd, HMENU(ID_CTRL_TRAY_CHECK as *mut core::ffi::c_void), None, None) {
                apply_modern_font(ctrl);
                let _ = SendMessageW(ctrl, BM_SETCHECK, WPARAM(check_state_tray.0 as usize), LPARAM(0));
            }
            y += row + gap;

            // Divider
            let _ = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("STATIC"), None, divider_style, lx, y, lw, 2, hwnd, HMENU::default(), None, None);
            y += 10;

            // Section 2: Hotkey
            if let Ok(ctrl) = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("STATIC"), w!("Global Hotkey"), lbl_style, lx, y + 3, cx - lx - 8, 16, hwnd, HMENU::default(), None, None) { apply_modern_font(ctrl); }
            let mut hotkey_w = to_wide(&current.hotkey_str);
            if let Ok(ctrl) = CreateWindowExW(WS_EX_CLIENTEDGE, w!("EDIT"), PCWSTR(hotkey_w.as_mut_ptr()),
                WINDOW_STYLE(WS_VISIBLE.0 | WS_CHILD.0 | ES_READONLY as u32),
                cx, y, cw - 70, 22, hwnd, HMENU(ID_CTRL_HOTKEY_DISPLAY as *mut core::ffi::c_void), None, None) { apply_modern_font(ctrl); }
            if let Ok(ctrl) = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("BUTTON"), w!("Record"),
                WINDOW_STYLE(WS_VISIBLE.0 | WS_CHILD.0 | BS_PUSHBUTTON as u32),
                cx + cw - 65, y, 65, 26, hwnd, HMENU(ID_CTRL_HOTKEY_BTN as *mut core::ffi::c_void), None, None) { apply_modern_font(ctrl); }
            y += row;

            if let Ok(ctrl) = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("STATIC"), w!("Hotkey Mode"), lbl_style, lx, y + 3, cx - lx - 8, 16, hwnd, HMENU::default(), None, None) { apply_modern_font(ctrl); }
            if let Ok(ctrl) = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("COMBOBOX"), None, combo_style, cx, y, cw, 80, hwnd, HMENU(ID_CTRL_HOTKEY_MODE_COMBO as *mut core::ffi::c_void), None, None) {
                apply_modern_font(ctrl);
                let _ = SendMessageW(ctrl, CB_ADDSTRING, WPARAM(0), LPARAM(w!("Toggle").as_ptr() as _));
                let _ = SendMessageW(ctrl, CB_ADDSTRING, WPARAM(0), LPARAM(w!("Push-to-Talk").as_ptr() as _));
                let _ = SendMessageW(ctrl, CB_ADDSTRING, WPARAM(0), LPARAM(w!("Push-to-Mute").as_ptr() as _));
                let _ = SendMessageW(ctrl, CB_SETCURSEL, WPARAM(current.hotkey_mode as usize), LPARAM(0));
            }
            y += row + gap;

            // Divider
            let _ = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("STATIC"), None, divider_style, lx, y, lw, 2, hwnd, HMENU::default(), None, None);
            y += 10;

            // Section 3: Overlay Appearance
            if let Ok(ctrl) = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("STATIC"), w!("Size (px, max 64)"), lbl_style, lx, y + 3, cx - lx - 8, 16, hwnd, HMENU::default(), None, None) { apply_modern_font(ctrl); }
            let mut size_str = to_wide(&current.glyph_size.to_string());
            if let Ok(ctrl) = CreateWindowExW(WS_EX_CLIENTEDGE, w!("EDIT"), PCWSTR(size_str.as_mut_ptr()), edit_num, cx, y, cw - 20, 22, hwnd, HMENU(ID_CTRL_SIZE_EDIT as *mut core::ffi::c_void), None, None) { apply_modern_font(ctrl); }
            if let Ok(spin) = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("msctls_updown32"), None,
                WINDOW_STYLE(WS_VISIBLE.0 | WS_CHILD.0 | UDS_AUTOBUDDY as u32 | UDS_SETBUDDYINT as u32 | UDS_ALIGNRIGHT as u32 | UDS_ARROWKEYS as u32),
                0, 0, 0, 0, hwnd, HMENU(ID_CTRL_SPIN_SIZE as *mut core::ffi::c_void), None, None) {
                let _ = SendMessageW(spin, 0x0465u32, WPARAM(0), LPARAM((64i32 | (1i32 << 16)) as isize));
                let _ = SendMessageW(spin, 0x0467u32, WPARAM(0), LPARAM(current.glyph_size as isize));
            }
            y += row;

            if let Ok(ctrl) = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("STATIC"), w!("Corner"), lbl_style, lx, y + 3, cx - lx - 8, 16, hwnd, HMENU::default(), None, None) { apply_modern_font(ctrl); }
            if let Ok(ctrl) = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("COMBOBOX"), None, combo_style, cx, y, cw, 80, hwnd, HMENU(ID_CTRL_CORNER_COMBO as *mut core::ffi::c_void), None, None) {
                apply_modern_font(ctrl);
                let _ = SendMessageW(ctrl, CB_ADDSTRING, WPARAM(0), LPARAM(w!("Top-Left").as_ptr() as _));
                let _ = SendMessageW(ctrl, CB_ADDSTRING, WPARAM(0), LPARAM(w!("Top-Right").as_ptr() as _));
                let _ = SendMessageW(ctrl, CB_ADDSTRING, WPARAM(0), LPARAM(w!("Bottom-Left").as_ptr() as _));
                let _ = SendMessageW(ctrl, CB_ADDSTRING, WPARAM(0), LPARAM(w!("Bottom-Right").as_ptr() as _));
                let _ = SendMessageW(ctrl, CB_SETCURSEL, WPARAM(current.glyph_corner as usize), LPARAM(0));
            }
            y += row;

            if let Ok(ctrl) = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("STATIC"), w!("Offset X (px)"), lbl_style, lx, y + 3, cx - lx - 8, 16, hwnd, HMENU::default(), None, None) { apply_modern_font(ctrl); }
            let mut x_str = to_wide(&current.offset_x.to_string());
            if let Ok(ctrl) = CreateWindowExW(WS_EX_CLIENTEDGE, w!("EDIT"), PCWSTR(x_str.as_mut_ptr()), edit_signed, cx, y, cw - 20, 22, hwnd, HMENU(ID_CTRL_OFFSET_X_EDIT as *mut core::ffi::c_void), None, None) { apply_modern_font(ctrl); }
            if let Ok(spin) = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("msctls_updown32"), None,
                WINDOW_STYLE(WS_VISIBLE.0 | WS_CHILD.0 | UDS_AUTOBUDDY as u32 | UDS_SETBUDDYINT as u32 | UDS_ALIGNRIGHT as u32 | UDS_ARROWKEYS as u32),
                0, 0, 0, 0, hwnd, HMENU(ID_CTRL_SPIN_X as *mut core::ffi::c_void), None, None) {
                let _ = SendMessageW(spin, 0x0465u32, WPARAM(0), LPARAM((9999i32 | (-9999i32 << 16)) as isize));
                let _ = SendMessageW(spin, 0x0467u32, WPARAM(0), LPARAM(current.offset_x as isize));
            }
            y += row;

            if let Ok(ctrl) = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("STATIC"), w!("Offset Y (px)"), lbl_style, lx, y + 3, cx - lx - 8, 16, hwnd, HMENU::default(), None, None) { apply_modern_font(ctrl); }
            let mut y_str = to_wide(&current.offset_y.to_string());
            if let Ok(ctrl) = CreateWindowExW(WS_EX_CLIENTEDGE, w!("EDIT"), PCWSTR(y_str.as_mut_ptr()), edit_signed, cx, y, cw - 20, 22, hwnd, HMENU(ID_CTRL_OFFSET_Y_EDIT as *mut core::ffi::c_void), None, None) { apply_modern_font(ctrl); }
            if let Ok(spin) = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("msctls_updown32"), None,
                WINDOW_STYLE(WS_VISIBLE.0 | WS_CHILD.0 | UDS_AUTOBUDDY as u32 | UDS_SETBUDDYINT as u32 | UDS_ALIGNRIGHT as u32 | UDS_ARROWKEYS as u32),
                0, 0, 0, 0, hwnd, HMENU(ID_CTRL_SPIN_Y as *mut core::ffi::c_void), None, None) {
                let _ = SendMessageW(spin, 0x0465u32, WPARAM(0), LPARAM((9999i32 | (-9999i32 << 16)) as isize));
                let _ = SendMessageW(spin, 0x0467u32, WPARAM(0), LPARAM(current.offset_y as isize));
            }
            y += row + gap;

            // Divider
            let _ = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("STATIC"), None, divider_style, lx, y, lw, 2, hwnd, HMENU::default(), None, None);
            y += 10;

            // Section 4: Shape & Colors
            if let Ok(ctrl) = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("STATIC"), w!("Shape"), lbl_style, lx, y + 3, cx - lx - 8, 16, hwnd, HMENU::default(), None, None) { apply_modern_font(ctrl); }
            if let Ok(ctrl) = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("COMBOBOX"), None, combo_style, cx, y, cw, 80, hwnd, HMENU(ID_CTRL_SHAPE_COMBO as *mut core::ffi::c_void), None, None) {
                apply_modern_font(ctrl);
                let _ = SendMessageW(ctrl, CB_ADDSTRING, WPARAM(0), LPARAM(w!("MicKey Icon").as_ptr() as _));
                let _ = SendMessageW(ctrl, CB_ADDSTRING, WPARAM(0), LPARAM(w!("Circle").as_ptr() as _));
                let _ = SendMessageW(ctrl, CB_ADDSTRING, WPARAM(0), LPARAM(w!("Square").as_ptr() as _));
                let _ = SendMessageW(ctrl, CB_SETCURSEL, WPARAM(current.glyph_shape as usize), LPARAM(0));
            }
            y += row;

            // Color rows: label | swatch | slider | % edit
            let color_rows: [(&str, isize, isize, isize, [u8; 3]); 3] = [
                ("Color: Active", ID_CTRL_COLOR_UNMUTED, ID_CTRL_OPACITY_UNMUTED, ID_CTRL_SLIDER_UNMUTED, current.color_unmuted),
                ("Color: Muted",  ID_CTRL_COLOR_MUTED,   ID_CTRL_OPACITY_MUTED,   ID_CTRL_SLIDER_MUTED,   current.color_muted),
                ("Color: Error",  ID_CTRL_COLOR_ERROR,   ID_CTRL_OPACITY_ERROR,   ID_CTRL_SLIDER_ERROR,   current.color_error),
            ];

            for (i, (_label, swatch_id, opacity_id, slider_id, _color)) in color_rows.iter().enumerate() {
                let label_text = color_rows[i].0;
                let label_wide = to_wide(label_text);
                if let Ok(ctrl) = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("STATIC"),
                    PCWSTR(label_wide.as_ptr()),
                    lbl_style, lx, y + 6, cx - lx - 8, 16,
                    hwnd, HMENU::default(), None, None) { apply_modern_font(ctrl); }

                // Swatch button
                let _ = CreateWindowExW(WS_EX_CLIENTEDGE, w!("BUTTON"), None,
                    color_btn_style, cx, y, 36, 26,
                    hwnd, HMENU(*swatch_id as *mut core::ffi::c_void), None, None);

                // Slider
                let opacity_pct = *[current.opacity_unmuted, current.opacity_muted, current.opacity_error].get(i).unwrap_or(&100) as u32;
                let slider_style = WINDOW_STYLE(WS_VISIBLE.0 | WS_CHILD.0 | TBS_HORZ as u32 | TBS_NOTICKS as u32);
                if let Ok(ctrl) = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("msctls_trackbar32"),
                    None, slider_style, cx + 44, y, 220, 26,
                    hwnd, HMENU(*slider_id as *mut core::ffi::c_void), None, None) {
                    let _ = SendMessageW(ctrl, TBM_SETRANGE, WPARAM(1), LPARAM(100 << 16));
                    let _ = SendMessageW(ctrl, TBM_SETPOS, WPARAM(1), LPARAM(opacity_pct as isize));
                }

                // % edit
                let mut op_str: Vec<u16> = opacity_pct.to_string().encode_utf16().chain(std::iter::once(0)).collect();
                if let Ok(ctrl) = CreateWindowExW(WS_EX_CLIENTEDGE, w!("EDIT"),
                    PCWSTR(op_str.as_mut_ptr()), edit_num, cx + 272, y + 2, 40, 22,
                    hwnd, HMENU(*opacity_id as *mut core::ffi::c_void), None, None) { apply_modern_font(ctrl); }

                // % label
                if let Ok(ctrl) = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("STATIC"),
                    w!("%"), static_style, cx + 316, y + 5, 14, 16,
                    hwnd, HMENU::default(), None, None) { apply_modern_font(ctrl); }

                y += row;
            }
            y += gap;

            // Action Buttons
            if let Ok(ctrl) = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("BUTTON"), w!("Discard Changes"),
                WINDOW_STYLE(WS_VISIBLE.0 | WS_CHILD.0 | BS_PUSHBUTTON as u32),
                lw - 240, y, 115, 26, hwnd, HMENU(ID_CTRL_DISCARD_BTN as *mut core::ffi::c_void), None, None) { apply_modern_font(ctrl); }
            if let Ok(ctrl) = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("BUTTON"), w!("Save Changes"),
                WINDOW_STYLE(WS_VISIBLE.0 | WS_CHILD.0 | BS_DEFPUSHBUTTON as u32),
                lw - 118, y, 115, 26, hwnd, HMENU(ID_CTRL_SAVE_BTN as *mut core::ffi::c_void), None, None) { apply_modern_font(ctrl); }
            // Copyright notice — bottom-left, vertically centered with the buttons
            if let Ok(ctrl) = CreateWindowExW(WINDOW_EX_STYLE::default(), w!("STATIC"), w!("© 2026 ForFunGplDev (GPLv3) - v2026.6.1"),
                WINDOW_STYLE(WS_VISIBLE.0 | WS_CHILD.0 | 0x00000000u32), // SS_LEFT
                8, y + 5, lw - 248, 16, hwnd, HMENU(ID_CTRL_COPYRIGHT as *mut core::ffi::c_void), None, None) { apply_modern_font(ctrl); }
            y += 38;

            let right_edge = cx + cw;
            let spin_w = 20i32;
            for spin_id in [ID_CTRL_SPIN_SIZE, ID_CTRL_SPIN_X, ID_CTRL_SPIN_Y] {
                if let Ok(spin) = GetDlgItem(hwnd, spin_id as i32) {
                    let buddy_id = match spin_id {
                        ID_CTRL_SPIN_SIZE => GetDlgItem(hwnd, ID_CTRL_SIZE_EDIT as i32),
                        ID_CTRL_SPIN_X    => GetDlgItem(hwnd, ID_CTRL_OFFSET_X_EDIT as i32),
                        _                 => GetDlgItem(hwnd, ID_CTRL_OFFSET_Y_EDIT as i32),
                    };
                    if let Ok(buddy) = buddy_id {
                        // Get buddy position in client coords
                        let mut r = RECT::default();
                        let _ = GetWindowRect(buddy, &mut r);
                        let edit_h = r.bottom - r.top;
                        let edit_w = r.right - r.left;
                        let mut tl = POINT { x: r.left, y: r.top };
                        let _ = MapWindowPoints(HWND::default(), hwnd, std::slice::from_mut(&mut tl));

                        // Place spinner at right edge, same y and height as edit
                        let _ = SetWindowPos(spin, HWND::default(),
                            right_edge - spin_w, tl.y,
                            spin_w, edit_h,
                            SWP_NOZORDER | SWP_NOACTIVATE);

                        // Shrink edit so it ends just before the spinner
                        let _ = SetWindowPos(buddy, HWND::default(),
                            tl.x, tl.y,
                            right_edge - spin_w - tl.x, edit_h,
                            SWP_NOZORDER | SWP_NOACTIVATE);
                        let _ = edit_w;
                    }
                }
            }

            // Only stamp WS_TABSTOP on interactive controls (not STATIC)
            if let Ok(first_child) = GetWindow(hwnd, GW_CHILD) {
                let mut child = first_child;
                loop {
                    let mut cls = [0u16; 32];
                    GetClassNameW(child, &mut cls);
                    let cls_name = String::from_utf16_lossy(&cls);
                    let cls_str = cls_name.trim_matches('\0');
                    // Skip statics, spinners, and the read-only hotkey display
                    let ctrl_id = GetDlgCtrlID(child);
                    let is_hotkey_display = ctrl_id == ID_CTRL_HOTKEY_DISPLAY as i32;
                    if !cls_str.eq_ignore_ascii_case("static")
                        && !cls_str.eq_ignore_ascii_case("msctls_updown32")
                        && !is_hotkey_display {
                        let style = GetWindowLongW(child, GWL_STYLE);
                        SetWindowLongW(child, GWL_STYLE, style | WS_TABSTOP.0 as i32);
                    }
                    match GetWindow(child, GW_HWNDNEXT) {
                        Ok(next) => child = next,
                        Err(_) => break,
                    }
                }
            }

            if let Ok(first_child) = GetWindow(hwnd, GW_CHILD) {
                let mut child = first_child;
                let mut subclass_id: usize = 100;
                loop {
                    let mut cls = [0u16; 32];
                    GetClassNameW(child, &mut cls);
                    let cls_name = String::from_utf16_lossy(&cls);
                    if cls_name.trim_matches('\0').eq_ignore_ascii_case("edit") {
                        let ctrl_id = GetDlgCtrlID(child) as isize;
                        let ref_data: usize = if ctrl_id == ID_CTRL_OFFSET_X_EDIT || ctrl_id == ID_CTRL_OFFSET_Y_EDIT { 1 } else { 0 };
                        let _ = SetWindowSubclass(child, Some(edit_subclass_proc), subclass_id, ref_data);
                        subclass_id += 1;
                    }
                    match GetWindow(child, GW_HWNDNEXT) {
                        Ok(next) => child = next,
                        Err(_) => break,
                    }
                }
            }

            let _ = SetWindowPos(hwnd, HWND::default(), 0, 0, lw + 28, y + 38,
                SWP_NOMOVE | SWP_NOZORDER | SWP_NOACTIVATE);

            let _ = ShowWindow(hwnd, SW_SHOW);
            LRESULT(0)
        }

        WM_HSCROLL => {
            let hctl = HWND(lparam.0 as *mut core::ffi::c_void);
            let pairs = [
                (ID_CTRL_SLIDER_MUTED,   ID_CTRL_OPACITY_MUTED),
                (ID_CTRL_SLIDER_UNMUTED, ID_CTRL_OPACITY_UNMUTED),
                (ID_CTRL_SLIDER_ERROR,   ID_CTRL_OPACITY_ERROR),
            ];
            for (slider_id, edit_id) in pairs {
                if let Ok(h_slider) = GetDlgItem(hwnd, slider_id as i32) {
                    if h_slider == hctl {
                        let pos = SendMessageW(h_slider, TBM_GETPOS, WPARAM(0), LPARAM(0)).0 as u32;
                        if let Ok(h_edit) = GetDlgItem(hwnd, edit_id as i32) {
                            let mut val_w = to_wide(&pos.to_string());
                            let _ = SetWindowTextW(h_edit, PCWSTR(val_w.as_mut_ptr()));
                        }
                        break;
                    }
                }
            }
            apply_settings_from_controls(hwnd);
            LRESULT(0)
        }

        WM_DRAWITEM => {
            let dis = &*(lparam.0 as *const DRAWITEMSTRUCT);
            let color_id = dis.CtlID as isize;
            if matches!(color_id, ID_CTRL_COLOR_MUTED | ID_CTRL_COLOR_UNMUTED | ID_CTRL_COLOR_ERROR) {
                let c = STATE.with(|s| {
                    let st = s.borrow();
                    match color_id {
                        ID_CTRL_COLOR_MUTED   => st.color_muted,
                        ID_CTRL_COLOR_UNMUTED => st.color_unmuted,
                        _                     => st.color_error,
                    }
                });
                // Fill the color
                let brush = CreateSolidBrush(rgb_to_colorref(c));
                FillRect(dis.hDC, &dis.rcItem, brush);
                let _ = DeleteObject(brush);
                // Draw a 1px dark border so the swatch reads as a button, matching the sunken-edge look of the edit fields beside it
                let border_brush = CreateSolidBrush(COLORREF(GetSysColor(COLOR_BTNSHADOW)));
                FrameRect(dis.hDC, &dis.rcItem, border_brush);
                let _ = DeleteObject(border_brush);
            }
            LRESULT(1)
        }

        WM_KEYDOWN | WM_SYSKEYDOWN => {
            if RECORDING_HOTKEY.with(|r| *r.borrow()) {
                let vk = wparam.0 as u32;

                // Ignore bare modifier keys — keep listening until a main key is pressed
                if matches!(vk, 0x10..=0x12 | 0xA0..=0xA5 | 0x5B | 0x5C) {
                    return LRESULT(0);
                }

                let combo = build_hotkey_string(vk);

                STATE.with(|s| s.borrow_mut().hotkey_str = combo.clone());

                if let Ok(h_display) = GetDlgItem(hwnd, ID_CTRL_HOTKEY_DISPLAY as i32) {
                    let wide_str = to_wide(&combo);
                    let _ = SetWindowTextW(h_display, PCWSTR(wide_str.as_ptr()));
                }
                if let Ok(h_btn) = GetDlgItem(hwnd, ID_CTRL_HOTKEY_BTN as i32) {
                    let _ = SetWindowTextW(h_btn, w!("Record"));
                }

                let _ = KillTimer(hwnd, HOTKEY_RECORD_TIMER_ID);
                RECORDING_HOTKEY.with(|r| *r.borrow_mut() = false);
                register_app_hotkey();
                return LRESULT(0);
            }
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }

        WM_COMMAND => {
            let control_id = (wparam.0 & 0xFFFF) as isize;
            let notify_code = (wparam.0 >> 16) as u16;

            match control_id {
                ID_CTRL_HOTKEY_BTN if notify_code == BN_CLICKED as u16 => {
                    let already_recording = RECORDING_HOTKEY.with(|r| *r.borrow());
                    if already_recording {
                        let _ = KillTimer(hwnd, HOTKEY_RECORD_TIMER_ID);
                        RECORDING_HOTKEY.with(|r| *r.borrow_mut() = false);
                        register_app_hotkey();
                        if let Ok(h_btn) = GetDlgItem(hwnd, ID_CTRL_HOTKEY_BTN as i32) {
                            let _ = SetWindowTextW(h_btn, w!("Record"));
                        }
                    } else {
                        let main_hwnd = STATE.with(|s| s.borrow().main_hwnd);
                        let _ = UnregisterHotKey(main_hwnd, 1);
                        RECORDING_HOTKEY.with(|r| *r.borrow_mut() = true);
                        if let Ok(h_btn) = GetDlgItem(hwnd, ID_CTRL_HOTKEY_BTN as i32) {
                            let _ = SetWindowTextW(h_btn, w!("Stop"));
                        }
                        if let Ok(h_display) = GetDlgItem(hwnd, ID_CTRL_HOTKEY_DISPLAY as i32) {
                            let _ = SetWindowTextW(h_display, w!(""));
                        }
                        let _ = SetForegroundWindow(hwnd);
                        let _ = SetFocus(hwnd);
                        let _ = SetTimer(hwnd, HOTKEY_RECORD_TIMER_ID, 10_000, None);
                    }
                    LRESULT(0)
                }

                ID_CTRL_OPACITY_MUTED | ID_CTRL_OPACITY_UNMUTED | ID_CTRL_OPACITY_ERROR
                    if notify_code == EN_CHANGE as u16 =>
                {
                    let slider_id = match control_id {
                        ID_CTRL_OPACITY_MUTED   => ID_CTRL_SLIDER_MUTED,
                        ID_CTRL_OPACITY_UNMUTED => ID_CTRL_SLIDER_UNMUTED,
                        _                       => ID_CTRL_SLIDER_ERROR,
                    };
                    let mut buf = [0u16; 8];
                    if let Ok(h_edit) = GetDlgItem(hwnd, control_id as i32) {
                        let _ = GetWindowTextW(h_edit, &mut buf);
                        let pct = String::from_utf16_lossy(&buf).trim_matches('\0').trim()
                            .parse::<u32>().unwrap_or(100).min(100);
                        if let Ok(h_slider) = GetDlgItem(hwnd, slider_id as i32) {
                            let _ = SendMessageW(h_slider, TBM_SETPOS, WPARAM(1), LPARAM(pct as isize));
                        }
                    }
                    apply_settings_from_controls(hwnd);
                    LRESULT(0)
                }

                // Live preview for combos, checkboxes, and numeric edits
                ID_CTRL_DEVICE_COMBO if notify_code == CBN_SELCHANGE as u16 => {
                    apply_settings_from_controls(hwnd);
                    LRESULT(0)
                }
                ID_CTRL_CORNER_COMBO if notify_code == CBN_SELCHANGE as u16 => {
                    apply_settings_from_controls(hwnd);
                    LRESULT(0)
                }
                ID_CTRL_SHAPE_COMBO if notify_code == CBN_SELCHANGE as u16 => {
                    apply_settings_from_controls(hwnd);
                    LRESULT(0)
                }
                ID_CTRL_HOTKEY_MODE_COMBO if notify_code == CBN_SELCHANGE as u16 => {
                    apply_settings_from_controls(hwnd);
                    let mode = STATE.with(|s| s.borrow().hotkey_mode);
                    match mode {
                        1 => { set_mic_mute(true); }   // PTT resting state = muted
                        2 => { set_mic_mute(false); }  // PTM resting state = unmuted
                        _ => {}                         // Toggle: keep current state
                    }
                    LRESULT(0)
                }
                ID_CTRL_OVERLAY_CHECK if notify_code == BN_CLICKED as u16 => {
                    apply_settings_from_controls(hwnd);
                    LRESULT(0)
                }
                ID_CTRL_TRAY_CHECK if notify_code == BN_CLICKED as u16 => {
                    apply_settings_from_controls(hwnd);
                    LRESULT(0)
                }
                ID_CTRL_SIZE_EDIT | ID_CTRL_OFFSET_X_EDIT | ID_CTRL_OFFSET_Y_EDIT
                    if notify_code == EN_CHANGE as u16 =>
                {
                    apply_settings_from_controls(hwnd);
                    LRESULT(0)
                }

                ID_CTRL_COLOR_MUTED | ID_CTRL_COLOR_UNMUTED | ID_CTRL_COLOR_ERROR
                    if notify_code == BN_CLICKED as u16 =>
                {
                    let initial = STATE.with(|s| {
                        let st = s.borrow();
                        match control_id {
                            ID_CTRL_COLOR_MUTED   => st.color_muted,
                            ID_CTRL_COLOR_UNMUTED => st.color_unmuted,
                            _                     => st.color_error,
                        }
                    });
                    if let Some(rgb) = show_native_color_picker(hwnd, initial) {
                        STATE.with(|s| {
                            let mut st = s.borrow_mut();
                            let slot = match control_id {
                                ID_CTRL_COLOR_MUTED   => &mut st.color_muted,
                                ID_CTRL_COLOR_UNMUTED => &mut st.color_unmuted,
                                _                     => &mut st.color_error,
                            };
                            *slot = rgb;
                        });
                        if let Ok(h) = GetDlgItem(hwnd, control_id as i32) {
                            let _ = InvalidateRect(h, None, BOOL(1));
                        }
                        apply_settings_from_controls(hwnd);
                    }
                    LRESULT(0)
                }

                ID_CTRL_SAVE_BTN => {
                    apply_settings_from_controls(hwnd);
                    let mut hotkey_buf = [0u16; 64];
                    if let Ok(h) = GetDlgItem(hwnd, ID_CTRL_HOTKEY_DISPLAY as i32) {
                        let _ = GetWindowTextW(h, &mut hotkey_buf);
                    }
                    let hotkey_val = String::from_utf16_lossy(&hotkey_buf).trim_matches('\0').trim().to_string();
                    STATE.with(|s| s.borrow_mut().hotkey_str = hotkey_val);

                    let mode = STATE.with(|s| s.borrow().hotkey_mode);
                    match mode {
                        1 => { set_mic_mute(true); }
                        2 => { set_mic_mute(false); }
                        _ => { sync_mute_state(); }
                    }

                    HOTKEY_HELD.with(|h| *h.borrow_mut() = false);
                    RECORDING_HOTKEY.with(|r| *r.borrow_mut() = false);
                    SETTINGS_SNAPSHOT.with(|s| *s.borrow_mut() = None);

                    save_settings_to_ini();
                    register_app_hotkey();
                    register_volume_callback();
                    redraw_overlay();
                    sync_visibilities();
                    let _ = DestroyWindow(hwnd);
                    LRESULT(0)
                }

                ID_CTRL_DISCARD_BTN => {
                    RECORDING_HOTKEY.with(|r| *r.borrow_mut() = false);
                    let snapshot = SETTINGS_SNAPSHOT.with(|s| s.borrow_mut().take());
                    if let Some(snap) = snapshot {
                        STATE.with(|s| *s.borrow_mut() = snap);
                    }
                    register_app_hotkey();
                    register_volume_callback();
                    redraw_overlay();
                    sync_visibilities();
                    let _ = DestroyWindow(hwnd);
                    LRESULT(0)
                }

                _ => DefWindowProcW(hwnd, msg, wparam, lparam),
            }
        }

        WM_TIMER => {
            if wparam.0 == HOTKEY_RECORD_TIMER_ID {
                let _ = KillTimer(hwnd, HOTKEY_RECORD_TIMER_ID);
                if RECORDING_HOTKEY.with(|r| *r.borrow()) {
                    RECORDING_HOTKEY.with(|r| *r.borrow_mut() = false);
                    if let Ok(h_btn) = GetDlgItem(hwnd, ID_CTRL_HOTKEY_BTN as i32) {
                        let _ = SetWindowTextW(h_btn, w!("Record"));
                    }
                    register_app_hotkey();
                }
            }
            LRESULT(0)
        }

        WM_ACTIVATE => {
            if (wparam.0 & 0xFFFF) as u16 == 0 /* WA_INACTIVE */ {
                if RECORDING_HOTKEY.with(|r| *r.borrow()) {
                    RECORDING_HOTKEY.with(|r| *r.borrow_mut() = false);
                    if let Ok(h_btn) = GetDlgItem(hwnd, ID_CTRL_HOTKEY_BTN as i32) {
                        let _ = SetWindowTextW(h_btn, w!("Record"));
                    }
                }
            }
            LRESULT(0)
        }

        WM_CTLCOLORSTATIC => {
            let ctrl_hwnd = HWND(lparam.0 as *mut _);
            let ctrl_id = GetDlgCtrlID(ctrl_hwnd) as isize;
            if ctrl_id == ID_CTRL_COPYRIGHT {
                let hdc = HDC(wparam.0 as *mut _);
                SetTextColor(hdc, COLORREF(0x00999999));
                SetBkMode(hdc, TRANSPARENT);
                return LRESULT(GetStockObject(NULL_BRUSH).0 as isize);
            }
            // All other statics: let Windows handle them normally
            DefWindowProcW(hwnd, msg, wparam, lparam)
        }
        WM_CLOSE => {
            let has_changes = SETTINGS_SNAPSHOT.with(|s| {
                s.borrow().as_ref().map_or(false, |snap| {
                    STATE.with(|st| {
                        let cur = st.borrow();
                        snap.show_tray_icon    != cur.show_tray_icon    ||
                        snap.show_overlay      != cur.show_overlay      ||
                        snap.glyph_size        != cur.glyph_size        ||
                        snap.glyph_corner      != cur.glyph_corner      ||
                        snap.offset_x          != cur.offset_x          ||
                        snap.offset_y          != cur.offset_y          ||
                        snap.glyph_shape       != cur.glyph_shape       ||
                        snap.hotkey_str        != cur.hotkey_str        ||
                        snap.hotkey_mode       != cur.hotkey_mode       ||
                        snap.color_muted       != cur.color_muted       ||
                        snap.color_unmuted     != cur.color_unmuted     ||
                        snap.color_error       != cur.color_error       ||
                        snap.opacity_muted     != cur.opacity_muted     ||
                        snap.opacity_unmuted   != cur.opacity_unmuted   ||
                        snap.opacity_error     != cur.opacity_error     ||
                        snap.device_override   != cur.device_override
                    })
                })
            });

            if has_changes {
                let response = MessageBoxW(
                    hwnd,
                    w!("Discard changes?"),
                    w!("MicKey Settings"),
                    MB_YESNO | MB_ICONQUESTION | MB_DEFBUTTON2,
                );
                if response != IDYES {
                    return LRESULT(0);
                }
            }

            RECORDING_HOTKEY.with(|r| *r.borrow_mut() = false);
            let snapshot = SETTINGS_SNAPSHOT.with(|s| s.borrow_mut().take());
            if let Some(snap) = snapshot {
                STATE.with(|s| *s.borrow_mut() = snap);
            }
            register_app_hotkey();
            register_volume_callback();
            redraw_overlay();
            sync_visibilities();
            let _ = DestroyWindow(hwnd);
            LRESULT(0)
        }

        WM_DESTROY => {
            RECORDING_HOTKEY.with(|r| *r.borrow_mut() = false);
            SETTINGS_HWND.with(|s| *s.borrow_mut() = HWND::default());
            LRESULT(0)
        }
        _ => DefWindowProcW(hwnd, msg, wparam, lparam),
    }
}