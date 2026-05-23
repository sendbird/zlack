#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use std::{
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        mpsc, Arc, Mutex, MutexGuard,
    },
    thread,
    time::{Duration as StdDuration, Instant},
};

#[cfg(target_os = "macos")]
use core::ffi::c_void;
#[cfg(target_os = "macos")]
use core_foundation::{
    base::{CFTypeRef, TCFType},
    dictionary::{CFDictionaryGetValueIfPresent, CFDictionaryRef},
    number::{kCFNumberFloat64Type, kCFNumberSInt32Type, CFNumberGetValue, CFNumberRef},
    string::CFString,
};
#[cfg(target_os = "macos")]
use core_graphics::window::{
    copy_window_info, kCGWindowAlpha, kCGWindowBounds, kCGWindowLayer,
    kCGWindowListExcludeDesktopElements, kCGWindowListOptionIncludingWindow,
    kCGWindowListOptionOnScreenAboveWindow, kCGWindowListOptionOnScreenOnly, kCGWindowNumber,
    kCGWindowOwnerPID,
    CGWindowID,
};
#[cfg(target_os = "macos")]
use objc2::{msg_send, runtime::AnyObject};
use tauri::{
    menu::{Menu, MenuBuilder, MenuItem, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    utils::config::BackgroundThrottlingPolicy,
    Manager, Theme, TitleBarStyle, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
#[cfg(not(target_os = "windows"))]
use tauri_plugin_notification::NotificationExt;

#[cfg(target_os = "windows")]
use tauri_winrt_notification::{Duration, Sound, Toast};

const SLACK_URL: &str = "https://app.slack.com/client";
const HIDDEN_MEMORY_SAVER_DELAY: StdDuration = StdDuration::from_secs(180);
const OCCLUDED_MEMORY_SAVER_DELAY: StdDuration = StdDuration::from_secs(180);
const MEMORY_SAVER_POLL_INTERVAL: StdDuration = StdDuration::from_secs(30);
#[cfg(target_os = "macos")]
const NS_WINDOW_OCCLUSION_STATE_VISIBLE: usize = 1 << 1;
#[cfg(target_os = "macos")]
const MATERIAL_VISIBILITY_SAMPLE_GRID_SIZE: usize = 12;
#[cfg(target_os = "macos")]
const MATERIAL_VISIBILITY_MIN_VISIBLE_RATIO: f64 = 0.10;
#[cfg(target_os = "macos")]
const MATERIAL_VISIBILITY_BLOCKER_ALPHA_THRESHOLD: f64 = 0.05;
const SHORTCUT_ACTIONS_JS: &str = include_str!("../shortcut_actions.js");
const SLACK_ACTION_SHORTCUTS: &[(&str, &str, &str)] = &[
    ("zlack_file_new_message", "New Message", "CmdOrCtrl+N"),
    ("zlack_file_new_canvas", "New Canvas", "CmdOrCtrl+Shift+N"),
    ("zlack_go_search", "Search", "CmdOrCtrl+F"),
    ("zlack_go_all_unreads", "All Unreads", "CmdOrCtrl+Shift+A"),
    ("zlack_go_threads", "Threads", "CmdOrCtrl+Shift+T"),
    ("zlack_go_all_dms", "All DMs", "CmdOrCtrl+Shift+K"),
    ("zlack_go_activity", "Activity", "CmdOrCtrl+Shift+M"),
    (
        "zlack_go_channel_browser",
        "Channel Browser",
        "CmdOrCtrl+Shift+L",
    ),
    (
        "zlack_go_people",
        "People & User Groups",
        "CmdOrCtrl+Shift+E",
    ),
    ("zlack_go_downloads", "Downloads", "CmdOrCtrl+Shift+J"),
    ("zlack_go_history_back", "Back", "CmdOrCtrl+["),
    ("zlack_go_history_forward", "Forward", "CmdOrCtrl+]"),
];
#[derive(Default)]
struct MemorySaverState {
    generation: u64,
    armed: bool,
}

type SharedMemorySaverState = Arc<Mutex<MemorySaverState>>;

fn is_slack_url(url: &Url) -> bool {
    matches!(url.host_str(), Some("slack.com") | Some("app.slack.com"))
        || url
            .host_str()
            .is_some_and(|host| host.ends_with(".slack.com"))
}

fn is_zoom_url(url: &Url) -> bool {
    matches!(url.scheme(), "zoommtg" | "zoomus" | "zoomphonecall")
        || ((url.scheme() == "http" || url.scheme() == "https")
            && (matches!(url.host_str(), Some("zoom.us") | Some("zoom.com"))
                || url.host_str().is_some_and(|host| {
                    host.ends_with(".zoom.us") || host.ends_with(".zoom.com")
                })))
}

fn is_allowed_external_url(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };

    matches!(parsed.scheme(), "http" | "https" | "zoommtg" | "zoomus" | "zoomphonecall")
}

fn open_external_url_with_os(url: &str) -> Result<(), String> {
    if !is_allowed_external_url(url) {
        return Err(format!("unsupported external URL scheme: {url}"));
    }

    #[cfg(target_os = "macos")]
    let mut command = {
        let mut command = Command::new("open");
        command.arg(url);
        command
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("cmd");
        command.args(["/C", "start", "", url]);
        command
    };

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(url);
        command
    };

    command
        .spawn()
        .map(|_| ())
        .map_err(|error| format!("failed to open external URL: {error}"))
}

#[tauri::command]
fn open_external_url(url: String) -> Result<(), String> {
    open_external_url_with_os(&url)
}

#[tauri::command]
fn notify(
    app_handle: tauri::AppHandle,
    title: String,
    body: String,
    team_id: Option<String>,
    channel_id: Option<String>,
) {
    let mut target_url = None;
    if let (Some(tid), Some(cid)) = (&team_id, &channel_id) {
        if tid != "unknown" && cid != "unknown" {
            target_url = Some(format!("https://app.slack.com/client/{}/{}", tid, cid));
        } else if tid != "unknown" {
            target_url = Some(format!("https://app.slack.com/client/{}", tid));
        }
    }

    #[cfg(not(target_os = "windows"))]
    let _ = &target_url;

    #[cfg(target_os = "windows")]
    {
        let app_handle_clone = app_handle.clone();
        let identifier = app_handle.config().identifier.clone();
        let target_url_clone = target_url.clone();

        let _ = app_handle.run_on_main_thread(move || {
            let res = Toast::new(&identifier)
                .title(&title)
                .text1(&body)
                .sound(Some(Sound::SMS))
                .duration(Duration::Short)
                .on_activated(move |_| {
                    let app_dispatcher = app_handle_clone.clone();
                    let app_worker = app_handle_clone.clone();
                    let url_to_open = target_url_clone.clone();

                    let _ = app_dispatcher.run_on_main_thread(move || {
                        restore_or_create_main_window(&app_worker);

                        if let Some(window) = app_worker.get_webview_window("main") {
                            if let Some(url) = url_to_open {
                                match serde_json::to_string(&url) {
                                    Ok(url_json) => {
                                        let js = format!(
                                            r#"
                                if (window.location.href !== {url}) {{
                                    window.location.href = {url};
                                }}
                              "#,
                                            url = url_json
                                        );
                                        if let Err(e) = window.eval(&js) {
                                            eprintln!("Zlack: Failed to navigate on click: {}", e);
                                        }
                                    }
                                    Err(e) => eprintln!(
                                        "Zlack: Failed to serialize navigation URL: {}",
                                        e
                                    ),
                                }
                            }
                        }
                    });
                    Ok(())
                })
                .show();

            if let Err(e) = res {
                eprintln!("Zlack: Failed to show toast: {}", e);
            }
        });
    }

    #[cfg(not(target_os = "windows"))]
    {
        if let Err(e) = app_handle
            .notification()
            .builder()
            .title(title)
            .body(body)
            .show()
        {
            eprintln!("Zlack: Failed to show notification: {}", e);
        }
    }
}

#[tauri::command]
fn start_window_drag(window: WebviewWindow) {
    if let Err(error) = window.start_dragging() {
        eprintln!("Zlack: Failed to start window drag: {}", error);
    }
}

#[tauri::command]
fn toggle_window_maximize(window: WebviewWindow) {
    match window.is_maximized() {
        Ok(true) => {
            if let Err(error) = window.unmaximize() {
                eprintln!("Zlack: Failed to unmaximize window: {}", error);
            }
        }
        Ok(false) => {
            if let Err(error) = window.maximize() {
                eprintln!("Zlack: Failed to maximize window: {}", error);
            }
        }
        Err(error) => {
            eprintln!("Zlack: Failed to read window maximize state: {}", error);
        }
    }
}

fn run_slack_action_shortcut(app: &tauri::AppHandle, shortcut_id: &str) {
    eprintln!("Zlack: menu shortcut fired: {shortcut_id}");

    #[cfg(not(target_os = "windows"))]
    let _ = app
        .notification()
        .builder()
        .title("Zlack shortcut fired")
        .body(shortcut_id)
        .show();

    let Some(window) = app.get_webview_window("main") else {
        eprintln!("Zlack: no main webview window for shortcut: {shortcut_id}");
        return;
    };

    let js = format!(
        r#"
        (() => {{
            const shortcutId = '{shortcut}';
            function showZlackShortcutDiagnostic(stage, detail) {{
                const id = 'zlack-shortcut-diagnostic';
                let overlay = document.getElementById(id);
                if (!overlay) {{
                    overlay = document.createElement('div');
                    overlay.id = id;
                    overlay.style.cssText = [
                        'position: fixed',
                        'z-index: 2147483647',
                        'top: 44px',
                        'right: 16px',
                        'max-width: 520px',
                        'padding: 12px 14px',
                        'border-radius: 10px',
                        'background: rgba(20, 20, 24, 0.94)',
                        'color: #fff',
                        'font: 12px/1.4 -apple-system, BlinkMacSystemFont, sans-serif',
                        'box-shadow: 0 8px 28px rgba(0, 0, 0, 0.36)',
                        'white-space: pre-wrap',
                        'pointer-events: none'
                    ].join(';');
                    document.documentElement.appendChild(overlay);
                }}
                overlay.textContent = 'Zlack shortcut\n' + shortcutId + '\n' + stage + '\n' + detail;
                window.clearTimeout(window.__zlackShortcutDiagnosticTimer);
                window.__zlackShortcutDiagnosticTimer = window.setTimeout(() => overlay.remove(), 5000);
            }}

            showZlackShortcutDiagnostic('eval reached', window.location.href);
            try {{
                {runner}
                const result = runZlackSlackShortcutAction(shortcutId);
                showZlackShortcutDiagnostic('action result: ' + result, window.location.href);
                console.log('Zlack shortcut result', {{ shortcutId, result, href: window.location.href }});
            }} catch (error) {{
                showZlackShortcutDiagnostic('exception', error && (error.stack || error.message || String(error)));
                console.error('Zlack shortcut exception', shortcutId, error);
            }}
        }})();
        "#,
        runner = SHORTCUT_ACTIONS_JS,
        shortcut = shortcut_id
    );
    if let Err(error) = window.eval(&js) {
        eprintln!("Zlack: Failed to run Slack shortcut action: {}", error);
    }
}

fn install_app_menu(app: &mut tauri::App) -> tauri::Result<()> {
    let app_menu = SubmenuBuilder::new(app, "Zlack")
        .item(&PredefinedMenuItem::quit(app, Some("Quit Zlack"))?)
        .build()?;

    let file_new_message = MenuItemBuilder::with_id("zlack_file_new_message", "New Message")
        .accelerator("CmdOrCtrl+N")
        .build(app)?;
    let file_new_canvas = MenuItemBuilder::with_id("zlack_file_new_canvas", "New Canvas")
        .accelerator("CmdOrCtrl+Shift+N")
        .build(app)?;
    let file_close_window = MenuItemBuilder::with_id("zlack_file_close_window", "Close Window")
        .accelerator("CmdOrCtrl+W")
        .build(app)?;
    let file_show_main =
        MenuItemBuilder::with_id("zlack_file_show_main", "Show Main Window").build(app)?;
    let file_menu = SubmenuBuilder::new(app, "File")
        .item(&file_new_message)
        .item(&file_new_canvas)
        .separator()
        .item(&file_close_window)
        .item(&file_show_main)
        .build()?;

    let edit_menu = SubmenuBuilder::new(app, "Edit")
        .item(&PredefinedMenuItem::undo(app, Some("Undo"))?)
        .item(&PredefinedMenuItem::redo(app, Some("Redo"))?)
        .separator()
        .item(&PredefinedMenuItem::cut(app, Some("Cut"))?)
        .item(&PredefinedMenuItem::copy(app, Some("Copy"))?)
        .item(&PredefinedMenuItem::paste(app, Some("Paste"))?)
        .item(&PredefinedMenuItem::select_all(app, Some("Select All"))?)
        .build()?;

    let mut slack_menu = SubmenuBuilder::new(app, "Go");
    for (id, text, accelerator) in SLACK_ACTION_SHORTCUTS {
        let item = MenuItemBuilder::with_id(*id, *text)
            .accelerator(*accelerator)
            .build(app)?;
        slack_menu = slack_menu.item(&item);
    }
    let slack_menu = slack_menu.build()?;

    let menu = MenuBuilder::new(app)
        .items(&[&app_menu, &file_menu, &edit_menu, &slack_menu])
        .build()?;
    app.set_menu(menu)?;

    app.on_menu_event(|app_handle, event| {
        let id = event.id().0.as_str();
        match id {
            "zlack_file_close_window" => {
                if let Some(window) = app_handle.get_webview_window("main") {
                    if let Err(error) = window.hide() {
                        eprintln!(
                            "Zlack: Failed to hide main window from File menu: {}",
                            error
                        );
                    }
                    schedule_memory_saver_destroy_after(
                        app_handle.clone(),
                        HIDDEN_MEMORY_SAVER_DELAY,
                        true,
                    );
                }
            }
            "zlack_file_show_main" => restore_or_create_main_window(app_handle),
            _ if SLACK_ACTION_SHORTCUTS
                .iter()
                .any(|(shortcut_id, _, _)| *shortcut_id == id) =>
            {
                run_slack_action_shortcut(app_handle, id);
            }
            _ => {}
        }
    });

    Ok(())
}

fn restore_window(window: &WebviewWindow) {
    if let Err(e) = window.set_skip_taskbar(false) {
        eprintln!("Zlack: Failed to set_skip_taskbar: {}", e);
    }

    if let Err(e) = window.unminimize() {
        eprintln!("Zlack: Failed to Unminimize: {}", e);
    }

    if let Err(e) = window.show() {
        eprintln!("Zlack: Failed to Show: {}", e);
    }

    let _ = window.set_always_on_top(true);
    if let Err(e) = window.set_focus() {
        eprintln!("Zlack: Failed to Focus: {}", e);
    }
    let _ = window.set_always_on_top(false);
}

fn restore_or_create_main_window(app: &tauri::AppHandle) {
    cancel_memory_saver_destroy(app);

    if let Some(window) = app.get_webview_window("main") {
        restore_window(&window);
        return;
    }

    match create_main_window(app) {
        Ok(window) => restore_window(&window),
        Err(e) => eprintln!("Zlack: Failed to create main window: {}", e),
    }
}

fn create_main_window(app: &tauri::AppHandle) -> tauri::Result<WebviewWindow> {
    let window = WebviewWindowBuilder::new(
        app,
        "main",
        WebviewUrl::External(SLACK_URL.parse().unwrap()),
    )
    .user_agent(
        if cfg!(target_os = "macos") {
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36"
        } else {
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/143.0.0.0 Safari/537.36"
        }
    )
    .title("")
    .theme(Some(Theme::Dark))
    .title_bar_style(TitleBarStyle::Overlay)
    .hidden_title(true)
    .background_throttling(BackgroundThrottlingPolicy::Suspend)
    .inner_size(1200.0, 800.0)
    .resizable(true)
    .initialization_script(include_str!("../preload.js"))
    .on_navigation(|url| {
        if is_zoom_url(url) {
            if let Err(error) = open_external_url_with_os(url.as_str()) {
                eprintln!("Zlack: Failed to open Zoom navigation externally: {error}");
            }
            return false;
        }

        true
    })
    .on_new_window(|url, _features| {
        if !is_slack_url(&url) {
            if let Err(error) = open_external_url_with_os(url.as_str()) {
                eprintln!("Zlack: Failed to open new-window URL externally: {error}");
            }
            return tauri::webview::NewWindowResponse::Deny;
        }

        tauri::webview::NewWindowResponse::Allow
    })
    .disable_drag_drop_handler()
    .build()?;

    Ok(window)
}

fn destroy_window(window: &WebviewWindow) {
    if let Err(e) = window.destroy() {
        eprintln!("Zlack: Failed to destroy main window: {}", e);
    }
}

fn lock_memory_saver_state(state: &SharedMemorySaverState) -> MutexGuard<'_, MemorySaverState> {
    match state.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            eprintln!("Zlack: Memory saver state lock poisoned; continuing");
            poisoned.into_inner()
        }
    }
}

fn shared_memory_saver_state(app: &tauri::AppHandle) -> SharedMemorySaverState {
    Arc::clone(app.state::<SharedMemorySaverState>().inner())
}

fn cancel_memory_saver_destroy(app: &tauri::AppHandle) {
    let state = shared_memory_saver_state(app);
    let mut state = lock_memory_saver_state(&state);
    state.generation = state.generation.wrapping_add(1);
    state.armed = false;
}

fn is_window_effectively_visible(window: &WebviewWindow) -> bool {
    match window.is_visible() {
        Ok(true) => {}
        Ok(false) => return false,
        Err(e) => {
            eprintln!("Zlack: Failed to check window visibility: {}", e);
            return true;
        }
    }

    match window.is_minimized() {
        Ok(true) => return false,
        Ok(false) => {}
        Err(e) => {
            eprintln!("Zlack: Failed to check window minimized state: {}", e);
            return true;
        }
    }

    #[cfg(target_os = "macos")]
    {
        is_macos_window_occlusion_visible(window)
    }

    #[cfg(not(target_os = "macos"))]
    {
        true
    }
}

#[cfg(target_os = "macos")]
#[derive(Clone, Copy)]
struct MacosWindowBounds {
    x: f64,
    y: f64,
    width: f64,
    height: f64,
}

#[cfg(target_os = "macos")]
impl MacosWindowBounds {
    fn contains_point(&self, x: f64, y: f64) -> bool {
        x >= self.x && x <= self.x + self.width && y >= self.y && y <= self.y + self.height
    }

    fn is_material(&self) -> bool {
        self.width > 1.0 && self.height > 1.0
    }
}

#[cfg(target_os = "macos")]
struct MacosWindowListEntry {
    window_number: CGWindowID,
    owner_pid: Option<i32>,
    layer: i32,
    alpha: f64,
    bounds: Option<MacosWindowBounds>,
}

#[cfg(target_os = "macos")]
fn is_macos_window_occlusion_visible(window: &WebviewWindow) -> bool {
    let ns_window = match window.ns_window() {
        Ok(ns_window) if !ns_window.is_null() => ns_window,
        Ok(_) => return true,
        Err(e) => {
            eprintln!("Zlack: Failed to get NSWindow handle: {}", e);
            return true;
        }
    };

    // SAFETY: `ns_window` is the live NSWindow pointer returned by Tauri on the main thread.
    let occlusion_state: usize = unsafe { msg_send![ns_window as *mut AnyObject, occlusionState] };
    if (occlusion_state & NS_WINDOW_OCCLUSION_STATE_VISIBLE) == 0 {
        return false;
    }

    // SAFETY: `windowNumber` is an Objective-C getter on a valid NSWindow and returns an NSInteger.
    let window_number: isize = unsafe { msg_send![ns_window as *mut AnyObject, windowNumber] };
    let Ok(window_number) = CGWindowID::try_from(window_number) else {
        return true;
    };

    match is_macos_window_materially_visible_by_window_list(window_number) {
        Some(true) => true,
        Some(false) => {
            eprintln!("Zlack: Window list coverage indicates main window is covered");
            false
        }
        None => true,
    }
}

#[cfg(target_os = "macos")]
fn is_macos_window_materially_visible_by_window_list(window_number: CGWindowID) -> Option<bool> {
    let target_windows = copy_window_info(kCGWindowListOptionIncludingWindow, window_number);
    let target_bounds = target_windows.as_ref().and_then(|windows| {
        windows
            .get_all_values()
            .into_iter()
            .filter_map(macos_window_list_entry)
            .find(|entry| entry.window_number == window_number)
            .and_then(|entry| entry.bounds)
    });

    if let Some(target_bounds) = target_bounds {
        if let Some(blocker_windows) = copy_window_info(
            kCGWindowListOptionOnScreenAboveWindow | kCGWindowListExcludeDesktopElements,
            window_number,
        ) {
            let blocker_bounds = blocker_windows
                .get_all_values()
                .into_iter()
                .filter_map(macos_window_list_entry)
                .filter(|entry| {
                    entry.layer == 0 && entry.alpha > MATERIAL_VISIBILITY_BLOCKER_ALPHA_THRESHOLD
                })
                .filter_map(|entry| entry.bounds)
                .filter(|bounds| bounds.is_material())
                .collect::<Vec<_>>();

            let visible_ratio = macos_bounds_material_visible_ratio(target_bounds, &blocker_bounds);
            let is_visible = visible_ratio >= MATERIAL_VISIBILITY_MIN_VISIBLE_RATIO;
            eprintln!(
                "Zlack: Window list target matched by window number; visible ratio {:.2}",
                visible_ratio
            );
            if !is_visible {
                eprintln!(
                    "Zlack: Window list coverage indicates main window is covered by {} blocker windows",
                    blocker_bounds.len()
                );
            }

            return Some(is_visible);
        }
    }

    is_macos_window_materially_visible_by_full_window_list(window_number)
}

#[cfg(target_os = "macos")]
fn is_macos_window_materially_visible_by_full_window_list(
    window_number: CGWindowID,
) -> Option<bool> {
    let current_pid = std::process::id() as i32;
    let windows = copy_window_info(
        kCGWindowListOptionOnScreenOnly | kCGWindowListExcludeDesktopElements,
        0,
    )?;
    let entries = windows
        .get_all_values()
        .into_iter()
        .filter_map(macos_window_list_entry)
        .collect::<Vec<_>>();

    let mut target_index = entries
        .iter()
        .position(|entry| entry.window_number == window_number && entry.bounds.is_some());
    let mut target_source = "window number";

    if target_index.is_none() {
        target_index = entries
            .iter()
            .enumerate()
            .filter(|(_, entry)| {
                entry.owner_pid == Some(current_pid)
                    && entry.layer == 0
                    && entry.bounds.as_ref().is_some_and(MacosWindowBounds::is_material)
            })
            .max_by(|(_, left), (_, right)| {
                let left_area = left.bounds.as_ref().map(macos_window_bounds_area).unwrap_or(0.0);
                let right_area = right.bounds.as_ref().map(macos_window_bounds_area).unwrap_or(0.0);
                left_area.total_cmp(&right_area)
            })
            .map(|(index, _)| index);
        target_source = "owner pid";
    }

    let target_index = target_index?;
    let target_bounds = entries[target_index].bounds?;
    if !target_bounds.is_material() {
        return None;
    }

    let blocker_bounds = entries[..target_index]
        .iter()
        .filter(|entry| entry.layer == 0 && entry.alpha > MATERIAL_VISIBILITY_BLOCKER_ALPHA_THRESHOLD)
        .filter_map(|entry| entry.bounds)
        .filter(MacosWindowBounds::is_material)
        .collect::<Vec<_>>();

    let visible_ratio = macos_bounds_material_visible_ratio(target_bounds, &blocker_bounds);
    let is_visible = visible_ratio >= MATERIAL_VISIBILITY_MIN_VISIBLE_RATIO;
    eprintln!(
        "Zlack: Full window list target matched by {}; visible ratio {:.2}",
        target_source, visible_ratio
    );
    if !is_visible {
        eprintln!(
            "Zlack: Full window list coverage indicates main window is covered by {} blocker windows",
            blocker_bounds.len()
        );
    }

    Some(is_visible)
}

#[cfg(target_os = "macos")]
fn macos_bounds_material_visible_ratio(
    target_bounds: MacosWindowBounds,
    blocker_bounds: &[MacosWindowBounds],
) -> f64 {
    if !target_bounds.is_material() {
        return 1.0;
    }

    let grid_size = MATERIAL_VISIBILITY_SAMPLE_GRID_SIZE;
    let total_samples = grid_size * grid_size;
    let mut visible_samples = 0;

    for row in 0..grid_size {
        let y = target_bounds.y + target_bounds.height * (row as f64 + 0.5) / grid_size as f64;
        for column in 0..grid_size {
            let x =
                target_bounds.x + target_bounds.width * (column as f64 + 0.5) / grid_size as f64;
            if !blocker_bounds
                .iter()
                .any(|bounds| bounds.contains_point(x, y))
            {
                visible_samples += 1;
            }
        }
    }

    visible_samples as f64 / total_samples as f64
}

#[cfg(target_os = "macos")]
fn macos_window_bounds_area(bounds: &MacosWindowBounds) -> f64 {
    bounds.width * bounds.height
}

#[cfg(target_os = "macos")]
fn macos_window_list_entry(raw_window: *const c_void) -> Option<MacosWindowListEntry> {
    let dictionary = raw_window as CFDictionaryRef;
    if dictionary.is_null() {
        return None;
    }

    let window_number = cg_window_number(dictionary)?;
    Some(MacosWindowListEntry {
        window_number,
        owner_pid: cg_window_i32(dictionary, cg_window_owner_pid_key()),
        layer: cg_window_i32(dictionary, cg_window_layer_key()).unwrap_or(0),
        alpha: cg_window_f64(dictionary, cg_window_alpha_key()).unwrap_or(1.0),
        bounds: cg_window_bounds(dictionary),
    })
}

#[cfg(target_os = "macos")]
fn cg_window_number_key() -> CFTypeRef {
    // SAFETY: CoreGraphics exports this immutable CFString key for window-list dictionaries.
    unsafe { kCGWindowNumber.cast::<c_void>() }
}

#[cfg(target_os = "macos")]
fn cg_window_layer_key() -> CFTypeRef {
    // SAFETY: CoreGraphics exports this immutable CFString key for window-list dictionaries.
    unsafe { kCGWindowLayer.cast::<c_void>() }
}

#[cfg(target_os = "macos")]
fn cg_window_alpha_key() -> CFTypeRef {
    // SAFETY: CoreGraphics exports this immutable CFString key for window-list dictionaries.
    unsafe { kCGWindowAlpha.cast::<c_void>() }
}

#[cfg(target_os = "macos")]
fn cg_window_owner_pid_key() -> CFTypeRef {
    // SAFETY: CoreGraphics exports this immutable CFString key for window-list dictionaries.
    unsafe { kCGWindowOwnerPID.cast::<c_void>() }
}

#[cfg(target_os = "macos")]
fn cg_window_bounds_key() -> CFTypeRef {
    // SAFETY: CoreGraphics exports this immutable CFString key for window-list dictionaries.
    unsafe { kCGWindowBounds.cast::<c_void>() }
}

#[cfg(target_os = "macos")]
fn cg_window_number(dictionary: CFDictionaryRef) -> Option<CGWindowID> {
    let raw_number = cg_window_i32(dictionary, cg_window_number_key())?;
    CGWindowID::try_from(raw_number).ok()
}

#[cfg(target_os = "macos")]
fn cg_window_bounds(dictionary: CFDictionaryRef) -> Option<MacosWindowBounds> {
    let bounds_dictionary =
        cg_dictionary_value(dictionary, cg_window_bounds_key())? as CFDictionaryRef;
    Some(MacosWindowBounds {
        x: cg_bounds_value(bounds_dictionary, "X")?,
        y: cg_bounds_value(bounds_dictionary, "Y")?,
        width: cg_bounds_value(bounds_dictionary, "Width")?,
        height: cg_bounds_value(bounds_dictionary, "Height")?,
    })
}

#[cfg(target_os = "macos")]
fn cg_bounds_value(dictionary: CFDictionaryRef, key: &str) -> Option<f64> {
    let key = CFString::new(key);
    cg_dictionary_number_value(dictionary, key.as_CFTypeRef())
        .and_then(|number| cf_number_to_f64(number))
}

#[cfg(target_os = "macos")]
fn cg_window_i32(dictionary: CFDictionaryRef, key: CFTypeRef) -> Option<i32> {
    cg_dictionary_number_value(dictionary, key).and_then(|number| cf_number_to_i32(number))
}

#[cfg(target_os = "macos")]
fn cg_window_f64(dictionary: CFDictionaryRef, key: CFTypeRef) -> Option<f64> {
    cg_dictionary_number_value(dictionary, key).and_then(|number| cf_number_to_f64(number))
}

#[cfg(target_os = "macos")]
fn cg_dictionary_number_value(dictionary: CFDictionaryRef, key: CFTypeRef) -> Option<CFNumberRef> {
    cg_dictionary_value(dictionary, key).map(|value| value as CFNumberRef)
}

#[cfg(target_os = "macos")]
fn cg_dictionary_value(dictionary: CFDictionaryRef, key: CFTypeRef) -> Option<CFTypeRef> {
    let mut value = std::ptr::null();
    // SAFETY: CoreGraphics provides immutable dictionaries and static CFString keys; output is checked.
    let found =
        unsafe { CFDictionaryGetValueIfPresent(dictionary, key.cast::<c_void>(), &mut value) };

    if found == 0 || value.is_null() {
        None
    } else {
        Some(value as CFTypeRef)
    }
}

#[cfg(target_os = "macos")]
fn cf_number_to_i32(number: CFNumberRef) -> Option<i32> {
    let mut value = 0;
    // SAFETY: `value` points to valid writable storage for the requested CFNumber type.
    let ok = unsafe {
        CFNumberGetValue(
            number,
            kCFNumberSInt32Type,
            (&mut value as *mut i32).cast::<c_void>(),
        )
    };
    ok.then_some(value)
}

#[cfg(target_os = "macos")]
fn cf_number_to_f64(number: CFNumberRef) -> Option<f64> {
    let mut value = 0.0;
    // SAFETY: `value` points to valid writable storage for the requested CFNumber type.
    let ok = unsafe {
        CFNumberGetValue(
            number,
            kCFNumberFloat64Type,
            (&mut value as *mut f64).cast::<c_void>(),
        )
    };
    ok.then_some(value)
}

fn schedule_memory_saver_destroy_after(
    app: tauri::AppHandle,
    delay: StdDuration,
    require_not_visible: bool,
) {
    let state = shared_memory_saver_state(&app);
    let scheduled_generation = {
        let mut state = lock_memory_saver_state(&state);
        state.generation = state.generation.wrapping_add(1);
        state.armed = true;
        state.generation
    };

    thread::spawn(move || {
        thread::sleep(delay);

        let should_destroy = {
            let mut state = lock_memory_saver_state(&state);
            if !state.armed || state.generation != scheduled_generation {
                false
            } else {
                state.armed = false;
                true
            }
        };

        if !should_destroy {
            return;
        }

        let app_for_main_thread = app.clone();
        if let Err(e) = app.run_on_main_thread(move || {
            if let Some(window) = app_for_main_thread.get_webview_window("main") {
                if require_not_visible && is_window_effectively_visible(&window) {
                    return;
                }

                destroy_window(&window);
            }
        }) {
            eprintln!("Zlack: Failed to schedule memory saver destroy: {}", e);
        }
    });
}

fn start_memory_saver_visibility_monitor(app: tauri::AppHandle) {
    thread::spawn(move || {
        let mut invisible_since: Option<Instant> = None;

        loop {
            thread::sleep(MEMORY_SAVER_POLL_INTERVAL);

            let (tx, rx) = mpsc::channel();
            let app_for_check = app.clone();
            if let Err(e) = app.run_on_main_thread(move || {
                let visibility = app_for_check
                    .get_webview_window("main")
                    .map(|window| is_window_effectively_visible(&window));
                let _ = tx.send(visibility);
            }) {
                eprintln!(
                    "Zlack: Failed to schedule memory saver visibility check: {}",
                    e
                );
                invisible_since = None;
                continue;
            }

            match rx.recv() {
                Ok(Some(true)) => {
                    if invisible_since.take().is_some() {
                        eprintln!("Zlack: Window effectively visible; clearing memory saver destroy timer");
                    }
                }
                Ok(Some(false)) => {
                    let since = invisible_since.get_or_insert_with(|| {
                        eprintln!(
                            "Zlack: Window not effectively visible; arming 3-minute destroy timer"
                        );
                        Instant::now()
                    });

                    if since.elapsed() >= OCCLUDED_MEMORY_SAVER_DELAY {
                        let app_for_destroy = app.clone();
                        if let Err(e) = app.run_on_main_thread(move || {
                            if let Some(window) = app_for_destroy.get_webview_window("main") {
                                if is_window_effectively_visible(&window) {
                                    return;
                                }

                                eprintln!("Zlack: Destroying occluded webview after 3 minutes");
                                destroy_window(&window);
                            }
                        }) {
                            eprintln!("Zlack: Failed to schedule occluded webview destroy: {}", e);
                        }
                    }
                }
                Ok(None) | Err(_) => {
                    invisible_since = None;
                }
            }
        }
    });
}

fn main() {
    let memory_saver_state: SharedMemorySaverState =
        Arc::new(Mutex::new(MemorySaverState::default()));
    let is_quitting = Arc::new(AtomicBool::new(false));

    tauri::Builder::default()
        .manage(Arc::clone(&memory_saver_state))
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            restore_or_create_main_window(app);
        }))
        .setup({
            let is_quitting = Arc::clone(&is_quitting);
            move |app| {
                install_app_menu(app)?;
                let show = MenuItem::with_id(app, "show", "Show Zlack", true, None::<&str>)?;
                let quit = MenuItem::with_id(app, "quit", "Quit Zlack", true, None::<&str>)?;
                let separator = PredefinedMenuItem::separator(app)?;
                let menu = Menu::with_items(app, &[&show, &separator, &quit])?;

                TrayIconBuilder::new()
                    .menu(&menu)
                    .show_menu_on_left_click(false)
                    .on_menu_event({
                        let is_quitting = Arc::clone(&is_quitting);
                        move |app, event| match event.id().as_ref() {
                            "quit" => {
                                is_quitting.store(true, Ordering::SeqCst);
                                app.exit(0);
                            }
                            "show" => restore_or_create_main_window(app),
                            _ => {}
                        }
                    })
                    .on_tray_icon_event(|tray, event| {
                        if let TrayIconEvent::Click {
                            button: MouseButton::Left,
                            button_state: MouseButtonState::Up,
                            ..
                        } = event
                        {
                            restore_or_create_main_window(tray.app_handle());
                        }
                    })
                    .build(app)?;

                create_main_window(app.handle())?;
                start_memory_saver_visibility_monitor(app.handle().clone());
                Ok(())
            }
        })
        .on_window_event({
            let is_quitting = Arc::clone(&is_quitting);
            move |window, event| match event {
                WindowEvent::CloseRequested { api, .. } => {
                    if is_quitting.load(Ordering::SeqCst) {
                        return;
                    }
                    api.prevent_close();
                    if let Err(e) = window.hide() {
                        eprintln!("Zlack: Failed to hide window before delayed destroy: {}", e);
                    }
                    schedule_memory_saver_destroy_after(
                        window.app_handle().clone(),
                        HIDDEN_MEMORY_SAVER_DELAY,
                        true,
                    );
                }
                WindowEvent::Resized(_) => {
                    if matches!(window.is_minimized(), Ok(true)) {
                        schedule_memory_saver_destroy_after(
                            window.app_handle().clone(),
                            HIDDEN_MEMORY_SAVER_DELAY,
                            true,
                        );
                    }
                }
                WindowEvent::Focused(false) => {
                    schedule_memory_saver_destroy_after(
                        window.app_handle().clone(),
                        OCCLUDED_MEMORY_SAVER_DELAY,
                        true,
                    );
                }
                WindowEvent::Focused(true) => {
                    cancel_memory_saver_destroy(window.app_handle());
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            notify,
            open_external_url,
            start_window_drag,
            toggle_window_maximize
        ])
        .build(tauri::generate_context!())
        .expect("error while building tauri application")
        .run({
            let is_quitting = Arc::clone(&is_quitting);
            move |app, event| match event {
                tauri::RunEvent::ExitRequested { api, .. }
                    if !is_quitting.load(Ordering::SeqCst) =>
                {
                    api.prevent_exit();
                }
                tauri::RunEvent::Reopen {
                    has_visible_windows: false,
                    ..
                } => {
                    restore_or_create_main_window(app);
                }
                _ => {}
            }
        });
}
