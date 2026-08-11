# Reference Brief

- Date: 2026-08-11
- Similarity target: **inspired** (UI 셸은 `cursor-usage-widget`, 데이터는 Git 상태)
- Asset id: `20260811-git-multi-check-widget`
- Sources: 사용자 요청; `cursor-usage-widget` 구현 히스토리; `rag` git 스크립트

## User callouts

| # | Callout | Intent |
|---|---------|--------|
| 1 | 레포 여러 개 등록 | 설정 파일로 경로 목록 관리 |
| 2 | 로컬 체인지 확인 | uncommitted / staged 변경 감지 |
| 3 | 푸시할 것 / 풀 받을 것 확인 | upstream 대비 ahead·behind |
| 4 | **한 곳에서 여러 레포 상태** | 제품의 1차 가치 — 대시보드 역할 |
| 5 | 비슷한 뷰 | usage 위젯과 같은 플로팅·다크·행 리스트 톤 |

## Observed user verbs

- 등록(설정) → 주기적 확인 → 필요 시 수동 새로고침
- (후순위) 레포 폴더 열기, 경로 복사

## Core loop

1. 설정에서 레포 목록 로드
2. 각 레포에 `git` 명령 실행 (worktree + remote sync)
3. 행 단위로 상태 배지 표시
4. 헤더에 요약 (`N dirty · M push · K pull`)
5. 주기 폴링 + 우클릭 새로고침

## Rules & numbers

| Item | Value | Confidence |
|------|-------|------------|
| 플랫폼 | Windows | seen |
| 스택 | Tauri 2 + Vite + TS + Rust (형제 위젯과 동일) | proposed |
| 폴링 | 300s 기본 | proposed |
| fetch | 새로고침·폴링 시 `git fetch` (타임아웃 15s/레포) | proposed |
| 레포 수 MVP | ~5–15개 (성능 가정) | proposed |
| 설정 경로 | `%LOCALAPPDATA%\GitMultiCheckWidget\repos.json` | proposed |

## Per-repo status model

### Worktree (로컬)

| State | Meaning | Badge 예 |
|-------|---------|----------|
| `clean` | 변경 없음 | (표시 생략 또는 `clean`) |
| `dirty` | `--porcelain` 비어 있지 않음 | `dirty ·3` (변경 항목 수) |

### Sync (원격 대비)

| State | Meaning | Badge 예 |
|-------|---------|----------|
| `synced` | upstream 있고 ahead=0, behind=0 | — |
| `ahead` | push 필요 | `push ·2` |
| `behind` | pull 필요 | `pull ·1` |
| `diverged` | ahead>0 and behind>0 | `↕ diverged` |
| `no_upstream` | tracking branch 없음 | `no upstream` |
| `no_remote` | origin 없음 | `local only` |

### Error

| State | Meaning |
|-------|---------|
| `path_missing` | 경로 없음 |
| `not_git` | `.git` 없음 |
| `git_error` | git 명령 실패 |

**표시 우선순위 (한 배지에 합성):** `error` > `dirty` > `diverged` > `ahead` > `behind` > `clean`  
보조 정보: 브랜치명은 항상 서브캡션으로 표시.

## UI / feedback (inspired by cursor-usage-widget)

```
┌──────────────────────────────────────┐
│ GIT MULTI-CHECK     2 dirty · 1 push │  ← header
│ rag              main    dirty ·3     │  ← row
│ TeenipingTycoon  develop pull ·2     │
│ system-crew      main    clean        │
│ updated 12:48                        │  ← footer
└──────────────────────────────────────┘
```

- 다크 반투명 패널, 둥근 모서리, 드래그 이동
- `dirty` 행: warm tint (usage 위젯 `hot` 색과 동일 계열)
- `ahead`/`behind`: muted vs accent 구분
- 창 높이: `min(520, header+footer+rows*rowHeight)` — 레포 수에 따라 자동

## Explicit non-goals (MVP)

- 위젯에서 `git pull` / `git push` / `commit` 실행
- GitHub API·PR·Actions 상태
- SSH credential UI·인증 관리
- macOS / Linux
- 레포 자동 발견(폴더 스캔) — **수동 등록만**
- 멀티 remote / worktree / submodule 내부 상태

## Adaptation notes

| cursor-usage-widget | git-multi-check-widget |
|---------------------|------------------------|
| Cursor / Other 2 tracks | N repo rows |
| 프로그레스바 fill | 상태 배지 (퍼센트 없음) |
| NeedLogin / FetchError | per-repo error + global git missing |
| usage API 폴링 | git CLI + optional fetch |

## Open questions (구현 전 확인)

1. **fetch 정책** — 폴링마다 fetch 할지, 수동 새로고침만 할지? (제안: 둘 다, 레포별 타임아웃)
2. **기본 등록 목록** — `rag`, `TeenipingTycoon` 시드 OK?
3. **설정 UI** — MVP는 JSON 수동 편집 vs 간단 추가 다이얼로그?

## 형제 레포 구현 히스토리 (참고)

`cursor-usage-widget` 진행 순서:

| Phase | 커밋 | 내용 |
|-------|------|------|
| 0 | `b73cd75` | 스파이크 스크립트로 데이터 경로 검증 |
| 1 | `c5d25c0` | Tauri MVP (듀얼 바 UI) |
| 2 | `b42828d` | 우클릭 컨텍스트 메뉴 |
| 3 | `058bb15` | 시작.bat + LOCALAPPDATA 설치본 |
| 4 | `303e4c0` | 문서·실행 경로 정리 |

동일한 단계 구조를 따른다.
