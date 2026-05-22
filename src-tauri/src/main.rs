#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use std::{
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, Mutex, MutexGuard,
    },
    thread,
    time::Duration as StdDuration,
};

use tauri::{
    menu::{Menu, MenuBuilder, MenuItem, MenuItemBuilder, PredefinedMenuItem, SubmenuBuilder},
    tray::{MouseButton, MouseButtonState, TrayIconBuilder, TrayIconEvent},
    utils::config::BackgroundThrottlingPolicy,
    Manager, Theme, TitleBarStyle, WebviewUrl, WebviewWindow, WebviewWindowBuilder, WindowEvent,
};
#[cfg(not(target_os = "windows"))]
use tauri_plugin_notification::NotificationExt;

#[cfg(target_os = "windows")]
use tauri_winrt_notification::{Duration, Sound, Toast};

const SLACK_URL: &str = "https://app.slack.com/client";
const MEMORY_SAVER_DELAY: StdDuration = StdDuration::from_secs(30 * 60);
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
                    schedule_memory_saver_destroy(app_handle.clone());
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
    .disable_drag_drop_handler()
    .build()?;

    Ok(window)
}

fn destroy_window(window: &WebviewWindow) {
    if let Err(e) = window.destroy() {
        eprintln!("Zlack: Failed to destroy main window: {}", e);
    }
}

fn destroy_window_on_main_thread(window: WebviewWindow) {
    let dispatcher = window.clone();
    if let Err(e) = dispatcher.run_on_main_thread(move || destroy_window(&window)) {
        eprintln!("Zlack: Failed to schedule main window destroy: {}", e);
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

fn schedule_memory_saver_destroy(app: tauri::AppHandle) {
    let state = shared_memory_saver_state(&app);
    let scheduled_generation = {
        let mut state = lock_memory_saver_state(&state);
        if state.armed {
            return;
        }
        state.generation = state.generation.wrapping_add(1);
        state.armed = true;
        state.generation
    };

    thread::spawn(move || {
        thread::sleep(MEMORY_SAVER_DELAY);

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

        if let Some(window) = app.get_webview_window("main") {
            destroy_window_on_main_thread(window);
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
                    schedule_memory_saver_destroy(window.app_handle().clone());
                }
                WindowEvent::Resized(_) => {
                    if matches!(window.is_minimized(), Ok(true)) {
                        schedule_memory_saver_destroy(window.app_handle().clone());
                    }
                }
                WindowEvent::Focused(true) => {
                    cancel_memory_saver_destroy(window.app_handle());
                }
                _ => {}
            }
        })
        .invoke_handler(tauri::generate_handler![
            notify,
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
