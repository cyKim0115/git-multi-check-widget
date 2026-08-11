use crate::config::OpenTarget;
use std::path::{Path, PathBuf};
use std::process::Command;

#[cfg(windows)]
use std::os::windows::process::CommandExt;

pub fn open_repo(path: &str, target: OpenTarget) -> Result<(), String> {
    let canonical = std::fs::canonicalize(path).map_err(|e| format!("경로 확인 실패: {e}"))?;
    let path_str = canonical.to_string_lossy().to_string();

    match target {
        OpenTarget::Fork => open_fork(&path_str),
        OpenTarget::Cmd => open_cmd(&path_str),
        OpenTarget::GitBash => open_git_bash(&path_str),
        OpenTarget::Cursor => open_cursor(&path_str),
        OpenTarget::VsCode => open_vscode(&path_str),
        OpenTarget::Explorer => open_explorer(&path_str),
    }
}

fn spawn_visible(cmd: &mut Command) -> Result<(), String> {
    #[cfg(windows)]
    {
        cmd.creation_flags(0);
    }
    cmd.spawn()
        .map_err(|e| format!("프로그램 실행 실패: {e}"))?;
    Ok(())
}

fn spawn_via_cmd_start(executable: &Path, args: &[&str]) -> Result<(), String> {
    let exe = executable.to_string_lossy();
    let mut cmd_args = vec!["/C", "start", "", exe.as_ref()];
    for arg in args {
        cmd_args.push(*arg);
    }
    let mut cmd = Command::new("cmd.exe");
    cmd.args(cmd_args);
    spawn_visible(&mut cmd)
}

fn open_fork(path: &str) -> Result<(), String> {
    let fork_exe = resolve_fork_executable()?;
    let mut cmd = Command::new(&fork_exe);
    cmd.arg(path);
    spawn_visible(&mut cmd)
}

fn open_cmd(path: &str) -> Result<(), String> {
    // New console window, cd to repo root.
    let mut cmd = Command::new("cmd.exe");
    cmd.args(["/C", "start", "cmd.exe", "/K", &format!("cd /d \"{}\"", path)]);
    spawn_visible(&mut cmd)
}

fn open_git_bash(path: &str) -> Result<(), String> {
    let git_bash = resolve_git_bash_executable()?;
    spawn_via_cmd_start(&git_bash, &["--cd", path])
}

fn open_cursor(path: &str) -> Result<(), String> {
    let cursor_exe = resolve_cursor_executable()?;
    let mut cmd = Command::new(&cursor_exe);
    cmd.arg(path);
    spawn_visible(&mut cmd)
}

fn open_vscode(path: &str) -> Result<(), String> {
    if let Some(code_cmd) = resolve_vscode_cli() {
        let mut cmd = Command::new(&code_cmd);
        cmd.arg(path);
        spawn_visible(&mut cmd)
    } else if let Some(code_exe) = resolve_vscode_executable() {
        let mut cmd = Command::new(&code_exe);
        cmd.arg(path);
        spawn_visible(&mut cmd)
    } else {
        Err("VS Code를 찾을 수 없습니다. code CLI 또는 Code.exe 경로를 확인하세요.".into())
    }
}

fn open_explorer(path: &str) -> Result<(), String> {
    let mut cmd = Command::new("explorer.exe");
    cmd.arg(path);
    spawn_visible(&mut cmd)
}

fn resolve_fork_executable() -> Result<PathBuf, String> {
    let candidates = [
        localappdata().map(|p| p.join("Fork").join("Fork.exe")),
        program_files().map(|p| p.join("Fork").join("Fork.exe")),
        program_files_x86().map(|p| p.join("Fork").join("Fork.exe")),
    ];
    find_executable(candidates.into_iter().flatten(), "Fork.exe")
}

fn resolve_git_bash_executable() -> Result<PathBuf, String> {
    let candidates = [
        program_files().map(|p| p.join("Git").join("git-bash.exe")),
        program_files_x86().map(|p| p.join("Git").join("git-bash.exe")),
        program_files().map(|p| p.join("Git").join("bin").join("bash.exe")),
    ];
    find_executable(candidates.into_iter().flatten(), "git-bash.exe")
}

fn resolve_cursor_executable() -> Result<PathBuf, String> {
    let candidates = [
        localappdata().map(|p| p.join("Programs").join("cursor").join("Cursor.exe")),
        localappdata().map(|p| p.join("cursor").join("Cursor.exe")),
        program_files().map(|p| p.join("Cursor").join("Cursor.exe")),
    ];
    if let Ok(path) = which_on_path("cursor") {
        return Ok(path);
    }
    find_executable(candidates.into_iter().flatten(), "Cursor.exe")
}

fn resolve_vscode_cli() -> Option<PathBuf> {
    if let Ok(path) = which_on_path("code") {
        return Some(path);
    }
    localappdata()
        .map(|p| p.join("Programs").join("Microsoft VS Code").join("bin").join("code.cmd"))
        .filter(|p| p.is_file())
}

fn resolve_vscode_executable() -> Option<PathBuf> {
    localappdata()
        .map(|p| p.join("Programs").join("Microsoft VS Code").join("Code.exe"))
        .filter(|p| p.is_file())
}

fn find_executable(
    candidates: impl IntoIterator<Item = PathBuf>,
    label: &str,
) -> Result<PathBuf, String> {
    for candidate in candidates {
        if candidate.is_file() {
            return Ok(candidate);
        }
    }
    Err(format!("{label}을(를) 찾을 수 없습니다."))
}

fn which_on_path(name: &str) -> Result<PathBuf, String> {
    let path_var = std::env::var_os("PATH").ok_or_else(|| "PATH 없음".to_string())?;
    let extensions: Vec<String> = if cfg!(windows) {
        std::env::var("PATHEXT")
            .map(|ext| {
                ext.split(';')
                    .map(|s| s.trim().to_string())
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_else(|_| vec![".EXE".into(), ".CMD".into(), ".BAT".into()])
    } else {
        vec![String::new()]
    };

    for dir in std::env::split_paths(&path_var) {
        for ext in &extensions {
            let candidate = if ext.is_empty() {
                dir.join(name)
            } else {
                dir.join(format!("{}{}", name, ext))
            };
            if candidate.is_file() {
                return Ok(candidate);
            }
        }
    }
    Err(format!("{name} not on PATH"))
}

fn localappdata() -> Option<PathBuf> {
    std::env::var("LOCALAPPDATA").ok().map(PathBuf::from)
}

fn program_files() -> Option<PathBuf> {
    std::env::var("ProgramFiles").ok().map(PathBuf::from)
}

fn program_files_x86() -> Option<PathBuf> {
    std::env::var("ProgramFiles(x86)").ok().map(PathBuf::from)
}
