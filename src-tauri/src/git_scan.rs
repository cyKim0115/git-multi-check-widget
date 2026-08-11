use crate::config::RepoEntry;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SyncState {
    Synced,
    Ahead,
    Behind,
    Diverged,
    NoUpstream,
    NoRemote,
    Error,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoStatus {
    pub name: String,
    pub path: String,
    pub branch: Option<String>,
    pub dirty: bool,
    pub changed_count: u32,
    pub ahead: u32,
    pub behind: u32,
    pub sync_state: SyncState,
    pub badge: String,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanResult {
    pub repos: Vec<RepoStatus>,
    pub scanned_at: String,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidateResult {
    pub ok: bool,
    pub message: String,
    pub resolved_path: Option<String>,
    pub suggested_name: Option<String>,
    pub remote_url: Option<String>,
}

struct GitOutput {
    ok: bool,
    stdout: String,
    stderr: String,
}

static GIT_PROGRAM: OnceLock<PathBuf> = OnceLock::new();

/// Prefer git.exe on PATH to avoid git.cmd spawning a visible console window.
fn git_program() -> PathBuf {
    GIT_PROGRAM
        .get_or_init(|| {
            if let Some(path_var) = std::env::var_os("PATH") {
                for dir in std::env::split_paths(&path_var) {
                    let exe = dir.join("git.exe");
                    if exe.is_file() {
                        return exe;
                    }
                }
            }
            PathBuf::from("git")
        })
        .clone()
}

fn new_git_command() -> Command {
    let mut cmd = Command::new(git_program());
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    cmd
}

fn run_git(args: &[&str], cwd: Option<&Path>) -> GitOutput {
    let mut cmd = new_git_command();
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    let output = cmd.args(args).output();

    match output {
        Ok(out) => GitOutput {
            ok: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).trim().to_string(),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        },
        Err(e) => GitOutput {
            ok: false,
            stdout: String::new(),
            stderr: format!("git spawn failed: {e}"),
        },
    }
}

fn now_unix() -> String {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs().to_string())
        .unwrap_or_else(|_| "0".into())
}

fn build_badge(dirty: bool, changed: u32, ahead: u32, behind: u32, sync: &SyncState) -> String {
    let mut parts = Vec::new();
    if dirty {
        parts.push(format!("dirty ·{changed}"));
    }
    match sync {
        SyncState::Diverged => parts.push(format!("↕ ·{ahead}/{behind}")),
        SyncState::Ahead => parts.push(format!("push ·{ahead}")),
        SyncState::Behind => parts.push(format!("pull ·{behind}")),
        SyncState::NoUpstream => parts.push("no upstream".into()),
        SyncState::NoRemote => parts.push("local only".into()),
        SyncState::Error => parts.push("sync err".into()),
        SyncState::Synced if !dirty => return "clean".into(),
        SyncState::Synced => {}
    }
    if parts.is_empty() {
        "clean".into()
    } else {
        parts.join(" · ")
    }
}

fn scan_one(entry: &RepoEntry, do_fetch: bool) -> RepoStatus {
    let path = Path::new(&entry.path);
    if !path.is_dir() {
        return RepoStatus {
            name: entry.name.clone(),
            path: entry.path.clone(),
            branch: None,
            dirty: false,
            changed_count: 0,
            ahead: 0,
            behind: 0,
            sync_state: SyncState::Error,
            badge: "ERR".into(),
            error: Some("path not found".into()),
        };
    }

    if !path.join(".git").exists() {
        return RepoStatus {
            name: entry.name.clone(),
            path: entry.path.clone(),
            branch: None,
            dirty: false,
            changed_count: 0,
            ahead: 0,
            behind: 0,
            sync_state: SyncState::Error,
            badge: "ERR".into(),
            error: Some("not a git repo".into()),
        };
    }

    if do_fetch {
        let _ = run_git(&["fetch", "origin", "--prune"], Some(path));
    }

    let branch_out = run_git(&["branch", "--show-current"], Some(path));
    let branch = if branch_out.ok && !branch_out.stdout.is_empty() {
        Some(branch_out.stdout)
    } else {
        let sha = run_git(&["rev-parse", "--short", "HEAD"], Some(path));
        if sha.ok {
            Some(format!("(detached {})", sha.stdout))
        } else {
            None
        }
    };

    let status_out = run_git(&["status", "--porcelain"], Some(path));
    let changed_count = if status_out.ok {
        status_out
            .stdout
            .lines()
            .filter(|l| !l.trim().is_empty())
            .count() as u32
    } else {
        return RepoStatus {
            name: entry.name.clone(),
            path: entry.path.clone(),
            branch,
            dirty: false,
            changed_count: 0,
            ahead: 0,
            behind: 0,
            sync_state: SyncState::Error,
            badge: "ERR".into(),
            error: Some(status_out.stderr),
        };
    };

    let upstream = run_git(&["rev-parse", "--abbrev-ref", "@{u}"], Some(path));
    let (ahead, behind, sync_state) = if !upstream.ok || upstream.stdout.is_empty() {
        (0, 0, SyncState::NoUpstream)
    } else {
        let ahead_out = run_git(&["rev-list", "--count", "@{u}..HEAD"], Some(path));
        let behind_out = run_git(&["rev-list", "--count", "HEAD..@{u}"], Some(path));
        let ahead = ahead_out.stdout.parse().unwrap_or(0);
        let behind = behind_out.stdout.parse().unwrap_or(0);
        let sync = if ahead > 0 && behind > 0 {
            SyncState::Diverged
        } else if ahead > 0 {
            SyncState::Ahead
        } else if behind > 0 {
            SyncState::Behind
        } else {
            SyncState::Synced
        };
        (ahead, behind, sync)
    };

    let dirty = changed_count > 0;
    let badge = build_badge(dirty, changed_count, ahead, behind, &sync_state);

    RepoStatus {
        name: entry.name.clone(),
        path: entry.path.clone(),
        branch,
        dirty,
        changed_count,
        ahead,
        behind,
        sync_state,
        badge,
        error: None,
    }
}

pub fn scan_repos(entries: &[RepoEntry], do_fetch: bool) -> ScanResult {
    let repos: Vec<RepoStatus> = entries.iter().map(|e| scan_one(e, do_fetch)).collect();
    let dirty = repos.iter().filter(|r| r.dirty && r.error.is_none()).count();
    let push = repos
        .iter()
        .filter(|r| matches!(r.sync_state, SyncState::Ahead | SyncState::Diverged))
        .count();
    let pull = repos
        .iter()
        .filter(|r| matches!(r.sync_state, SyncState::Behind | SyncState::Diverged))
        .count();
    let err = repos.iter().filter(|r| r.error.is_some()).count();
    let summary = format!("{dirty} dirty · {push} push · {pull} pull · {err} err");

    ScanResult {
        repos,
        scanned_at: now_unix(),
        summary,
    }
}

pub fn validate_repo_input(input: &str) -> ValidateResult {
    use crate::config::{
        extract_repo_name_from_url, is_git_remote_url, resolve_local_clone_path, repo_name_from_path,
    };

    let trimmed = input.trim();
    if trimmed.is_empty() {
        return ValidateResult {
            ok: false,
            message: "경로 또는 URL을 입력하세요.".into(),
            resolved_path: None,
            suggested_name: None,
            remote_url: None,
        };
    }

    if is_git_remote_url(trimmed) {
        let remote = run_git(&["ls-remote", "--heads", trimmed], None);
        if !remote.ok {
            return ValidateResult {
                ok: false,
                message: format!(
                    "원격 URL을 확인할 수 없습니다: {}",
                    if remote.stderr.is_empty() {
                        "unknown error".into()
                    } else {
                        remote.stderr
                    }
                ),
                resolved_path: None,
                suggested_name: extract_repo_name_from_url(trimmed),
                remote_url: Some(trimmed.to_string()),
            };
        }

        let name = extract_repo_name_from_url(trimmed);
        if let Some(ref repo_name) = name {
            if let Some(local) = resolve_local_clone_path(repo_name) {
                let path_str = local.display().to_string();
                return ValidateResult {
                    ok: true,
                    message: format!("원격 OK · 로컬 클론 발견: {path_str}"),
                    resolved_path: Some(path_str),
                    suggested_name: name,
                    remote_url: Some(trimmed.to_string()),
                };
            }
        }

        return ValidateResult {
            ok: false,
            message: "원격은 유효하지만 로컬 클론을 찾지 못했습니다. Windows 경로를 직접 입력하세요.".into(),
            resolved_path: None,
            suggested_name: name,
            remote_url: Some(trimmed.to_string()),
        };
    }

    let path = Path::new(trimmed);
    if !path.is_dir() {
        return ValidateResult {
            ok: false,
            message: "폴더를 찾을 수 없습니다.".into(),
            resolved_path: None,
            suggested_name: None,
            remote_url: None,
        };
    }
    if !path.join(".git").exists() {
        return ValidateResult {
            ok: false,
            message: "Git 저장소가 아닙니다 (.git 없음).".into(),
            resolved_path: None,
            suggested_name: Some(repo_name_from_path(trimmed)),
            remote_url: None,
        };
    }

    let check = run_git(&["rev-parse", "--is-inside-work-tree"], Some(path));
    if !check.ok || check.stdout != "true" {
        return ValidateResult {
            ok: false,
            message: "Git worktree 확인 실패.".into(),
            resolved_path: None,
            suggested_name: Some(repo_name_from_path(trimmed)),
            remote_url: None,
        };
    }

    let path_str = path.display().to_string();
    ValidateResult {
        ok: true,
        message: format!("유효한 Git 저장소: {path_str}"),
        resolved_path: Some(path_str.clone()),
        suggested_name: Some(repo_name_from_path(&path_str)),
        remote_url: None,
    }
}
