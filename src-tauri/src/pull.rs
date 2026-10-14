//! 위젯 영역 제목줄의 "빠른 풀" 버튼이 레포 하나씩 부르는 실제 동작.
//!
//! Git은 fast-forward를 먼저 시도하고, 갈라진 경우에만 merge한다. 실패한 레포는
//! 손대기 전 상태로 남겨야 하므로 충돌 난 merge는 그 자리에서 abort한다.
//! SVN은 기존 `SVN-Update-All.bat`과 같이 `svn update --accept postpone`이다.

use crate::config::Vcs;
use crate::git_scan::{new_git_command, GitOutput};
use crate::svn_scan::{is_working_copy, run_watched, svn_program};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::{Duration, SystemTime};

/// 레포 하나의 풀 결과. `ok == false`인 것만 결과 팝업에 오른다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullOutcome {
    pub ok: bool,
    /// 성공이면 무엇을 했는지, 실패면 왜 실패했는지.
    pub message: String,
}

/// 결과 팝업 한 줄. 프런트가 실패한 레포만 모아 보낸다.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PullFailure {
    pub vcs: Vcs,
    pub name: String,
    pub path: String,
    pub message: String,
}

/// 긴 git/svn 출력이 팝업을 다 덮지 않도록 자르는 줄 수.
const MAX_DETAIL_LINES: usize = 12;

/// svn update가 출력도, 읽고 쓰는 I/O도 없이 이만큼 지나면 멈춘 것으로 본다.
/// 큰 파일 하나를 받는 동안에도 프로세스의 I/O는 계속 늘어나므로 정상 업데이트는 걸리지 않는다.
const SVN_UPDATE_IDLE: Duration = Duration::from_secs(10 * 60);

fn success(message: impl Into<String>) -> PullOutcome {
    PullOutcome {
        ok: true,
        message: message.into(),
    }
}

fn failure(message: impl Into<String>) -> PullOutcome {
    PullOutcome {
        ok: false,
        message: message.into(),
    }
}

/// 이유 한 줄 + 명령 출력. 출력이 비면 이유만 남긴다.
fn failure_with(reason: &str, detail: String) -> PullOutcome {
    if detail.is_empty() {
        failure(reason)
    } else {
        failure(format!("{reason}\n{detail}"))
    }
}

/// stderr·stdout에서 사람이 읽을 줄만 남긴다. `hint:` 줄은 git이 늘 붙이는 안내라 뺀다.
fn command_detail(stdout: &str, stderr: &str) -> String {
    let lines: Vec<&str> = stderr
        .lines()
        .chain(stdout.lines())
        .map(str::trim_end)
        .filter(|l| !l.trim().is_empty() && !l.trim_start().starts_with("hint:"))
        .collect();
    let mut kept: Vec<String> = lines
        .iter()
        .take(MAX_DETAIL_LINES)
        .map(|l| (*l).to_string())
        .collect();
    if lines.len() > MAX_DETAIL_LINES {
        kept.push(format!("… ({}줄 더)", lines.len() - MAX_DETAIL_LINES));
    }
    kept.join("\n")
}

fn git_detail(out: &GitOutput) -> String {
    command_detail(&out.stdout, &out.stderr)
}

/// 출력을 다듬지 않은 git. `-z` 출력의 맨 앞 공백(` M path`)도 의미가 있어 그대로 둔다.
fn git_untrimmed(path: &Path, args: &[&str]) -> GitOutput {
    let mut cmd = new_git_command();
    // 풀은 사람이 지켜보지 않는 백그라운드 동작이라, 비밀번호·편집기를 기다리며 멈추지 않게 한다.
    cmd.current_dir(path)
        .env("GIT_TERMINAL_PROMPT", "0")
        .env("GIT_MERGE_AUTOEDIT", "no")
        .args(args);
    match cmd.output() {
        Ok(out) => GitOutput {
            ok: out.status.success(),
            stdout: String::from_utf8_lossy(&out.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&out.stderr).into_owned(),
        },
        Err(e) => GitOutput {
            ok: false,
            stdout: String::new(),
            stderr: format!("git spawn failed: {e}"),
        },
    }
}

fn git(path: &Path, args: &[&str]) -> GitOutput {
    let out = git_untrimmed(path, args);
    GitOutput {
        ok: out.ok,
        stdout: out.stdout.trim().to_string(),
        stderr: out.stderr.trim().to_string(),
    }
}

/// Windows는 대소문자를 가리지 않으므로 경로 비교도 그렇게 한다.
fn path_key(p: &str) -> String {
    if cfg!(windows) {
        p.to_lowercase()
    } else {
        p.to_string()
    }
}

/// `git status --porcelain=v1 -z -uall`의 경로들 (바뀐 것·추적 안 되는 것 모두), [`path_key`]로.
fn dirty_paths(path: &Path) -> Option<HashSet<String>> {
    let out = git_untrimmed(path, &["status", "--porcelain=v1", "-z", "-uall"]);
    if !out.ok {
        return None;
    }
    let mut paths = HashSet::new();
    let mut entries = out.stdout.split('\0').filter(|e| !e.is_empty());
    while let Some(entry) = entries.next() {
        let (code, file) = entry.split_at(entry.len().min(3));
        paths.insert(path_key(file));
        // 이름 바꾸기·복사는 원래 경로가 다음 항목으로 한 번 더 온다.
        if code.contains('R') || code.contains('C') {
            if let Some(orig) = entries.next() {
                paths.insert(path_key(orig));
            }
        }
    }
    Some(paths)
}

/// `from`과 `to` 사이에 바뀐 경로. `filter`는 `--diff-filter` 값 (예: 지워진 것만 `D`).
fn paths_changed_between(
    path: &Path,
    from: &str,
    to: &str,
    filter: Option<&str>,
) -> Option<Vec<String>> {
    let filter_arg = filter.map(|f| format!("--diff-filter={f}"));
    let mut args = vec!["diff", "--name-only", "--no-renames", "-z"];
    if let Some(f) = &filter_arg {
        args.push(f);
    }
    args.extend([from, to]);
    let out = git_untrimmed(path, &args);
    out.ok.then(|| {
        out.stdout
            .split('\0')
            .filter(|p| !p.is_empty())
            .map(str::to_string)
            .collect()
    })
}

/// 커밋의 파일 → blob id. 서브모듈(gitlink)은 파일이 아니라 뺀다.
fn tree_blobs(path: &Path, commit: &str) -> Option<HashMap<String, String>> {
    let out = git_untrimmed(path, &["ls-tree", "-r", "-z", "--full-tree", commit]);
    if !out.ok {
        return None;
    }
    Some(
        out.stdout
            .split('\0')
            .filter_map(|entry| {
                let (meta, file) = entry.split_once('\t')?;
                let mut parts = meta.split(' ');
                let (_mode, kind, id) = (parts.next()?, parts.next()?, parts.next()?);
                (kind == "blob").then(|| (file.to_string(), id.to_string()))
            })
            .collect(),
    )
}

/// 작업 트리 파일들의 blob id를 git 한 번으로 구한다 (`hash-object --stdin-paths`).
fn worktree_blobs(path: &Path, files: &[&String]) -> HashMap<String, String> {
    // 줄 단위로 넘기므로 이름에 줄바꿈이 든 파일은 셀 수 없다 — 그런 파일은 "모름"으로 남는다.
    let files: Vec<&String> = files
        .iter()
        .copied()
        .filter(|f| !f.contains('\n'))
        .collect();
    if files.is_empty() {
        return HashMap::new();
    }
    let mut cmd = new_git_command();
    cmd.current_dir(path)
        .stdin(Stdio::piped())
        .args(["hash-object", "--stdin-paths"]);
    let Ok(mut child) = cmd.spawn() else {
        return HashMap::new();
    };
    let input: String = files.iter().map(|f| format!("{f}\n")).collect();
    if let Some(mut stdin) = child.stdin.take() {
        let _ = stdin.write_all(input.as_bytes());
    }
    let Ok(out) = child.wait_with_output() else {
        return HashMap::new();
    };
    if !out.status.success() {
        return HashMap::new();
    }
    let ids: Vec<String> = String::from_utf8_lossy(&out.stdout)
        .lines()
        .map(str::to_string)
        .collect();
    if ids.len() != files.len() {
        return HashMap::new();
    }
    files.into_iter().cloned().zip(ids).collect()
}

fn head_commit(path: &Path) -> Option<String> {
    let out = git(path, &["rev-parse", "HEAD"]);
    (out.ok && !out.stdout.is_empty()).then_some(out.stdout)
}

/// 시작 전 상태. 실패한 ff·merge가 도중에 써 버린 파일을 가려내는 기준이다.
struct Snapshot {
    head: String,
    /// ff·merge를 시작한 시각. 이보다 오래된 파일은 git이 건드리지 않은 것이다.
    started: SystemTime,
    /// `git status`에 나온 경로 ([`path_key`]) — 사용자의 변경이라 손대지 않는다.
    dirty: HashSet<String>,
    /// 이번 풀이 바꾸는 경로 (HEAD..upstream).
    touched: Vec<String>,
    /// 그중 시작 전에 디스크에 있던 경로. 무시(.gitignore)된 파일도 여기 잡힌다.
    existed: HashSet<String>,
    /// 그 경로들의 상위 폴더 중 시작 전에 있던 것.
    dirs_existed: HashSet<PathBuf>,
}

fn take_snapshot(path: &Path, upstream: &str) -> Option<Snapshot> {
    let head = head_commit(path)?;
    let dirty = dirty_paths(path)?;
    let touched = paths_changed_between(path, &head, upstream, None)?;
    let mut existed = HashSet::new();
    let mut dirs_existed = HashSet::new();
    let mut dirs_seen = HashSet::new();
    for file in &touched {
        if path.join(file).symlink_metadata().is_ok() {
            existed.insert(file.clone());
        }
        for dir in Path::new(file).ancestors().skip(1) {
            if dir.as_os_str().is_empty() || !dirs_seen.insert(dir.to_path_buf()) {
                break;
            }
            if path.join(dir).is_dir() {
                dirs_existed.insert(dir.to_path_buf());
            }
        }
    }
    Some(Snapshot {
        head,
        started: SystemTime::now() - Duration::from_secs(2),
        dirty,
        touched,
        existed,
        dirs_existed,
    })
}

/// 시작 전에 없던 빈 상위 폴더를 지운다 (Unity는 빈 폴더를 보면 새 .meta를 만든다).
fn remove_new_empty_dirs(path: &Path, file: &str, before: &Snapshot) {
    for dir in Path::new(file).ancestors().skip(1) {
        if dir.as_os_str().is_empty() || before.dirs_existed.contains(dir) {
            break;
        }
        if std::fs::remove_dir(path.join(dir)).is_err() {
            break;
        }
    }
}

/// Windows에서 Unity·엑셀이 잡고 있는 파일, LFS 실패 같은 이유로 ff·merge가 **도중에**
/// 멈추면 HEAD는 그대로인데 파일 일부만 upstream 것으로 바뀐 채 남는다. 그 잔해만 되돌린다:
/// 이번 풀이 바꾸는 경로 중 시작 전엔 사용자가 손대지 않았고, 지금은 정확히 upstream
/// 내용인(또는 upstream이 지운 대로 없어진) 파일. 그 밖의 차이는 되돌리지 않고 돌려준다.
fn undo_partial_update(
    path: &Path,
    before: &Snapshot,
    upstream: &str,
) -> Result<usize, Vec<String>> {
    if head_commit(path).as_deref() != Some(before.head.as_str()) {
        return Err(vec!["HEAD가 바뀌어 되돌리지 않았습니다".into()]);
    }
    let (Some(head_tree), Some(up_tree)) =
        (tree_blobs(path, &before.head), tree_blobs(path, upstream))
    else {
        return Err(vec!["상태를 확인하지 못해 되돌리지 않았습니다".into()]);
    };

    // 사용자의 변경, 서브모듈, 시작 전부터 있던 추적 안 되는 파일은 후보가 아니다.
    let candidates: Vec<&String> = before
        .touched
        .iter()
        .filter(|f| !before.dirty.contains(&path_key(f)))
        .filter(|f| head_tree.contains_key(*f) || up_tree.contains_key(*f))
        .filter(|f| head_tree.contains_key(*f) || !before.existed.contains(*f))
        .filter(|f| !path.join(f).is_dir())
        .collect();
    // 시작 뒤에 쓰인 파일만 해시를 잰다. 오래된 파일은 git이 건드리지 않았다.
    let recent: Vec<&String> = candidates
        .iter()
        .copied()
        .filter(|f| {
            path.join(f)
                .symlink_metadata()
                .and_then(|m| m.modified())
                .is_ok_and(|t| t >= before.started)
        })
        .collect();
    let now = worktree_blobs(path, &recent);

    let mut restored = 0;
    let mut left = Vec::new();
    for file in candidates {
        let on_disk = path.join(file).symlink_metadata().is_ok();
        let head_blob = head_tree.get(file);
        let current = if on_disk {
            match now.get(file) {
                Some(id) => Some(id.as_str()),
                // 시작 전 그대로 (최근에 안 쓰였다).
                None if !recent.contains(&file) => head_blob.map(String::as_str),
                None => {
                    left.push(file.clone());
                    continue;
                }
            }
        } else {
            None
        };
        if current == head_blob.map(String::as_str) {
            continue;
        }
        // git이 쓴 것만 되돌린다: upstream 내용 그대로이거나, upstream이 지운 대로 없어진 경우.
        if current != up_tree.get(file).map(String::as_str) {
            left.push(file.clone());
            continue;
        }
        if head_blob.is_some() {
            // 파일 이름의 `*`·`?`가 glob으로 읽혀 다른 파일까지 되돌리지 않게 한다.
            let _ = git(
                path,
                &["--literal-pathspecs", "checkout", "-q", "HEAD", "--", file],
            );
        } else {
            // upstream에만 있는 새 파일 — 시작 전에 없던 것이니 지운다.
            let _ = std::fs::remove_file(path.join(file));
            remove_new_empty_dirs(path, file, before);
        }
        restored += 1;
    }

    // 되돌렸는데도 남은 차이 (아직 잠긴 파일 등).
    if let Some(after) = dirty_paths(path) {
        left.extend(
            before
                .touched
                .iter()
                .filter(|f| after.contains(&path_key(f)) && !before.dirty.contains(&path_key(f)))
                .filter(|f| head_tree.contains_key(*f) || !before.existed.contains(*f))
                .filter(|f| !left.contains(f))
                .cloned()
                .collect::<Vec<_>>(),
        );
    }
    if left.is_empty() {
        Ok(restored)
    } else {
        Err(left)
    }
}

/// ff·merge가 성공해도, 열려 있는 파일은 지우지 못하고 경고만 남긴다 (Unity가 잡은 DLL 등).
/// upstream이 지운 파일이 아직 디스크에 있으면 돌려준다.
fn undeleted_leftovers(path: &Path, before_head: &str) -> Vec<String> {
    paths_changed_between(path, before_head, "HEAD", Some("D"))
        .unwrap_or_default()
        .into_iter()
        .filter(|f| path.join(f).symlink_metadata().is_ok())
        .collect()
}

fn list_paths(paths: &[String]) -> String {
    let mut shown: Vec<String> = paths.iter().take(MAX_DETAIL_LINES).cloned().collect();
    if paths.len() > MAX_DETAIL_LINES {
        shown.push(format!("… ({}개 더)", paths.len() - MAX_DETAIL_LINES));
    }
    shown.join("\n")
}

/// 성공한 풀이라도 지우지 못한 파일이 남았으면 팝업에 오르도록 실패로 알린다.
fn success_unless_leftovers(path: &Path, before: &Snapshot, message: String) -> PullOutcome {
    let leftovers = undeleted_leftovers(path, &before.head);
    if leftovers.is_empty() {
        success(message)
    } else {
        failure(format!(
            "{message} — 받았지만 지우지 못한 파일이 남았습니다. 다른 프로그램이 열고 있을 수 있습니다:\n{}",
            list_paths(&leftovers)
        ))
    }
}

/// 실패 사유 뒤에, 도중에 바뀐 파일을 되돌렸는지 덧붙인다.
fn failure_after_undo(
    path: &Path,
    before: &Snapshot,
    upstream: &str,
    reason: &str,
    detail: String,
) -> PullOutcome {
    let note = match undo_partial_update(path, before, upstream) {
        Ok(0) => None,
        Ok(n) => Some(format!("도중에 바뀐 파일 {n}개는 되돌렸습니다.")),
        Err(left) => Some(format!(
            "도중에 바뀐 파일을 다 되돌리지 못했습니다 — 직접 확인하세요:\n{}",
            list_paths(&left)
        )),
    };
    match note {
        Some(note) => failure_with(&format!("{reason}\n{note}"), detail),
        None => failure_with(reason, detail),
    }
}

/// 워크트리·서브모듈에서는 `.git`이 파일이라 git에게 실제 git 디렉터리를 묻는다.
fn git_dir(path: &Path) -> Option<PathBuf> {
    let out = git(path, &["rev-parse", "--absolute-git-dir"]);
    if out.ok && !out.stdout.is_empty() {
        Some(PathBuf::from(out.stdout))
    } else {
        None
    }
}

/// 사용자가 하던 merge·rebase 위에서 풀하면, 실패 시 abort가 사용자의 작업까지 날린다.
fn operation_in_progress(git_dir: &Path) -> Option<&'static str> {
    const MARKERS: [(&str, &str); 5] = [
        ("MERGE_HEAD", "머지"),
        ("rebase-merge", "리베이스"),
        ("rebase-apply", "리베이스"),
        ("CHERRY_PICK_HEAD", "체리픽"),
        ("REVERT_HEAD", "리버트"),
    ];
    MARKERS
        .iter()
        .find(|(marker, _)| git_dir.join(marker).exists())
        .map(|(_, label)| *label)
}

/// fast-forward를 먼저, 안 되면 merge. 실패하면 레포를 손대기 전 상태로 두고 이유를 돌려준다.
pub fn pull_git(path_str: &str) -> PullOutcome {
    let path = Path::new(path_str);
    if !path.is_dir() {
        return failure("경로를 찾을 수 없습니다.");
    }
    if !path.join(".git").exists() {
        return failure("Git 저장소가 아닙니다 (.git 없음).");
    }
    let Some(git_dir) = git_dir(path) else {
        return failure("Git 디렉터리를 확인하지 못했습니다.");
    };
    if let Some(op) = operation_in_progress(&git_dir) {
        return failure(format!(
            "진행 중인 {op}가 있어 건너뛰었습니다. 먼저 마무리하거나 취소하세요."
        ));
    }

    let upstream = git(path, &["rev-parse", "--abbrev-ref", "@{u}"]);
    if !upstream.ok || upstream.stdout.is_empty() {
        return failure("upstream 브랜치가 없습니다.");
    }
    let upstream = upstream.stdout;

    // 인자 없는 fetch는 현재 브랜치가 추적하는 원격을 받는다 (origin이 아닐 수도 있다).
    let fetch = git(path, &["fetch"]);
    if !fetch.ok {
        return failure_with("fetch 실패", git_detail(&fetch));
    }
    // fetch하는 몇 초 사이에 사용자가 Fork 등에서 merge를 시작했을 수도 있다.
    if let Some(op) = operation_in_progress(&git_dir) {
        return failure(format!(
            "진행 중인 {op}가 있어 건너뛰었습니다. 먼저 마무리하거나 취소하세요."
        ));
    }

    // fetch가 prune하면 upstream이 사라진다. 그걸 "0개 뒤처짐"으로 읽으면 안 된다.
    let upstream_commit = git(
        path,
        &[
            "rev-parse",
            "--verify",
            "-q",
            &format!("{upstream}^{{commit}}"),
        ],
    );
    if !upstream_commit.ok || upstream_commit.stdout.is_empty() {
        return failure(format!(
            "fetch 뒤 {upstream}을(를) 찾을 수 없습니다. 원격 브랜치가 지워졌을 수 있습니다."
        ));
    }
    let upstream_commit = upstream_commit.stdout;
    let behind_out = git(
        path,
        &["rev-list", "--count", &format!("HEAD..{upstream_commit}")],
    );
    let Ok(behind) = behind_out.stdout.parse::<u32>() else {
        return failure_with(
            "뒤처진 커밋 수를 확인하지 못했습니다",
            git_detail(&behind_out),
        );
    };
    if behind == 0 {
        return success("이미 최신");
    }

    let Some(before) = take_snapshot(path, &upstream_commit) else {
        return failure("작업 트리 상태를 확인하지 못했습니다.");
    };

    // `--no-autostash`: merge.autostash 설정이 로컬 변경을 치웠다 다시 얹다 충돌 내고도
    // 성공으로 끝나는 일을 막는다. 덮어쓸 로컬 변경이 있으면 그냥 거절되게 둔다.
    let ff = git(
        path,
        &["merge", "--ff-only", "--no-autostash", &upstream_commit],
    );
    if ff.ok {
        return success_unless_leftovers(path, &before, format!("fast-forward ({behind}개 커밋)"));
    }

    // HEAD가 upstream의 조상인데도 ff가 실패했다면 갈라진 게 아니라 ff 자체가 막힌 것
    // (덮어쓸 로컬 변경, 열려 있는 파일 등)이다. merge로 넘어가도 같은 이유로 막히니 여기서 멈춘다.
    let ancestor = git(
        path,
        &["merge-base", "--is-ancestor", "HEAD", &upstream_commit],
    );
    if ancestor.ok {
        return failure_after_undo(
            path,
            &before,
            &upstream_commit,
            "fast-forward 실패",
            git_detail(&ff),
        );
    }

    // `--ff`는 merge.ff=only 같은 설정보다 "ff 다음 merge"라는 요청을 우선한다.
    // 이름이 아니라 확인해 둔 커밋을 merge한다 — 그 사이 다른 도구가 fetch해도 MERGE_HEAD가
    // 아래 abort 판정과 어긋나지 않게. 메시지는 이름으로 merge했을 때와 같게 준다.
    let message = format!("Merge remote-tracking branch '{upstream}'");
    let merge = git(
        path,
        &[
            "merge",
            "--ff",
            "--no-edit",
            "--no-autostash",
            "-m",
            &message,
            &upstream_commit,
        ],
    );
    if merge.ok {
        return success_unless_leftovers(
            path,
            &before,
            format!("merge ({upstream}의 커밋 {behind}개)"),
        );
    }

    // abort는 이 merge가 남긴 MERGE_HEAD일 때만 한다 — 다른 merge였다면 사용자의 것이다.
    let merge_head = std::fs::read_to_string(git_dir.join("MERGE_HEAD")).unwrap_or_default();
    if merge_head.lines().next().map(str::trim) == Some(upstream_commit.as_str()) {
        let conflicted = format!("{}\n{}", merge.stdout, merge.stderr).contains("CONFLICT");
        let reason = if conflicted {
            "충돌이 나서 머지를 취소했습니다 (merge --abort)."
        } else {
            "머지를 끝내지 못해 취소했습니다 (merge --abort)."
        };
        let abort = git(path, &["merge", "--abort"]);
        let mut detail = git_detail(&merge);
        if !abort.ok {
            detail = format!(
                "{detail}\nmerge --abort 실패 — 저장소를 직접 확인하세요.\n{}",
                git_detail(&abort)
            );
            return failure_with(reason, detail);
        }
        return failure_after_undo(path, &before, &upstream_commit, reason, detail);
    }
    if !merge_head.is_empty() {
        return failure_with(
            "다른 머지가 진행 중이라 손대지 않았습니다",
            git_detail(&merge),
        );
    }

    failure_after_undo(
        path,
        &before,
        &upstream_commit,
        "merge 실패",
        git_detail(&merge),
    )
}

/// `svn update` 출력의 마지막 `Summary of conflicts:` 블록 (충돌·건너뛴 경로 수).
fn svn_conflict_summary(stdout: &str) -> Option<String> {
    let mut lines = stdout.lines();
    lines.find(|l| l.trim_start().starts_with("Summary of conflicts"))?;
    let summary: Vec<String> = lines
        .map(str::trim)
        .take_while(|l| !l.is_empty())
        .map(str::to_string)
        .collect();
    if summary.is_empty() {
        None
    } else {
        Some(summary.join("\n"))
    }
}

/// 업데이트 알림 줄 중 충돌 표시(`C    a.txt`, ` C   a.txt`, `   C a.txt`) — 앞 네 칸이 상태, 다섯째가 공백.
fn is_svn_conflict_notice(line: &str) -> bool {
    let bytes = line.as_bytes();
    bytes.len() > 5
        && bytes[4] == b' '
        && bytes[..4].contains(&b'C')
        && bytes[..4].iter().all(|b| b" ADUCGERB".contains(b))
}

/// 팝업에는 오류·건너뜀·충돌 줄과 충돌 요약만 싣는다. 수천 줄의 `U path`는 뺀다.
fn svn_detail(stdout: &str, stderr: &str) -> String {
    let mut picked = Vec::new();
    let mut in_summary = false;
    for line in stdout.lines() {
        if line.trim_start().starts_with("Summary of conflicts") {
            in_summary = true;
        } else if in_summary && line.trim().is_empty() {
            in_summary = false;
            continue;
        }
        if in_summary
            || line.starts_with("svn:")
            || line.starts_with("Skipped")
            || is_svn_conflict_notice(line)
        {
            picked.push(line);
        }
    }
    command_detail(&picked.join("\n"), stderr)
}

/// `Updated to revision 123.` / `At revision 123.` — externals 줄(`External at revision`)은 빼고 본 작업본 것만.
fn svn_revision(stdout: &str) -> Option<(bool, String)> {
    stdout.lines().rev().find_map(|line| {
        let line = line.trim();
        let (updated, rest) = match line.strip_prefix("Updated to revision ") {
            Some(rest) => (true, rest),
            None => (false, line.strip_prefix("At revision ")?),
        };
        Some((updated, rest.trim_end_matches('.').to_string()))
    })
}

/// `svn update --non-interactive --accept postpone`. SVN에는 abort가 없어 충돌은
/// 작업본에 표시된 채로 남는다 — 그래서 충돌도 실패로 보고해 사용자가 알게 한다.
pub fn update_svn(path_str: &str) -> PullOutcome {
    let path = Path::new(path_str);
    if !path.is_dir() {
        return failure("경로를 찾을 수 없습니다.");
    }
    if svn_program().is_none() {
        return failure("svn.exe 없음 (CLI 미설치)");
    }
    if !is_working_copy(path) {
        return failure("SVN 작업본이 아닙니다 (.svn 없음).");
    }

    let Some(program) = svn_program() else {
        return failure("svn.exe 없음 (CLI 미설치)");
    };
    // 작업본 경로를 인자로 주면 svn이 끝의 `@2024`를 peg revision으로 읽고, ANSI에
    // 없는 글자(`ü` 등)는 비슷한 글자로 바꿔 엉뚱한 작업본을 업데이트한다. 스캔처럼
    // 작업본 안에서 대상 없이 돌린다.
    //
    // 대형 아트 작업본은 몇 분씩 걸려도 정상이고 도중에 끊으면 작업본이 잠기므로 전체
    // 시간 제한은 두지 않는다. 대신 아무 진행이 없을 때만 끊는다 (svn://은 자체 타임아웃이 없다).
    let watched = run_watched(
        &program,
        &["update", "--non-interactive", "--accept", "postpone"],
        path,
        SVN_UPDATE_IDLE,
    );
    let out = watched.output;
    if watched.stalled {
        return failure_with(
            &format!(
                "svn update가 {}분 동안 진행이 없어 멈췄습니다. 작업본이 잠겼으면 TortoiseSVN의 Clean up을 실행하세요.",
                SVN_UPDATE_IDLE.as_secs() / 60
            ),
            svn_detail(&out.stdout, &out.stderr),
        );
    }

    let conflicts = svn_conflict_summary(&out.stdout);
    if !out.ok {
        // 외부 항목 오류처럼 실패로 끝나도 본 작업본은 업데이트되고 충돌이 남을 수 있다.
        let reason = if conflicts.is_some() {
            "svn update 실패 — 충돌도 남았습니다. TortoiseSVN으로 해결하세요."
        } else {
            "svn update 실패"
        };
        return failure_with(reason, svn_detail(&out.stdout, &out.stderr));
    }

    if conflicts.is_some() {
        return failure_with(
            "업데이트는 했지만 충돌이 남았습니다. TortoiseSVN으로 해결하세요.",
            svn_detail(&out.stdout, &out.stderr),
        );
    }

    match svn_revision(&out.stdout) {
        Some((true, rev)) => success(format!("r{rev}로 업데이트")),
        Some((false, rev)) => success(format!("이미 최신 (r{rev})")),
        None => success("업데이트 완료"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::process::Command;
    use std::sync::atomic::{AtomicU32, Ordering};

    static SEQ: AtomicU32 = AtomicU32::new(0);

    /// 테스트마다 비어 있는 임시 폴더. 끝나면 지운다.
    struct Sandbox(PathBuf);

    impl Sandbox {
        fn new(tag: &str) -> Self {
            let dir = std::env::temp_dir().join(format!(
                "rmcw-pull-{tag}-{}-{}",
                std::process::id(),
                SEQ.fetch_add(1, Ordering::SeqCst)
            ));
            let _ = fs::remove_dir_all(&dir);
            fs::create_dir_all(&dir).unwrap();
            Sandbox(dir)
        }

        fn path(&self, name: &str) -> PathBuf {
            self.0.join(name)
        }
    }

    impl Drop for Sandbox {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    /// 사용자 전역 설정(merge.ff, hooks 등)에 휘둘리지 않게 격리한 git.
    fn git_cmd(dir: &Path, args: &[&str]) -> Command {
        let empty_config = std::env::temp_dir().join("rmcw-pull-empty.gitconfig");
        if !empty_config.exists() {
            let _ = fs::write(&empty_config, "");
        }
        let mut cmd = Command::new("git");
        cmd.current_dir(dir)
            .env("GIT_CONFIG_GLOBAL", &empty_config)
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .env("GIT_AUTHOR_NAME", "test")
            .env("GIT_AUTHOR_EMAIL", "test@example.com")
            .env("GIT_COMMITTER_NAME", "test")
            .env("GIT_COMMITTER_EMAIL", "test@example.com")
            .args(args);
        cmd
    }

    fn run(dir: &Path, args: &[&str]) -> String {
        let out = git_cmd(dir, args).output().expect("git");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    fn commit_file(repo: &Path, file: &str, body: &str, msg: &str) {
        fs::write(repo.join(file), body).unwrap();
        run(repo, &["add", file]);
        run(repo, &["commit", "-q", "-m", msg]);
    }

    /// bare 원격 + 로컬 클론(`local`) + 원격에 커밋을 올릴 두 번째 클론(`other`).
    struct Remote {
        sandbox: Sandbox,
    }

    impl Remote {
        fn new(tag: &str) -> Self {
            let sandbox = Sandbox::new(tag);
            let root = sandbox.0.clone();
            run(&root, &["init", "-q", "--bare", "-b", "main", "origin.git"]);
            run(&root, &["clone", "-q", "origin.git", "other"]);
            let other = root.join("other");
            run(&other, &["checkout", "-q", "-b", "main"]);
            commit_file(&other, "a.txt", "base\n", "base");
            run(&other, &["push", "-q", "-u", "origin", "main"]);
            run(&root, &["clone", "-q", "origin.git", "local"]);
            // pull_git은 실제 사용자 설정으로 git을 부르므로, 결과를 흔들 값은 레포 설정으로 고정한다.
            for repo in ["local", "other"] {
                let dir = root.join(repo);
                run(&dir, &["config", "user.name", "test"]);
                run(&dir, &["config", "user.email", "test@example.com"]);
                run(&dir, &["config", "core.autocrlf", "false"]);
            }
            Remote { sandbox }
        }

        fn local(&self) -> PathBuf {
            self.sandbox.path("local")
        }

        fn other(&self) -> PathBuf {
            self.sandbox.path("other")
        }

        /// 원격에 새 커밋을 올려 local을 behind로 만든다.
        fn push_remote(&self, file: &str, body: &str) {
            let other = self.other();
            commit_file(&other, file, body, &format!("remote {file}"));
            run(&other, &["push", "-q"]);
        }
    }

    fn head(repo: &Path) -> String {
        run(repo, &["rev-parse", "HEAD"])
    }

    #[test]
    fn git_fast_forwards_when_only_behind() {
        let r = Remote::new("ff");
        r.push_remote("b.txt", "remote\n");
        let out = pull_git(r.local().to_str().unwrap());
        assert!(out.ok, "{}", out.message);
        assert!(out.message.starts_with("fast-forward"), "{}", out.message);
        assert_eq!(head(&r.local()), head(&r.other()));
    }

    #[test]
    fn git_merges_when_diverged_without_conflict() {
        let r = Remote::new("merge");
        r.push_remote("b.txt", "remote\n");
        commit_file(&r.local(), "c.txt", "local\n", "local c");
        let out = pull_git(r.local().to_str().unwrap());
        assert!(out.ok, "{}", out.message);
        assert!(out.message.starts_with("merge"), "{}", out.message);
        let parents = run(&r.local(), &["rev-list", "--parents", "-n", "1", "HEAD"]);
        assert_eq!(
            parents.split_whitespace().count(),
            3,
            "merge commit expected"
        );
        assert_eq!(
            run(&r.local(), &["log", "-1", "--format=%s"]),
            "Merge remote-tracking branch 'origin/main'"
        );
        assert!(r.local().join("b.txt").exists() && r.local().join("c.txt").exists());
    }

    #[test]
    fn git_aborts_conflicting_merge_and_leaves_repo_as_is() {
        let r = Remote::new("conflict");
        r.push_remote("a.txt", "remote side\n");
        commit_file(&r.local(), "a.txt", "local side\n", "local a");
        let before = head(&r.local());

        let out = pull_git(r.local().to_str().unwrap());
        assert!(!out.ok);
        assert!(out.message.contains("충돌"), "{}", out.message);
        assert_eq!(head(&r.local()), before);
        assert!(!r.local().join(".git").join("MERGE_HEAD").exists());
        assert_eq!(run(&r.local(), &["status", "--porcelain"]), "");
        assert_eq!(
            fs::read_to_string(r.local().join("a.txt")).unwrap(),
            "local side\n"
        );
    }

    #[test]
    fn git_reports_blocked_fast_forward_without_touching_local_changes() {
        let r = Remote::new("dirty");
        r.push_remote("a.txt", "remote side\n");
        fs::write(r.local().join("a.txt"), "uncommitted\n").unwrap();
        let before = head(&r.local());

        let out = pull_git(r.local().to_str().unwrap());
        assert!(!out.ok);
        assert!(
            out.message.starts_with("fast-forward 실패"),
            "{}",
            out.message
        );
        assert_eq!(head(&r.local()), before);
        assert_eq!(
            fs::read_to_string(r.local().join("a.txt")).unwrap(),
            "uncommitted\n"
        );
    }

    #[test]
    fn git_skips_repo_with_merge_already_in_progress() {
        let r = Remote::new("inprogress");
        r.push_remote("a.txt", "remote side\n");
        commit_file(&r.local(), "a.txt", "local side\n", "local a");
        run(&r.local(), &["fetch", "-q"]);
        // 사용자가 직접 시작해 충돌을 풀던 merge.
        let _ = git_cmd(&r.local(), &["merge", "--no-edit", "origin/main"]).output();
        assert!(r.local().join(".git").join("MERGE_HEAD").exists());

        let out = pull_git(r.local().to_str().unwrap());
        assert!(!out.ok);
        assert!(out.message.contains("진행 중인 머지"), "{}", out.message);
        assert!(
            r.local().join(".git").join("MERGE_HEAD").exists(),
            "the user's merge must survive"
        );
    }

    #[test]
    fn git_reports_up_to_date_and_missing_upstream() {
        let r = Remote::new("uptodate");
        let out = pull_git(r.local().to_str().unwrap());
        assert!(out.ok && out.message == "이미 최신", "{}", out.message);

        run(&r.local(), &["checkout", "-q", "-b", "topic"]);
        let out = pull_git(r.local().to_str().unwrap());
        assert!(
            !out.ok && out.message.contains("upstream"),
            "{}",
            out.message
        );
    }

    /// Unity·엑셀처럼 지우기를 막고 파일을 여는 프로그램을 흉내 낸다.
    #[cfg(windows)]
    fn hold_open(file: &Path) -> fs::File {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_SHARE_READ: u32 = 0x1;
        fs::OpenOptions::new()
            .read(true)
            .share_mode(FILE_SHARE_READ)
            .open(file)
            .unwrap()
    }

    #[cfg(windows)]
    #[test]
    fn git_undoes_a_fast_forward_that_stopped_halfway() {
        let r = Remote::new("halfway");
        let other = r.other();
        commit_file(&other, "c.txt", "base c\n", "c");
        run(&other, &["push", "-q"]);
        run(&r.local(), &["pull", "-q", "--ff-only"]);
        // 잠긴 a.txt 말고도 바뀌는 파일(c.txt)과 새 파일(b.txt)이 있어야 잔해가 생긴다.
        fs::write(other.join("a.txt"), "remote a\n").unwrap();
        fs::write(other.join("b.txt"), "remote new\n").unwrap();
        fs::write(other.join("c.txt"), "remote c\n").unwrap();
        run(&other, &["add", "-A"]);
        run(&other, &["commit", "-q", "-m", "remote a b c"]);
        run(&other, &["push", "-q"]);
        let before = head(&r.local());

        let lock = hold_open(&r.local().join("a.txt"));
        let out = pull_git(r.local().to_str().unwrap());
        drop(lock);

        assert!(!out.ok, "{}", out.message);
        assert!(out.message.contains("되돌렸습니다"), "{}", out.message);
        assert_eq!(head(&r.local()), before);
        assert_eq!(
            run(&r.local(), &["status", "--porcelain", "-uall"]),
            "",
            "{}",
            out.message
        );
        assert!(!r.local().join("b.txt").exists());
        assert_eq!(
            fs::read_to_string(r.local().join("c.txt")).unwrap(),
            "base c\n"
        );

        // 잠금이 풀리면 다음 풀은 정상으로 끝난다.
        let out = pull_git(r.local().to_str().unwrap());
        assert!(out.ok, "{}", out.message);
        assert_eq!(head(&r.local()), head(&other));
    }

    /// 잠긴 파일로 멈춘 ff가 새 폴더·무시되던 파일·assume-unchanged 파일까지 걸쳤을 때.
    #[cfg(windows)]
    #[test]
    fn git_undo_spares_what_it_did_not_write() {
        let r = Remote::new("undo-edges");
        let other = r.other();
        fs::write(other.join(".gitignore"), "*.lock.json\n").unwrap();
        fs::write(other.join("au.txt"), "au base\n").unwrap();
        run(&other, &["add", "-A"]);
        run(&other, &["commit", "-q", "-m", "ignore + au"]);
        run(&other, &["push", "-q"]);
        run(&r.local(), &["pull", "-q", "--ff-only"]);
        // 무시되던 사용자 파일, assume-unchanged 파일.
        fs::write(r.local().join("p.lock.json"), "mine\n").unwrap();
        run(
            &r.local(),
            &["update-index", "--assume-unchanged", "au.txt"],
        );

        // upstream: 무시 규칙을 없애고 같은 이름 파일을 추적, au.txt 수정, 새 폴더, 잠길 z.txt 수정.
        fs::write(other.join(".gitignore"), "").unwrap();
        fs::write(other.join("p.lock.json"), "theirs\n").unwrap();
        fs::write(other.join("au.txt"), "au remote\n").unwrap();
        fs::create_dir_all(other.join("NewFolder")).unwrap();
        fs::write(other.join("NewFolder").join("x.cs"), "x\n").unwrap();
        fs::write(other.join("a.txt"), "remote a\n").unwrap();
        run(&other, &["add", "-A"]);
        run(&other, &["commit", "-q", "-m", "remote"]);
        run(&other, &["push", "-q"]);
        let before = head(&r.local());

        let lock = hold_open(&r.local().join("a.txt"));
        let out = pull_git(r.local().to_str().unwrap());
        drop(lock);

        assert!(!out.ok, "{}", out.message);
        assert!(out.message.contains("되돌렸습니다"), "{}", out.message);
        assert_eq!(head(&r.local()), before);
        assert!(!r.local().join("NewFolder").exists(), "{}", out.message);
        assert_eq!(
            fs::read_to_string(r.local().join("au.txt")).unwrap(),
            "au base\n",
            "{}",
            out.message
        );
        assert_eq!(
            fs::read_to_string(r.local().join(".gitignore")).unwrap(),
            "*.lock.json\n"
        );
        // 시작 전부터 있던 파일은 지우지 않는다 (git이 덮어썼다면 내용은 어쩔 수 없다).
        assert!(r.local().join("p.lock.json").exists(), "{}", out.message);
    }

    #[cfg(windows)]
    #[test]
    fn git_reports_a_file_it_could_not_delete_after_pulling() {
        let r = Remote::new("undeleted");
        let other = r.other();
        commit_file(&other, "plugin.dll", "binary\n", "plugin");
        run(&other, &["push", "-q"]);
        run(&r.local(), &["pull", "-q", "--ff-only"]);
        run(&other, &["rm", "-q", "plugin.dll"]);
        run(&other, &["commit", "-q", "-m", "drop plugin"]);
        run(&other, &["push", "-q"]);

        let lock = hold_open(&r.local().join("plugin.dll"));
        let out = pull_git(r.local().to_str().unwrap());
        drop(lock);

        assert!(!out.ok, "{}", out.message);
        assert!(out.message.contains("plugin.dll"), "{}", out.message);
        assert_eq!(
            head(&r.local()),
            head(&other),
            "the pull itself went through"
        );
    }

    #[test]
    fn git_refuses_instead_of_autostashing_local_changes() {
        let r = Remote::new("autostash");
        run(&r.local(), &["config", "merge.autostash", "true"]);
        r.push_remote("a.txt", "remote side\n");
        fs::write(r.local().join("a.txt"), "uncommitted\n").unwrap();

        let out = pull_git(r.local().to_str().unwrap());
        assert!(!out.ok, "{}", out.message);
        assert_eq!(
            fs::read_to_string(r.local().join("a.txt")).unwrap(),
            "uncommitted\n"
        );
        assert_eq!(run(&r.local(), &["stash", "list"]), "");
    }

    #[test]
    fn git_reports_an_upstream_pruned_by_the_fetch() {
        let r = Remote::new("pruned");
        let other = r.other();
        run(&other, &["checkout", "-q", "-b", "topic"]);
        run(&other, &["push", "-q", "-u", "origin", "topic"]);
        run(&r.local(), &["fetch", "-q"]);
        run(
            &r.local(),
            &["checkout", "-q", "-b", "topic", "--track", "origin/topic"],
        );
        commit_file(&other, "t.txt", "topic\n", "topic");
        run(&other, &["push", "-q"]);
        run(&r.local(), &["fetch", "-q"]);
        run(&other, &["push", "-q", "origin", "--delete", "topic"]);
        run(&r.local(), &["config", "fetch.prune", "true"]);

        let out = pull_git(r.local().to_str().unwrap());
        assert!(!out.ok, "{}", out.message);
        assert!(out.message.contains("찾을 수 없습니다"), "{}", out.message);
    }

    #[test]
    fn detail_drops_hints_and_caps_length() {
        let stderr = "hint: noise\nerror: real\n";
        let stdout = (0..20)
            .map(|i| format!("line {i}"))
            .collect::<Vec<_>>()
            .join("\n");
        let detail = command_detail(&stdout, stderr);
        assert!(!detail.contains("hint:"));
        assert!(detail.starts_with("error: real"));
        assert!(detail.ends_with("(9줄 더)"), "{detail}");
    }

    #[test]
    fn svn_output_parsing() {
        let updated = "Updating '.':\nU    a.txt\nUpdated to revision 42.\n";
        assert_eq!(svn_revision(updated), Some((true, "42".into())));
        assert_eq!(svn_conflict_summary(updated), None);

        let externals = "Updating '.':\n\nFetching external item into 'ext':\nExternal at revision 7.\n\nAt revision 42.\n";
        assert_eq!(svn_revision(externals), Some((false, "42".into())));

        let conflicted = "Updating '.':\nC    a.txt\nUpdated to revision 43.\nSummary of conflicts:\n  Text conflicts: 1\n";
        assert_eq!(
            svn_conflict_summary(conflicted).as_deref(),
            Some("Text conflicts: 1")
        );
    }

    #[test]
    fn svn_detail_keeps_only_errors_and_conflicts() {
        let stdout = "Updating '.':\nA    f1.txt\nA    f2.txt\nC    t.txt\n   C tree.txt\nU    u.txt\nUpdated to revision 2.\nSummary of conflicts:\n  Text conflicts: 1\n  Tree conflicts: 1";
        let detail = svn_detail(
            stdout,
            "svn: E205011: Failure occurred processing one or more externals definitions",
        );
        assert_eq!(
            detail,
            "svn: E205011: Failure occurred processing one or more externals definitions\nC    t.txt\n   C tree.txt\nSummary of conflicts:\n  Text conflicts: 1\n  Tree conflicts: 1"
        );
    }

    fn svn_tools() -> Option<(PathBuf, PathBuf)> {
        let svn = svn_program()?;
        let admin = svn.with_file_name("svnadmin.exe");
        admin.is_file().then_some((svn, admin))
    }

    fn svn(svn: &Path, dir: &Path, args: &[&str]) {
        let out = Command::new(svn)
            .current_dir(dir)
            .args(args)
            .output()
            .expect("svn");
        assert!(
            out.status.success(),
            "svn {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    fn file_url(path: &Path) -> String {
        format!("file:///{}", path.display().to_string().replace('\\', "/"))
    }

    #[test]
    fn svn_updates_and_reports_conflicts() {
        let Some((svn_exe, admin)) = svn_tools() else {
            eprintln!("svn / svnadmin not installed — skipped");
            return;
        };
        let sb = Sandbox::new("svn");
        let repo = sb.path("repo");
        let out = Command::new(&admin)
            .arg("create")
            .arg(&repo)
            .output()
            .unwrap();
        assert!(out.status.success());
        let url = file_url(&repo);
        svn(&svn_exe, &sb.0, &["checkout", "-q", &url, "wc"]);
        svn(&svn_exe, &sb.0, &["checkout", "-q", &url, "other"]);
        let wc = sb.path("wc");
        let other = sb.path("other");

        fs::write(other.join("a.txt"), "base\n").unwrap();
        svn(&svn_exe, &other, &["add", "-q", "a.txt"]);
        svn(&svn_exe, &other, &["commit", "-q", "-m", "base"]);

        let out = update_svn(wc.to_str().unwrap());
        assert!(out.ok, "{}", out.message);
        assert_eq!(out.message, "r1로 업데이트");
        assert_eq!(fs::read_to_string(wc.join("a.txt")).unwrap(), "base\n");

        let out = update_svn(wc.to_str().unwrap());
        assert!(out.ok && out.message == "이미 최신 (r1)", "{}", out.message);

        fs::write(other.join("a.txt"), "remote side\n").unwrap();
        svn(&svn_exe, &other, &["commit", "-q", "-m", "remote"]);
        fs::write(wc.join("a.txt"), "local side\n").unwrap();
        let out = update_svn(wc.to_str().unwrap());
        assert!(!out.ok);
        assert!(out.message.contains("Text conflicts: 1"), "{}", out.message);
        assert!(out.message.contains("C    a.txt"), "{}", out.message);
    }

    /// 예전처럼 경로를 인자로 넘기면 `@`는 peg revision으로, `ü`는 `u`로 바뀌어 실패하거나
    /// 옆 작업본을 업데이트했다.
    #[test]
    fn svn_updates_working_copies_with_awkward_names() {
        let Some((svn_exe, admin)) = svn_tools() else {
            eprintln!("svn / svnadmin not installed — skipped");
            return;
        };
        let sb = Sandbox::new("svn-names");
        let repo = sb.path("repo");
        assert!(Command::new(&admin)
            .arg("create")
            .arg(&repo)
            .output()
            .unwrap()
            .status
            .success());
        let url = file_url(&repo);
        svn(&svn_exe, &sb.0, &["checkout", "-q", &url, "author"]);
        let author = sb.path("author");
        fs::write(author.join("a.txt"), "v1\n").unwrap();
        svn(&svn_exe, &author, &["add", "-q", "a.txt"]);
        svn(&svn_exe, &author, &["commit", "-q", "-m", "v1"]);

        // checkout도 경로 인자를 망가뜨리니, 이름이 무난한 곳에 받고 폴더 이름만 바꾼다.
        for (plain, awkward) in [("wc1", "art@2024"), ("wc2", "Müller")] {
            svn(&svn_exe, &sb.0, &["checkout", "-q", &url, plain]);
            fs::rename(sb.path(plain), sb.path(awkward)).unwrap();
        }
        fs::write(author.join("a.txt"), "v2\n").unwrap();
        svn(&svn_exe, &author, &["commit", "-q", "-m", "v2"]);

        for awkward in ["art@2024", "Müller"] {
            let wc = sb.path(awkward);
            let out = update_svn(wc.to_str().unwrap());
            assert!(out.ok, "{awkward}: {}", out.message);
            assert_eq!(out.message, "r2로 업데이트", "{awkward}");
            assert_eq!(
                fs::read_to_string(wc.join("a.txt")).unwrap(),
                "v2\n",
                "{awkward}"
            );
        }
    }
}
