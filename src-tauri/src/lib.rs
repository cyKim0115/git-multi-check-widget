mod config;
mod git_scan;

use config::{load_config, save_config, RepoConfig};
use git_scan::{scan_repos, validate_repo_input, ScanResult, ValidateResult};
use tauri::Manager;

const POLL_INTERVAL_MS: u64 = 300_000;

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
fn resize_main_window(app: tauri::AppHandle, height: f64) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    let size = window
        .outer_size()
        .map_err(|e| e.to_string())?;
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
            validate_repo,
            get_poll_interval_ms,
            quit_app,
            resize_main_window
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
