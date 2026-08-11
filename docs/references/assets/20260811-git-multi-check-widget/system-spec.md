# System Spec

- Title: Git Multi-Check Desktop Widget (MVP)
- Spec version: 0.1.0-draft
- Based on brief: `reference-brief.md`
- Similarity: inspired (`cursor-usage-widget` 셸 + Git 상태 행)
- Status: **approved** (2026-08-11)

## Player-facing summary

Windows 바탕화면에 작은 플로팅 창이 떠 있고, 등록해 둔 Git 레포마다 **브랜치명**과 **로컬 변경 / push 필요 / pull 필요** 상태가 한눈에 보인다. 여러 레포를 Cursor·터미널·탐색기를 오가며 확인하는 대신 **한 곳에서 전체 상태**를 파악하는 것이 목적이다.

## Success criteria

1. 사용자가 지정한 **2개 이상** 레포가 동시에 목록에 표시된다.
2. 각 레포에 대해 **dirty 여부**(변경 파일 수)가 맞게 표시된다.
3. upstream이 있는 레포에서 **ahead(push)** / **behind(pull)** / **diverged**가 맞게 표시된다.
4. 헤더 요약(`N dirty · M push · K pull`)이 행 상태와 일치한다.
5. `cursor-usage-widget`과 **같은 톤**의 플로팅 UI(다크·frameless·드래그·우클릭 메뉴)다.
6. 기본 5분 폴링 + 우클릭 **새로고침**이 동작한다.
7. Git 자격 증명·레포 내용이 로그/커밋에 남지 않는다.

## Scope

### In

- Windows 플로팅 위젯 (Tauri 2)
- 레포 목록 설정 (`repos.json`)
- 레포당 상태: branch, dirty count, ahead, behind, error
- `git fetch origin` (폴링/수동 새로고침 시, 레포별 타임아웃)
- UI: repo row list + header summary + footer timestamp
- 우클릭: 새로고침, 종료 (시작프로그램은 Phase 3)
- `시작.bat` → LOCALAPPDATA 설치본 패턴 (usage 위젯과 동일)

### Out (MVP)

- pull/push/commit 실행
- GitHub API / PR / CI
- 레포 폴더 자동 스캔
- 설정 GUI (MVP는 JSON; Phase 2에서 최소 편집)
- macOS / Linux
- submodule·worktree 세부 상태

## Mechanics

### Inputs

- **시스템:** 폴링 타이머, 시작 시 1회 스캔
- **사용자:** 드래그, 우클릭 새로고침·종료
- **설정:** `repos.json` — `{ "repos": [{ "name": "...", "path": "..." }] }`

### State machine / flow

```
AppStart
  → LoadConfig
      ├─ ok → ScanAllRepos → Render → Wait(interval) → ScanAllRepos …
      └─ fail → RenderConfigError (empty list + 안내)

ScanRepo(path):
  1. validate path + .git
  2. branch --show-current
  3. status --porcelain → dirty_count
  4. (optional fetch with timeout)
  5. resolve @{u}
      ├─ none → no_upstream
      └─ ok → rev-list counts → ahead / behind / diverged / synced
  → RepoStatus row

ManualRefresh → ScanAllRepos (fetch=true)
PollTick → ScanAllRepos (fetch=true, shared timeout budget)
```

### Data (tunables)

| Name | Default | Notes |
|------|---------|-------|
| `poll_interval_sec` | `300` | usage 위젯과 동일 |
| `fetch_on_scan` | `true` | false면 로컬 캐시된 remote ref만 |
| `fetch_timeout_sec` | `15` | 레포당 |
| `max_repos` | `30` | 초과 시 경고 |
| `window_width` | `340` | usage와 동일 |
| `row_height` | `36` | |
| `always_on_top` | `true` | |
| `dev_vite_port` | `1421` | usage 1420과 충돌 방지 |

### RepoStatus (Rust → JSON)

```json
{
  "name": "rag",
  "path": "C:\\Users\\cykim\\repo\\rag",
  "branch": "main",
  "dirty": true,
  "changed_count": 3,
  "ahead": 0,
  "behind": 2,
  "sync_state": "behind",
  "error": null
}
```

`sync_state` enum: `synced | ahead | behind | diverged | no_upstream | no_remote | error`

### Git commands (per repo)

| Step | Command |
|------|---------|
| dirty | `git -C <path> status --porcelain` |
| branch | `git -C <path> branch --show-current` |
| upstream | `git -C <path> rev-parse --abbrev-ref @{u}` |
| ahead | `git -C <path> rev-list --count @{u}..HEAD` |
| behind | `git -C <path> rev-list --count HEAD..@{u}` |
| fetch | `git -C <path> fetch origin --prune` |

`git` 미설치 시 전역 `GIT_NOT_FOUND` 상태.

### UI layout (MVP)

```
┌──────────────────────────────────────┐
│ GIT MULTI-CHECK     2 dirty · 1 push │
├──────────────────────────────────────┤
│ {name}          {branch}   {badge}   │  × N
├──────────────────────────────────────┤
│ updated HH:MM                        │
└──────────────────────────────────────┘
```

**Badge 규칙**

| 조건 | Badge |
|------|-------|
| error | `ERR` |
| dirty && ahead | `dirty ·N · push ·M` |
| dirty only | `dirty ·N` |
| diverged | `↕ ·{ahead}/{behind}` |
| ahead only | `push ·N` |
| behind only | `pull ·N` |
| clean synced | `clean` (muted) |
| no_upstream | `no upstream` |

### Context menu (MVP)

- 새로고침
- 종료

Phase 3: 시작프로그램, 설정 열기, 레포 폴더 열기

## Implementation sketch

### Stack (proposed — usage 위젯 승인 패턴 재사용)

**Tauri 2 + Rust (`git` CLI) + Vite + TypeScript**

| Layer | Responsibility |
|-------|----------------|
| `src-tauri/src/git_scan.rs` | RepoStatus, git invocation, timeout |
| `src-tauri/src/config.rs` | repos.json load/save paths |
| `src-tauri/src/lib.rs` | `scan_repos`, `quit_app`, commands |
| `src/main.ts` | render rows, poll, context menu |
| `src/styles.css` | usage 위젯 CSS 변수·패턴 재사용 |

### Vertical slice (minimum demo)

1. `config/repos.default.json`에 2레포
2. `npm run tauri dev` → 두 레포 행 + dirty/push/pull 배지
3. 한 레포에서 파일 수정 → 다음 스캔에 `dirty` 반영

### Phased delivery

| Phase | Goal | Exit criteria |
|-------|------|---------------|
| **0 — Spike** | `scripts/spike_git_status.ps1` | 3개 샘플 레포에서 dirty/ahead/behind 표 출력 PASS |
| **1 — Shell** | Tauri 창 + 빈 리스트 + 드래그 | usage와 동일 창 속성 |
| **2 — MVP** | scan + render + poll + context menu | Success criteria 1–6 |
| **3 — Ship** | 시작.bat, LOCALAPPDATA, autostart | usage `058bb15` 패턴 |
| **4 — Polish** | 설정 UI, 창 높이 auto, 폴더 열기 | 사용자 요청 시 |

### Default seed repos

```json
{
  "repos": [
    { "name": "rag", "path": "C:\\Users\\cykim\\repo\\rag" },
    { "name": "TeenipingTycoon", "path": "C:\\Users\\cykim\\repo\\TeenipingTycoon" }
  ]
}
```

## Edge cases

| Case | Behavior |
|------|----------|
| detached HEAD | branch=`(detached)` + 가능하면 short SHA |
| empty repo / no commits | `error: no commits` |
| fetch 네트워크 실패 | 마지막 로컬 known ref로 ahead/behind; footer `fetch failed` |
| 병렬 스캔 | Rust: `rayon` 또는 순차 + 전체 타임아웃 60s |
| 경로에 공백 | `git -C` 인용 처리 |
| 대소문자 | Windows 경로 그대로 |

## Risks

| Risk | Mitigation |
|------|------------|
| fetch가 느림 | 레포별 타임아웃; 수동 새로고침만 fetch 옵션 (Phase 4) |
| private remote auth 실패 | fetch 실패 시 로컬 기준 표시 + 배지 `fetch failed` |
| 많은 레포 | MVP 30 cap; 순차 스캔 |

## Deferred

- 트레이 아이콘
- 알림 (dirty N개일 때)
- 클릭으로 SourceTree / Cursor 열기
- **등록 저장소 순서 변경 (드래그 정렬)** — 설정 팝업에서 reorder UI

## Approval

- Approved by user: **yes**
- Date: 2026-08-11
- Decisions: 폴링마다 fetch; 시드 rag+TeenipingTycoon; 설정 팝업(경로/URL+테스트+등록)

---

## 다음 단계 (system-crew)

1. **사용자 승인** — 이 스펙 + Open questions 3건
2. **Phase 0 spike** — `scripts/spike_git_status.ps1`
3. **승인 후 Implementer** — Phase 1–2 수직 슬라이스
