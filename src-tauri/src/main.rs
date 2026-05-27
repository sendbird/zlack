#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use std::{
    env,
    path::{Path, PathBuf},
    process::Command,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
};

use tauri::{
    menu::{Menu, MenuBuilder, MenuItem, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    utils::config::BackgroundThrottlingPolicy,
    webview::{DownloadEvent, Webview},
    Manager, Theme, TitleBarStyle, Url, WebviewUrl, WebviewWindow, WebviewWindowBuilder,
    WindowEvent,
};
#[cfg(not(target_os = "windows"))]
use tauri_plugin_notification::NotificationExt;

#[cfg(target_os = "windows")]
use tauri_winrt_notification::{Duration, Sound, Toast};

const SLACK_URL: &str = "https://app.slack.com/client";
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
static LAST_DOWNLOAD_PATH: Mutex<Option<PathBuf>> = Mutex::new(None);

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
                || url
                    .host_str()
                    .is_some_and(|host| host.ends_with(".zoom.us") || host.ends_with(".zoom.com"))))
}

fn is_open_downloads_url(url: &Url) -> bool {
    url.scheme() == "zlack" && url.host_str() == Some("open-downloads")
}

fn is_allowed_external_url(url: &str) -> bool {
    let Ok(parsed) = Url::parse(url) else {
        return false;
    };

    matches!(
        parsed.scheme(),
        "http" | "https" | "zoommtg" | "zoomus" | "zoomphonecall"
    )
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
        let mut command = Command::new("rundll32");
        command.args(["url.dll,FileProtocolHandler", url]);
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

fn downloads_dir() -> Option<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        env::var_os("USERPROFILE").map(|home| PathBuf::from(home).join("Downloads"))
    }

    #[cfg(not(target_os = "windows"))]
    {
        env::var_os("HOME").map(|home| PathBuf::from(home).join("Downloads"))
    }
}

fn lock_last_download_path() -> MutexGuard<'static, Option<PathBuf>> {
    match LAST_DOWNLOAD_PATH.lock() {
        Ok(guard) => guard,
        Err(poisoned) => {
            eprintln!("Zlack: Last download path lock poisoned; continuing");
            poisoned.into_inner()
        }
    }
}

fn remember_download_path(path: Option<&Path>) {
    let mut last_download_path = lock_last_download_path();
    *last_download_path = path.map(Path::to_path_buf);
}

fn remembered_download_path() -> Option<PathBuf> {
    lock_last_download_path().clone()
}

fn revealable_download_file(downloads_dir: &Path) -> Option<PathBuf> {
    let candidate = remembered_download_path()?.canonicalize().ok()?;
    if !candidate.is_file() {
        return None;
    }

    let downloads_dir = downloads_dir.canonicalize().ok()?;
    if candidate.starts_with(&downloads_dir) {
        Some(candidate)
    } else {
        None
    }
}

fn open_downloads_folder_with_os() -> Result<(), String> {
    let downloads_dir =
        downloads_dir().ok_or_else(|| "failed to resolve Downloads folder".to_string())?;

    if !downloads_dir.is_dir() {
        return Err(format!(
            "Downloads folder does not exist: {}",
            downloads_dir.display()
        ));
    }

    let reveal_file = revealable_download_file(&downloads_dir);

    #[cfg(target_os = "macos")]
    let mut command = {
        if let Some(file) = &reveal_file {
            let mut command = Command::new("/usr/bin/osascript");
            command.args([
                "-e",
                "on run argv",
                "-e",
                "tell application \"Finder\"",
                "-e",
                "reveal (POSIX file (item 1 of argv) as alias)",
                "-e",
                "activate",
                "-e",
                "end tell",
                "-e",
                "end run",
            ]);
            command.arg(file);
            command
        } else {
            let mut command = Command::new("/usr/bin/open");
            command.args(["-a", "Finder"]);
            command.arg(&downloads_dir);
            command
        }
    };

    #[cfg(target_os = "windows")]
    let mut command = {
        let mut command = Command::new("explorer");
        if let Some(file) = &reveal_file {
            command.arg(format!("/select,{}", file.display()));
        } else {
            command.arg(&downloads_dir);
        }
        command
    };

    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut command = {
        let mut command = Command::new("xdg-open");
        command.arg(
            reveal_file
                .as_deref()
                .and_then(Path::parent)
                .unwrap_or(downloads_dir.as_path()),
        );
        command
    };

    let output = command
        .output()
        .map_err(|error| format!("failed to open Downloads folder: {error}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(format!(
            "failed to open Downloads folder: status={}; stderr={}",
            output.status, stderr
        ));
    }

    #[cfg(target_os = "macos")]
    {
        let _ = Command::new("/usr/bin/osascript")
            .args(["-e", "tell application \"Finder\" to activate"])
            .status();
    }

    Ok(())
}

#[tauri::command]
fn open_downloads_folder() -> Result<(), String> {
    open_downloads_folder_with_os()
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

    let Some(window) = app.get_webview_window("main") else {
        eprintln!("Zlack: no main webview window for shortcut: {shortcut_id}");
        return;
    };

    let js = format!(
        r#"
        (() => {{
            const shortcutId = '{shortcut}';

            try {{
                {runner}
                const result = runZlackSlackShortcutAction(shortcutId);
                console.log('Zlack shortcut result', {{ shortcutId, result, href: window.location.href }});
            }} catch (error) {{
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
    if let Some(window) = app.get_webview_window("main") {
        restore_window(&window);
        return;
    }

    match create_main_window(app) {
        Ok(window) => restore_window(&window),
        Err(e) => eprintln!("Zlack: Failed to create main window: {}", e),
    }
}

fn js_string_literal(value: &str) -> String {
    let mut output = String::from("\"");
    for character in value.chars() {
        match character {
            '\\' => output.push_str("\\\\"),
            '"' => output.push_str("\\\""),
            '\n' => output.push_str("\\n"),
            '\r' => output.push_str("\\r"),
            '\t' => output.push_str("\\t"),
            character if character < ' ' => {
                output.push_str(&format!("\\u{:04x}", character as u32));
            }
            character => output.push(character),
        }
    }
    output.push('"');
    output
}

fn filename_from_download(url: &Url, path: Option<&Path>) -> String {
    if let Some(filename) = path
        .and_then(Path::file_name)
        .map(|filename| filename.to_string_lossy().into_owned())
        .filter(|filename| !filename.is_empty())
    {
        return filename;
    }

    url.path_segments()
        .and_then(|mut segments| segments.next_back())
        .filter(|segment| !segment.is_empty())
        .unwrap_or("file")
        .to_string()
}

fn show_download_toast(window: &Webview, filename: &str, url: &Url, status: &str, success: bool) {
    let js = format!(
        r#"
        (() => {{
            if (typeof window.__zlackShowDownloadToast !== 'function') {{
                window.__zlackShowDownloadToast = (payload) => {{
                    const filename = payload?.filename || 'file';
                    const complete = payload?.status === 'finished';
                    const success = payload?.success !== false;
                    const ext = String(filename).split('.').pop()?.toLowerCase() || '';
                    const kindMap = {{ pdf: 'PDF', png: 'PNG image', jpg: 'JPEG image', jpeg: 'JPEG image', gif: 'GIF image', zip: 'Archive', json: 'JSON' }};
                    const kind = complete ? (success ? (kindMap[ext] || (ext ? ext.toUpperCase() : 'File')) : 'Download failed') : (kindMap[ext] || (ext ? ext.toUpperCase() : 'File'));
                    const iconText = ext === 'pdf' ? 'PDF' : (['png', 'jpg', 'jpeg', 'gif', 'webp'].includes(ext) ? 'IMG' : (ext ? ext.slice(0, 3).toUpperCase() : '01'));

                    document.getElementById('zlack-download-toast')?.remove();
                    const toast = document.createElement('div');
                    toast.id = 'zlack-download-toast';
                    toast.setAttribute('role', 'status');
                    toast.setAttribute('aria-live', 'polite');
	                    Object.assign(toast.style, {{
	                        position: 'fixed',
	                        right: '28px',
	                        bottom: '84px',
	                        width: 'min(380px, calc(100vw - 56px))',
	                        zIndex: '2147483647',
	                        pointerEvents: 'none',
	                        color: '#f8f8f8',
                        fontFamily: 'Slack-Lato, Slack-Averta, -apple-system, BlinkMacSystemFont, "Segoe UI", sans-serif',
                    }});

                    const panel = document.createElement('div');
	                    Object.assign(panel.style, {{
	                        boxSizing: 'border-box',
	                        width: '100%',
	                        padding: '12px 14px 14px',
	                        border: '1px solid rgba(232, 232, 232, .14)',
	                        borderRadius: '13px',
	                        background: 'rgba(35, 38, 42, .76)',
	                        boxShadow: '0 14px 36px rgba(0, 0, 0, .32), inset 0 1px 0 rgba(255, 255, 255, .045)',
	                        backdropFilter: 'blur(18px) saturate(1.12)',
	                        WebkitBackdropFilter: 'blur(18px) saturate(1.12)',
	                    }});

	                    const card = document.createElement('div');
	                    Object.assign(card.style, {{
	                        display: 'grid',
	                        gridTemplateColumns: '42px minmax(0, 1fr)',
	                        alignItems: 'center',
	                        gap: '12px',
	                        minHeight: '56px',
	                        width: '100%',
	                        padding: '8px 12px',
	                        borderRadius: '10px',
	                        background: 'rgba(28, 24, 25, .88)',
	                    }});

                    const icon = document.createElement('div');
                    icon.textContent = iconText;
                    Object.assign(icon.style, {{
	                        position: 'relative',
	                        display: 'grid',
	                        placeItems: 'center',
	                        width: '42px',
	                        height: '42px',
	                        borderRadius: '9px',
	                        background: ext === 'pdf' ? 'linear-gradient(145deg, #ff2d6f, #df1457)' : 'linear-gradient(145deg, #6c6a6d, #4e4c50)',
	                        color: '#fff',
	                        fontFamily: 'ui-monospace, SFMono-Regular, Menlo, monospace',
	                        fontSize: ext === 'pdf' ? '11px' : '13px',
	                        fontWeight: '800',
	                        letterSpacing: '-.04em',
	                    }});

                    const badge = document.createElement('div');
                    badge.textContent = complete ? (success ? '✓' : '!') : '↓';
	                    Object.assign(badge.style, {{
	                        position: 'absolute',
	                        right: '-5px',
	                        bottom: '-5px',
	                        display: 'grid',
	                        placeItems: 'center',
	                        width: '19px',
	                        height: '19px',
	                        borderRadius: '999px',
	                        background: '#f8f8f8',
	                        color: success ? '#1d1c1d' : '#e01e5a',
	                        border: '2px solid rgba(28, 24, 25, .95)',
	                        fontSize: '12px',
	                        fontWeight: '900',
	                    }});
                    icon.appendChild(badge);

                    const copy = document.createElement('div');
                    copy.style.minWidth = '0';
	                    const title = document.createElement('div');
	                    title.textContent = filename;
	                    Object.assign(title.style, {{ overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', fontSize: '14px', lineHeight: '18px', fontWeight: '700', color: '#fff' }});
	                    const subtitle = document.createElement('div');
	                    subtitle.textContent = kind;
	                    Object.assign(subtitle.style, {{ marginTop: '3px', overflow: 'hidden', textOverflow: 'ellipsis', whiteSpace: 'nowrap', fontSize: '13px', lineHeight: '17px', fontWeight: '500', color: 'rgba(255, 255, 255, .88)' }});
	                    copy.append(title, subtitle);
	                    card.append(icon, copy);

	                    const downloads = document.createElement('button');
	                    downloads.type = 'button';
	                    downloads.textContent = 'View all downloads';
	                    Object.assign(downloads.style, {{ pointerEvents: 'auto', margin: '9px 0 0 54px', padding: '0', border: '0', background: 'transparent', color: 'rgba(255, 255, 255, .94)', font: 'inherit', fontSize: '13px', lineHeight: '18px', fontWeight: '500', textAlign: 'left', cursor: 'pointer' }});
	                    downloads.addEventListener('click', (event) => {{
	                        event.preventDefault();
	                        event.stopImmediatePropagation();
	                        window.location.href = 'zlack://open-downloads?source=native-toast&ts=' + Date.now();
	                        const invoke = window.__TAURI__?.core?.invoke || window.__TAURI__?.invoke || window.__TAURI_INTERNALS__?.invoke;
	                        if (typeof invoke === 'function') {{
	                            invoke('open_downloads_folder', {{}}).catch((error) => console.error('Zlack: Failed to open Downloads folder', error));
	                        }}
	                    }});

                    panel.append(card, downloads);
                    toast.append(panel);
                    document.documentElement.appendChild(toast);
                    window.clearTimeout(window.__zlackDownloadToastTimeout);
                    window.__zlackDownloadToastTimeout = window.setTimeout(() => toast.remove(), complete ? 7000 : 5000);
                }};
            }}

            window.__zlackShowDownloadToast({{
                filename: {filename},
                url: {url},
                status: {status},
                success: {success},
            }});
        }})();
        "#,
        filename = js_string_literal(filename),
        url = js_string_literal(url.as_str()),
        status = js_string_literal(status),
        success = success,
    );

    if let Err(error) = window.eval(&js) {
        eprintln!("Zlack: Failed to show download toast: {error}");
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
    .background_throttling(BackgroundThrottlingPolicy::Throttle)
    .inner_size(1200.0, 800.0)
    .resizable(true)
    .initialization_script(include_str!("../preload.js"))
    .on_navigation(|url| {
        if is_open_downloads_url(url) {
            if let Err(error) = open_downloads_folder_with_os() {
                eprintln!("Zlack: Failed to open Downloads folder from navigation: {error}");
            }
            return false;
        }

        if is_zoom_url(url) {
            if let Err(error) = open_external_url_with_os(url.as_str()) {
                eprintln!("Zlack: Failed to open Zoom navigation externally: {error}");
            }
            return false;
        }

        true
    })
    .on_new_window(|url, _features| {
        if is_open_downloads_url(&url) {
            if let Err(error) = open_downloads_folder_with_os() {
                eprintln!("Zlack: Failed to open Downloads folder from new-window: {error}");
            }
            return tauri::webview::NewWindowResponse::Deny;
        }

        if !is_slack_url(&url) {
            if let Err(error) = open_external_url_with_os(url.as_str()) {
                eprintln!("Zlack: Failed to open new-window URL externally: {error}");
            }
            return tauri::webview::NewWindowResponse::Deny;
        }

        tauri::webview::NewWindowResponse::Allow
    })
    .on_download(|window, event| {
        match event {
            DownloadEvent::Requested { url, destination } => {
                let filename = filename_from_download(&url, Some(destination.as_path()));
                eprintln!(
                    "Zlack: download requested: {url}; destination={}",
                    destination.display()
                );
                remember_download_path(Some(destination.as_path()));
                show_download_toast(&window, &filename, &url, "downloading", true);
            }
            DownloadEvent::Finished { url, path, success } => {
                let display_path = if success {
                    path.clone().or_else(remembered_download_path)
                } else {
                    path.clone()
                };
                let filename = filename_from_download(&url, display_path.as_deref());
                eprintln!(
                    "Zlack: download finished: {url}; success={success}; path={}",
                    display_path
                        .as_deref()
                        .map(Path::display)
                        .map(|path| path.to_string())
                        .unwrap_or_else(|| "unknown".to_string())
                );
                if success {
                    if let Some(path) = path.as_deref() {
                        remember_download_path(Some(path));
                    }
                } else {
                    remember_download_path(None);
                }
                show_download_toast(&window, &filename, &url, "finished", success);
            }
            _ => {}
        }

        true
    })
    .disable_drag_drop_handler()
    .build()?;

    Ok(window)
}

fn main() {
    let is_quitting = Arc::new(AtomicBool::new(false));

    tauri::Builder::default()
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
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            notify,
            open_external_url,
            open_downloads_folder,
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
