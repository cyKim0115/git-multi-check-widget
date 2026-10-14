use crate::config::{RepoEntry, Vcs};
use crate::git_scan::{RepoStatus, SyncState, ValidateResult};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

#[cfg(windows)]
use std::os::windows::process::CommandExt;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// `svn status -u` walks the tree server-side, so it needs a hard ceiling —
/// a stalled repo must not freeze the whole widget scan.
const REMOTE_TIMEOUT_SEC: u64 = 20;
const LOCAL_TIMEOUT_SEC: u64 = 15;

pub(crate) struct SvnOutput {
    pub(crate) ok: bool,
    pub(crate) stdout: String,
    pub(crate) stderr: String,
}

static SVN_PROGRAM: OnceLock<Option<PathBuf>> = OnceLock::new();

/// TortoiseSVN ships the CLI only when "command line client tools" is selected,
/// so absence is a normal state we surface per row instead of failing the app.
pub(crate) fn svn_program() -> Option<PathBuf> {
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

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn MultiByteToWideChar(
        code_page: u32,
        flags: u32,
        multi_byte: *const u8,
        multi_byte_len: i32,
        wide: *mut u16,
        wide_len: i32,
    ) -> i32;
}

/// LC_ALL=C keeps svn's messages in English, but paths still come out in the
/// ANSI code page (CP949 for a Korean `D:\아트`), which UTF-8 decoding garbles.
pub(crate) fn decode_output(bytes: &[u8]) -> String {
    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_string();
    }
    #[cfg(windows)]
    {
        const CP_ACP: u32 = 0;
        if let Ok(len) = i32::try_from(bytes.len()) {
            // SAFETY: both buffers are valid for the lengths passed; the first call only sizes.
            unsafe {
                let wide_len =
                    MultiByteToWideChar(CP_ACP, 0, bytes.as_ptr(), len, std::ptr::null_mut(), 0);
                if wide_len > 0 {
                    let mut wide = vec![0u16; wide_len as usize];
                    let written = MultiByteToWideChar(
                        CP_ACP,
                        0,
                        bytes.as_ptr(),
                        len,
                        wide.as_mut_ptr(),
                        wide_len,
                    );
                    if written > 0 {
                        return String::from_utf16_lossy(&wide[..written as usize]);
                    }
                }
            }
        }
    }
    String::from_utf8_lossy(bytes).into_owned()
}

fn svn_command(program: &Path, args: &[&str], cwd: Option<&Path>) -> Command {
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
    cmd.args(args);
    cmd
}

fn svn_not_found() -> SvnOutput {
    SvnOutput {
        ok: false,
        stdout: String::new(),
        stderr: "SVN_NOT_FOUND".into(),
    }
}

pub(crate) fn run_svn_blocking(args: &[String], cwd: Option<&Path>) -> SvnOutput {
    let Some(program) = svn_program() else {
        return svn_not_found();
    };
    let args: Vec<&str> = args.iter().map(String::as_str).collect();

    match svn_command(&program, &args, cwd).output() {
        Ok(out) => SvnOutput {
            ok: out.status.success(),
            stdout: decode_output(&out.stdout).trim_end().to_string(),
            stderr: decode_output(&out.stderr).trim().to_string(),
        },
        Err(e) => SvnOutput {
            ok: false,
            stdout: String::new(),
            stderr: format!("svn spawn failed: {e}"),
        },
    }
}

/// Result of [`run_watched`]; `stalled` means it was killed for making no progress.
pub(crate) struct WatchedOutput {
    pub(crate) output: SvnOutput,
    pub(crate) stalled: bool,
}

#[cfg(windows)]
#[repr(C)]
#[derive(Default, PartialEq, Eq, Clone, Copy)]
struct IoCounters {
    read_ops: u64,
    write_ops: u64,
    other_ops: u64,
    read_bytes: u64,
    write_bytes: u64,
    other_bytes: u64,
}

#[cfg(windows)]
#[link(name = "kernel32")]
extern "system" {
    fn GetProcessIoCounters(process: *mut std::ffi::c_void, counters: *mut IoCounters) -> i32;
}

/// Everything the process has read or written so far — files (a share for
/// `file://`, `.svn/tmp` while a big file streams in) and sockets alike. A
/// directory listing would not do: NTFS only updates an open file's size there
/// once the file is closed, so one large download looks frozen.
#[cfg(windows)]
fn io_progress(child: &std::process::Child) -> Option<IoCounters> {
    use std::os::windows::io::AsRawHandle;
    let mut counters = IoCounters::default();
    // SAFETY: the handle belongs to a child we still own; the struct matches IO_COUNTERS.
    let ok = unsafe { GetProcessIoCounters(child.as_raw_handle(), &mut counters) };
    (ok != 0).then_some(counters)
}

#[cfg(not(windows))]
fn io_progress(_child: &std::process::Child) -> Option<()> {
    None
}

/// After the process ends, how long to wait for the last output. A killed svn
/// can leave a tunnel agent (svn+ssh) holding the inherited pipe open forever.
const PIPE_DRAIN: Duration = Duration::from_secs(5);

/// Runs a long command without a total time limit — a big update may rightly
/// take many minutes, and killing one midway leaves the working copy locked —
/// but kills it once it has gone `idle` with neither output nor any I/O.
/// (ra_svn has no network timeout of its own, so a server that stops answering
/// would otherwise hang the update forever.)
pub(crate) fn run_watched(
    program: &Path,
    args: &[&str],
    cwd: &Path,
    idle: Duration,
) -> WatchedOutput {
    let mut child = match svn_command(program, args, Some(cwd)).spawn() {
        Ok(child) => child,
        Err(e) => {
            return WatchedOutput {
                output: SvnOutput {
                    ok: false,
                    stdout: String::new(),
                    stderr: format!("spawn failed: {e}"),
                },
                stalled: false,
            }
        }
    };

    let started = Instant::now();
    // Milliseconds since `started` of the last byte read from either pipe.
    let last_output = Arc::new(AtomicU64::new(0));
    let (done_tx, done_rx) = mpsc::channel::<()>();
    let pump = |pipe: Option<Box<dyn Read + Send>>| {
        let last_output = Arc::clone(&last_output);
        let collected = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&collected);
        let done = done_tx.clone();
        std::thread::spawn(move || {
            if let Some(mut pipe) = pipe {
                let mut buf = [0u8; 8192];
                while let Ok(n) = pipe.read(&mut buf) {
                    if n == 0 {
                        break;
                    }
                    if let Ok(mut out) = sink.lock() {
                        out.extend_from_slice(&buf[..n]);
                    }
                    last_output.store(started.elapsed().as_millis() as u64, Ordering::Relaxed);
                }
            }
            let _ = done.send(());
        });
        collected
    };
    let stdout = pump(
        child
            .stdout
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    let stderr = pump(
        child
            .stderr
            .take()
            .map(|p| Box::new(p) as Box<dyn Read + Send>),
    );
    drop(done_tx);

    let mut last_io = io_progress(&child);
    let mut last_activity = Duration::ZERO;
    let mut stalled = false;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break Some(status),
            Ok(None) => {}
            Err(_) => break None,
        }
        std::thread::sleep(Duration::from_millis(250));
        let io = io_progress(&child);
        if io != last_io {
            last_io = io;
            last_activity = started.elapsed();
        }
        let output_at = Duration::from_millis(last_output.load(Ordering::Relaxed));
        let active_at = last_activity.max(output_at);
        if started.elapsed().saturating_sub(active_at) >= idle {
            let _ = child.kill();
            stalled = true;
            break child.wait().ok();
        }
    };

    // Both pumps report when their pipe closes; don't wait past PIPE_DRAIN for it.
    let deadline = Instant::now() + PIPE_DRAIN;
    for _ in 0..2 {
        let left = deadline.saturating_duration_since(Instant::now());
        if done_rx.recv_timeout(left).is_err() {
            break;
        }
    }
    let take = |buf: &Arc<Mutex<Vec<u8>>>| buf.lock().map(|b| b.clone()).unwrap_or_default();
    WatchedOutput {
        output: SvnOutput {
            ok: !stalled && status.is_some_and(|s| s.success()),
            stdout: decode_output(&take(&stdout)).trim_end().to_string(),
            stderr: decode_output(&take(&stderr)).trim().to_string(),
        },
        stalled,
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
        // A tree conflict's description: `      >   local file unversioned, …`
        || (trimmed.starts_with('>') && line.starts_with(' '))
}

/// Status rows only: the `Summary of conflicts:` block that closes the output
/// counts conflicts, not files.
fn status_rows(stdout: &str) -> impl Iterator<Item = &str> {
    stdout
        .lines()
        .take_while(|l| !l.trim_start().starts_with("Summary of conflicts"))
        .filter(|l| !is_noise_line(l))
}

/// Local modifications, counted the same way git's `status --porcelain` rows are.
/// `X` marks an externals definition, not a change, so it is skipped.
fn count_local_changes(stdout: &str) -> u32 {
    status_rows(stdout).filter(|l| !l.starts_with('X')).count() as u32
}

/// With `-u`, svn marks out-of-date items with `*` in the ninth column.
fn count_out_of_date(stdout: &str) -> u32 {
    status_rows(stdout)
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
            message:
                "svn.exe를 찾을 수 없습니다. TortoiseSVN의 command line client tools를 설치하세요."
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

    let url_out = run_svn(
        &["info", "--show-item", "url"],
        Some(path),
        LOCAL_TIMEOUT_SEC,
    );
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn conflict_summary_and_tree_conflict_notes_are_not_changes() {
        let stdout = "C       a.txt\n?       a.txt.mine\n?       a.txt.r1\n?       a.txt.r2\n      C b.txt\n      >   local file unversioned, incoming file add upon update\nSummary of conflicts:\n  Text conflicts: 1\n  Tree conflicts: 1";
        assert_eq!(count_local_changes(stdout), 5);
    }

    #[test]
    fn decodes_utf8_as_is() {
        assert_eq!(decode_output("Updating 'é':".as_bytes()), "Updating 'é':");
    }

    #[cfg(windows)]
    #[test]
    fn decodes_ansi_code_page_paths() {
        #[link(name = "kernel32")]
        extern "system" {
            fn GetACP() -> u32;
        }
        // CP949 bytes for "아트"; only meaningful on a Korean-locale machine.
        if unsafe { GetACP() } == 949 {
            assert_eq!(decode_output(b"D:\\\xbe\xc6\xc6\xae"), "D:\\아트");
        }
    }

    /// powershell.exe is a single process, so killing it also closes the pipes
    /// (a `cmd /c` wrapper would leave its child holding them open).
    #[cfg(windows)]
    fn powershell(script: &str, idle: Duration) -> WatchedOutput {
        let ps = PathBuf::from("powershell.exe");
        run_watched(
            &ps,
            &["-NoProfile", "-NonInteractive", "-Command", script],
            &std::env::temp_dir(),
            idle,
        )
    }

    #[cfg(windows)]
    #[test]
    fn watched_command_is_killed_when_idle() {
        let started = Instant::now();
        let out = powershell("Start-Sleep -Seconds 30", Duration::from_secs(3));
        assert!(out.stalled);
        assert!(!out.output.ok);
        assert!(started.elapsed() < Duration::from_secs(20));
    }

    #[cfg(windows)]
    #[test]
    fn watched_command_keeps_running_while_it_prints() {
        let script = "1..6 | % { [Console]::Out.WriteLine('tick'); [Console]::Out.Flush(); Start-Sleep -Milliseconds 700 }";
        let out = powershell(script, Duration::from_secs(3));
        assert!(!out.stalled, "{}", out.output.stderr);
        assert!(out.output.ok);
        assert_eq!(out.output.stdout.matches("tick").count(), 6);
    }

    /// One big file streaming into a held-open temp file prints nothing and,
    /// in a directory listing, never grows — but the process keeps writing.
    #[cfg(windows)]
    #[test]
    fn watched_command_keeps_running_while_it_writes_silently() {
        let file = std::env::temp_dir().join(format!("rmcw-watch-{}.tmp", std::process::id()));
        let script = format!(
            "$f = [IO.File]::Open('{}', 'Create'); $b = New-Object byte[] 65536; 1..12 | % {{ $f.Write($b, 0, $b.Length); $f.Flush(); Start-Sleep -Milliseconds 500 }}; $f.Close()",
            file.display()
        );
        let out = powershell(&script, Duration::from_secs(3));
        let _ = std::fs::remove_file(&file);
        assert!(!out.stalled, "{}", out.output.stderr);
        assert!(out.output.ok, "{}", out.output.stderr);
    }
}
