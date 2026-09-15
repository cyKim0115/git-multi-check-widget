mod app_launch;
mod config;
mod git_scan;
mod install;
mod svn_scan;

use install::{
    autostart_disable, autostart_enable, autostart_is_enabled, cleanup_stale_debug_autostart,
    ensure_installed_release,
};

use config::{load_config, save_config, OpenTarget, RepoConfig, RepoEntry};
use git_scan::{scan_one, scan_repos, validate_repo_input, RepoStatus, ScanResult, ValidateResult};
use tauri::{Emitter, Manager, WindowEvent};
use tauri_plugin_window_state::StateFlags;

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
fn scan_one_svn_repo(name: String, path: String, do_fetch: bool) -> RepoStatus {
    let entry = RepoEntry {
        name,
        path,
        url: None,
    };
    svn_scan::scan_one(&entry, do_fetch)
}

#[tauri::command]
fn validate_svn_repo(input: String) -> ValidateResult {
    svn_scan::validate_input(&input)
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

/// The setting lives in the config, but the OS flag has to be pushed onto the
/// live window, so settings applies it right after saving and startup replays it.
#[tauri::command]
fn set_always_on_top(app: tauri::AppHandle, enabled: bool) -> Result<(), String> {
    let window = app
        .get_webview_window("main")
        .ok_or_else(|| "main window not found".to_string())?;
    window.set_always_on_top(enabled).map_err(|e| e.to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        // 위젯 위치만 복원한다. 창 높이는 레포 개수에 따라 프런트가 매번 다시 계산하므로
        // (main.ts의 maybeAutoResizeWindow), 크기까지 저장하면 지난 실행의 레포 개수가
        // 이번 실행을 덮어써 잘못된 높이로 떴다가 줄어드는 깜빡임이 생긴다.
        // 설정 창은 tauri.conf.json의 center 동작을 유지한다.
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(StateFlags::POSITION)
                .with_denylist(&[SETTINGS_LABEL])
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            get_config,
            set_config,
            scan_all_repos,
            scan_one_repo,
            validate_repo,
            scan_one_svn_repo,
            validate_svn_repo,
            get_poll_interval_ms,
            quit_app,
            open_settings_window,
            close_settings_window,
            open_repo,
            resize_main_window,
            is_dev_build,
            enable_autostart,
            disable_autostart,
            is_autostart_enabled,
            set_always_on_top
        ])
        .setup(|app| {
            cleanup_stale_debug_autostart();
            let _ = ensure_installed_release();
            // tauri.conf.json pins the window to always-on-top, so a user who
            // turned it off is restored here before the webview paints.
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_always_on_top(load_config().always_on_top);
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if window.label() != SETTINGS_LABEL {
                return;
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                // 창 X는 취소와 같다. 설정이 즉시 반영해 둔 창 플래그를 되돌릴 기회를 준다.
                let _ = window.emit("settings-cancelled", ());
                let _ = window.hide();
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
