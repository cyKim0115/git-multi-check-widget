use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoEntry {
    pub name: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoConfig {
    pub repos: Vec<RepoEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoStatus {
    pub name: String,
    pub path: String,
    pub branch: Option<String>,
    pub dirty: bool,
    pub changed_count: u32,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub repos: Vec<RepoStatus>,
    pub scanned_at: String,
}

fn user_config_path() -> PathBuf {
    let base = std::env::var("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|_| PathBuf::from("."));
    base.join("GitMultiCheckWidget").join("repos.json")
}

fn default_config_candidates() -> Vec<PathBuf> {
    let mut paths = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        paths.push(cwd.join("config").join("repos.default.json"));
    }
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            paths.push(parent.join("config").join("repos.default.json"));
            paths.push(parent.join("..").join("config").join("repos.default.json"));
        }
    }
    paths
}

fn load_config() -> RepoConfig {
    let user_path = user_config_path();
    if user_path.is_file() {
        if let Ok(text) = std::fs::read_to_string(&user_path) {
            if let Ok(cfg) = serde_json::from_str::<RepoConfig>(&text) {
                return cfg;
            }
        }
    }

    for path in default_config_candidates() {
        if path.is_file() {
            if let Ok(text) = std::fs::read_to_string(&path) {
                if let Ok(cfg) = serde_json::from_str::<RepoConfig>(&text) {
                    return cfg;
                }
            }
        }
    }

    RepoConfig {
        repos: vec![
            RepoEntry {
                name: "rag".into(),
                path: "C:\\Users\\cykim\\repo\\rag".into(),
            },
            RepoEntry {
                name: "TeenipingTycoon".into(),
                path: "C:\\Users\\cykim\\repo\\TeenipingTycoon".into(),
            },
        ],
    }
}

fn run_git(repo_path: &Path, args: &[&str]) -> Result<String, String> {
    let output = Command::new("git")
        .args(["-C"])
        .arg(repo_path)
        .args(args)
        .output()
        .map_err(|e| format!("git spawn failed: {e}"))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
        return Err(if stderr.is_empty() {
            format!("git {:?} failed", args)
        } else {
            stderr
        });
    }

    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

fn scan_one(entry: &RepoEntry) -> RepoStatus {
    let path = PathBuf::from(&entry.path);
    if !path.is_dir() {
        return RepoStatus {
            name: entry.name.clone(),
            path: entry.path.clone(),
            branch: None,
            dirty: false,
            changed_count: 0,
            error: Some("path not found".into()),
        };
    }

    let git_dir = path.join(".git");
    if !git_dir.exists() {
        return RepoStatus {
            name: entry.name.clone(),
            path: entry.path.clone(),
            branch: None,
            dirty: false,
            changed_count: 0,
            error: Some("not a git repo".into()),
        };
    }

    let branch = match run_git(&path, &["branch", "--show-current"]) {
        Ok(b) if b.is_empty() => None,
        Ok(b) => Some(b),
        Err(e) => {
            return RepoStatus {
                name: entry.name.clone(),
                path: entry.path.clone(),
                branch: None,
                dirty: false,
                changed_count: 0,
                error: Some(e),
            };
        }
    };

    let status = match run_git(&path, &["status", "--porcelain"]) {
        Ok(s) => s,
        Err(e) => {
            return RepoStatus {
                name: entry.name.clone(),
                path: entry.path.clone(),
                branch,
                dirty: false,
                changed_count: 0,
                error: Some(e),
            };
        }
    };

    let changed_count = status
        .lines()
        .filter(|line| !line.trim().is_empty())
        .count() as u32;

    RepoStatus {
        name: entry.name.clone(),
        path: entry.path.clone(),
        branch,
        dirty: changed_count > 0,
        changed_count,
        error: None,
    }
}

fn now_iso() -> String {
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    format!("{}", secs)
}

#[tauri::command]
fn scan_repos() -> ScanResult {
    let config = load_config();
    let repos = config.repos.iter().map(scan_one).collect();
    ScanResult {
        repos,
        scanned_at: now_iso(),
    }
}

#[tauri::command]
fn quit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![scan_repos, quit_app])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
