mod app_launch;
mod config;
mod git_scan;
mod install;

use install::{
    autostart_disable, autostart_enable, autostart_is_enabled, cleanup_stale_debug_autostart,
    ensure_installed_release,
};

use config::{load_config, save_config, OpenTarget, RepoConfig, RepoEntry};
use git_scan::{scan_one, scan_repos, validate_repo_input, RepoStatus, ScanResult, ValidateResult};
use tauri::{Emitter, Manager, WindowEvent};

const POLL_INTERVAL_MS: u64 = 300_000;
const SETTINGS_LABEL: &str = "settings";

#[tauri::command]
fn get_config() -> RepoConfig {
    load_config()
}

#[tauri::command]
fn set_config(config: RepoConfig) -> Result<(), String> {
    save_config(&config)
}

#[tauri::command]
fn scan_all_repos() -> ScanResult {
    let config = load_config();
    scan_repos(&config.repos, true)
}

#[tauri::command]
fn scan_one_repo(name: String, path: String, do_fetch: bool) -> RepoStatus {
    let entry = RepoEntry {
        name,
        path,
        url: None,
    };
    scan_one(&entry, do_fetch)
}

#[tauri::command]
fn validate_repo(input: String) -> ValidateResult {
    validate_repo_input(&input)
}

#[tauri::command]
fn get_poll_interval_ms() -> u64 {
    POLL_INTERVAL_MS
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn open_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    let window = app
        .get_webview_window(SETTINGS_LABEL)
        .ok_or_else(|| "settings window not found".to_string())?;
    window.show().map_err(|e| e.to_string())?;
    window.unminimize().map_err(|e| e.to_string())?;
    window.set_focus().map_err(|e| e.to_string())?;
    window
        .emit("settings-open", ())
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn close_settings_window(app: tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window(SETTINGS_LABEL) {
        window.hide().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn open_repo(path: String, open_target: OpenTarget) -> Result<(), String> {
    app_launch::open_repo(&path, open_target)
}

#[tauri::command]
fn is_dev_build() -> bool {
    cfg!(debug_assertions)
}

#[tauri::command]
fn enable_autostart() -> Result<(), String> {
    autostart_enable()
}

#[tauri::command]
fn disable_autostart() -> Result<(), String> {
    autostart_disable()
}

#[tauri::command]
fn is_autostart_enabled() -> Result<bool, String> {
    autostart_is_enabled()
}

#[tauri::command]
fn resize_main_window(app: tauri::AppHandle, height: f64) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    let size = window.outer_size().map_err(|e| e.to_string())?;
    window
        .set_size(tauri::Size::Physical(tauri::PhysicalSize {
            width: size.width,
            height: height.max(160.0) as u32,
        }))
        .map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_config,
            set_config,
            scan_all_repos,
            scan_one_repo,
            validate_repo,
            get_poll_interval_ms,
            quit_app,
            open_settings_window,
            close_settings_window,
            open_repo,
            resize_main_window,
            is_dev_build,
            enable_autostart,
            disable_autostart,
            is_autostart_enabled
        ])
        .setup(|_app| {
            cleanup_stale_debug_autostart();
            let _ = ensure_installed_release();
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != SETTINGS_LABEL {
                return;
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
