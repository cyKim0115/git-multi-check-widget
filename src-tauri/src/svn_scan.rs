use crate::config::{RepoEntry, Vcs};
use crate::git_scan::{RepoStatus, SyncState, ValidateResult};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::sync::OnceLock;
use std::time::Duration;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// `svn status -u` walks the tree server-side, so it needs a hard ceiling —
/// a stalled repo must not freeze the whole widget scan.
const REMOTE_TIMEOUT_SEC: u64 = 20;
const LOCAL_TIMEOUT_SEC: u64 = 15;

struct SvnOutput {
    ok: bool,
    stdout: String,
    stderr: String,
}

static SVN_PROGRAM: OnceLock<Option<PathBuf>> = OnceLock::new();

/// TortoiseSVN ships the CLI only when "command line client tools" is selected,
/// so absence is a normal state we surface per row instead of failing the app.
fn svn_program() -> Option<PathBuf> {
    SVN_PROGRAM
        .get_or_init(|| {
            if let Some(path_var) = std::env::var_os("PATH") {
                for dir in std::env::split_paths(&path_var) {
                    let exe = dir.join("svn.exe");
                    if exe.is_file() {
                        return Some(exe);
                    }
                }
            }
            let fallback = PathBuf::from("C:/Program Files/TortoiseSVN/bin/svn.exe");
            if fallback.is_file() {
                return Some(fallback);
            }
            None
        })
        .clone()
}

fn run_svn_blocking(args: &[String], cwd: Option<&Path>) -> SvnOutput {
    let program = match svn_program() {
        Some(p) => p,
        None => {
            return SvnOutput {
                ok: false,
                stdout: String::new(),
                stderr: "SVN_NOT_FOUND".into(),
            }
        }
    };

    let mut cmd = Command::new(program);
    cmd.stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(windows)]
    cmd.creation_flags(CREATE_NO_WINDOW);
    if let Some(dir) = cwd {
        cmd.current_dir(dir);
    }
    // Keep server answers parseable regardless of the user's locale.
    cmd.env("LC_ALL", "C");

    match cmd.args(args).output() {
        Ok(out) => SvnOutput {
            ok: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).trim_end().to_string(),
            stderr: String::from_utf8_lossy(&out.stderr).trim().to_string(),
        },
        Err(e) => SvnOutput {
            ok: false,
            stdout: String::new(),
            stderr: format!("svn spawn failed: {e}"),
        },
    }
}

/// Runs svn on a worker thread so a hung network call times out instead of
/// blocking the scan. `Command::output()` has no timeout of its own.
fn run_svn(args: &[&str], cwd: Option<&Path>, timeout_sec: u64) -> SvnOutput {
    let owned_args: Vec<String> = args.iter().map(|s| (*s).to_string()).collect();
    let owned_cwd = cwd.map(|p| p.to_path_buf());
    let (tx, rx) = mpsc::channel();

    std::thread::spawn(move || {
        let _ = tx.send(run_svn_blocking(&owned_args, owned_cwd.as_deref()));
    });

    match rx.recv_timeout(Duration::from_secs(timeout_sec)) {
        Ok(out) => out,
        Err(_) => SvnOutput {
            ok: false,
            stdout: String::new(),
            stderr: format!("svn timeout ({timeout_sec}s)"),
        },
    }
}

fn error_status(entry: &RepoEntry, branch: Option<String>, message: &str) -> RepoStatus {
    RepoStatus {
        vcs: Vcs::Svn,
        name: entry.name.clone(),
        path: entry.path.clone(),
        branch,
        dirty: false,
        changed_count: 0,
        ahead: 0,
        behind: 0,
        sync_state: SyncState::Error,
        badge: "ERR".into(),
        error: Some(message.to_string()),
    }
}

fn is_noise_line(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.is_empty()
        || trimmed.starts_with("Performing status on external")
        || trimmed.starts_with("Status against revision")
        || trimmed.starts_with("Summary of conflicts")
}

/// Local modifications, counted the same way git's `status --porcelain` rows are.
/// `X` marks an externals definition, not a change, so it is skipped.
fn count_local_changes(stdout: &str) -> u32 {
    stdout
        .lines()
        .filter(|l| !is_noise_line(l))
        .filter(|l| !l.starts_with('X'))
        .count() as u32
}

/// With `-u`, svn marks out-of-date items with `*` in the ninth column.
fn count_out_of_date(stdout: &str) -> u32 {
    stdout
        .lines()
        .filter(|l| !is_noise_line(l))
        .filter(|l| l.chars().nth(8) == Some('*'))
        .count() as u32
}

fn build_badge(dirty: bool, changed: u32, behind: u32) -> String {
    let mut parts = Vec::new();
    if dirty {
        parts.push(format!("dirty ·{changed}"));
    }
    if behind > 0 {
        parts.push(format!("pull ·{behind}"));
    }
    if parts.is_empty() {
        "clean".into()
    } else {
        parts.join(" · ")
    }
}

/// SVN has no branch. A trunk/branches checkout is best described by its
/// relative URL, but a repository-root checkout reports a bare `^/`, which says
/// nothing — there the working revision is the informative label.
fn working_copy_label(path: &Path) -> Option<String> {
    let url_out = run_svn(
        &["info", "--show-item", "relative-url"],
        Some(path),
        LOCAL_TIMEOUT_SEC,
    );
    if url_out.ok {
        let rel = url_out.stdout.trim();
        if !rel.is_empty() && rel != "^/" {
            return Some(rel.to_string());
        }
    }

    let rev_out = run_svn(
        &["info", "--show-item", "revision"],
        Some(path),
        LOCAL_TIMEOUT_SEC,
    );
    if rev_out.ok {
        let rev = rev_out.stdout.trim();
        if !rev.is_empty() {
            return Some(format!("r{rev}"));
        }
    }
    None
}

pub fn is_working_copy(path: &Path) -> bool {
    path.join(".svn").exists()
}

pub fn scan_one(entry: &RepoEntry, do_fetch: bool) -> RepoStatus {
    let path = Path::new(&entry.path);
    if !path.is_dir() {
        return error_status(entry, None, "path not found");
    }
    if svn_program().is_none() {
        return error_status(entry, None, "svn.exe 없음 (CLI 미설치)");
    }
    if !is_working_copy(path) {
        return error_status(entry, None, "SVN 작업본이 아님 (.svn 없음)");
    }

    let branch = working_copy_label(path);

    let status_out = run_svn(&["status"], Some(path), LOCAL_TIMEOUT_SEC);
    if !status_out.ok {
        let message = if status_out.stderr.is_empty() {
            "svn status 실패".to_string()
        } else {
            status_out.stderr.clone()
        };
        return error_status(entry, branch, &message);
    }
    let changed_count = count_local_changes(&status_out.stdout);

    // Only the poll / manual refresh pays for the server round trip.
    let behind = if do_fetch {
        let remote = run_svn(&["status", "-u", "--quiet"], Some(path), REMOTE_TIMEOUT_SEC);
        if remote.ok {
            count_out_of_date(&remote.stdout)
        } else {
            0
        }
    } else {
        0
    };

    let dirty = changed_count > 0;
    // SVN commits go straight to the server, so `ahead` has no meaning here.
    let sync_state = if behind > 0 {
        SyncState::Behind
    } else {
        SyncState::Synced
    };

    RepoStatus {
        vcs: Vcs::Svn,
        name: entry.name.clone(),
        path: entry.path.clone(),
        branch,
        dirty,
        changed_count,
        ahead: 0,
        behind,
        sync_state,
        badge: build_badge(dirty, changed_count, behind),
        error: None,
    }
}

pub fn validate_input(input: &str) -> ValidateResult {
    use crate::config::repo_name_from_path;

    let trimmed = input.trim();
    if trimmed.is_empty() {
        return ValidateResult {
            ok: false,
            message: "경로를 입력하세요.".into(),
            resolved_path: None,
            suggested_name: None,
            remote_url: None,
        };
    }

    if svn_program().is_none() {
        return ValidateResult {
            ok: false,
            message: "svn.exe를 찾을 수 없습니다. TortoiseSVN의 command line client tools를 설치하세요."
                .into(),
            resolved_path: None,
            suggested_name: None,
            remote_url: None,
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
    if !is_working_copy(path) {
        return ValidateResult {
            ok: false,
            message: "SVN 작업본이 아닙니다 (.svn 없음).".into(),
            resolved_path: None,
            suggested_name: Some(repo_name_from_path(trimmed)),
            remote_url: None,
        };
    }

    let url_out = run_svn(&["info", "--show-item", "url"], Some(path), LOCAL_TIMEOUT_SEC);
    if !url_out.ok {
        return ValidateResult {
            ok: false,
            message: format!(
                "svn info 실패: {}",
                if url_out.stderr.is_empty() {
                    "unknown error".to_string()
                } else {
                    url_out.stderr
                }
            ),
            resolved_path: None,
            suggested_name: Some(repo_name_from_path(trimmed)),
            remote_url: None,
        };
    }

    let path_str = path.display().to_string();
    let remote_url = url_out.stdout.trim().to_string();
    ValidateResult {
        ok: true,
        message: format!("유효한 SVN 작업본: {remote_url}"),
        resolved_path: Some(path_str.clone()),
        suggested_name: Some(repo_name_from_path(&path_str)),
        remote_url: if remote_url.is_empty() {
            None
        } else {
            Some(remote_url)
        },
    }
}
