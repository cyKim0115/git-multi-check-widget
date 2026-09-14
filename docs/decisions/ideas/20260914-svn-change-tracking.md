# Idea Evaluation

- Date: 2026-09-14
- Idea id: `20260914-svn-change-tracking`
- Status: `decided`
- Verdict: `ADOPT_WITH_CHANGES`
- Related: `docs/references/assets/20260811-git-multi-check-widget/system-spec.md`

## Proposal (user)

- 현재 Git 레포만 추적하는 위젯에 **SVN 작업본(working copy)의 변경사항도 함께 추적**하는 기능을 넣고 싶다.
- 판단(채택 여부)을 system-crew 절차로 받고 싶다.

## Context

- Goal of this pass: 아이디어 판정만. 판정 전 본구현 없음.
- Constraints
  - Tauri 2 + Rust 백엔드, VCS를 **CLI 프로세스 호출**로 조회 (`git_scan.rs`의 `run_git`).
  - 스캔은 **순차**(`scan_repos`의 `entries.iter().map`) — 레포 수 × 네트워크 시간이 누적된다.
  - 데이터 모델 `RepoStatus`가 git 분산 모델 전제(`ahead`/`behind`/`@{u}`).
  - 설정 파일 `repos.json`이 이미 사용자 머신(`%LOCALAPPDATA%\GitMultiCheckWidget`)에 존재 → 스키마 변경 시 하위 호환 필요.
  - 검증 시점 머신 상태: TortoiseSVN GUI(`TortoiseProc.exe`, `SubWCRev.exe`) 설치됨. `svn.exe` CLI는 최초 평가 시 없었으나 **판정 당일 1.14.5 설치 완료** — `svn --version` 동작 확인.

## Scores

| Axis | Result | Evidence |
|------|--------|----------|
| Feasibility | **Pass** | `svn status` / `svn info`는 git과 동일하게 CLI 호출로 얻을 수 있어 `run_git` 패턴을 그대로 복제하면 된다. 최초 평가 시 걸림돌이던 CLI 부재는 **2026-09-14 `svn.exe` 1.14.5 설치로 해소**(`C:/Program Files/TortoiseSVN/bin`, Machine PATH 등록 확인). 남은 미지수는 원격 대비 최신 여부(`svn status -u`)가 대형 작업본에서 git fetch보다 느릴 수 있다는 점뿐이다. |
| Direction fit | **Pass** | 제품 목표는 "여러 저장소 상태를 한 곳에서 파악"이고, 실사용 환경(Unity 게임 프로젝트)은 SVN 비중이 크다. 위젯 셸·행 UI·폴링·우클릭 메뉴를 그대로 재사용하므로 아키텍처 충돌이 없다. 다만 `RepoStatus`에 VCS 종류 축이 추가되는 **모델 확장**은 불가피하다. |
| Efficiency | **Partial** | 기존 행 UI·요약 헤더를 100% 재사용하면 비용이 낮다. 반대로 SVN을 별도 모델·별도 UI로 병렬 구축하면 비용이 급증한다. 또 순차 스캔에 느린 SVN 네트워크 조회가 끼면 **전체 위젯 갱신이 함께 늘어진다**. |

## Git ↔ SVN 개념 매핑 (판정 근거)

| 개념 | Git | SVN | 위젯 처리 |
|------|-----|-----|-----------|
| 로컬 변경 | `status --porcelain` 줄 수 | `svn status` 줄 수 | 동일하게 `changed_count` |
| push 필요(ahead) | `rev-list @{u}..HEAD` | **개념 없음** (커밋 = 즉시 서버 반영) | 항상 0, 배지에 push 미표시 |
| pull 필요(behind) | `rev-list HEAD..@{u}` | BASE rev < HEAD rev | `behind`에 리비전 차이 |
| 브랜치 | `branch --show-current` | 작업본 URL 경로 (`^/trunk`) | `branch` 자리에 relative-url |
| 원격 조회 | `fetch origin --prune` | `svn status -u` / `svn info -r HEAD` | 폴링 시 1회 |

핵심: **SVN에는 ahead가 없다.** 이 차이를 무시하고 같은 배지를 쓰면 표시가 거짓말을 하게 된다.

## Alternatives considered

| Alternative | Pros | Cons | Better when |
|-------------|------|------|-------------|
| (사용자 제안) SVN 추적 기능 추가 | 실사용 가치 큼, 셸·UI 재사용 | 모델에 VCS 축 추가, CLI 선행 설치, 성능 리스크 | 지금 (수정안 적용 시) |
| A. `kind` 필드로 한 목록에 통합 (**채택**) | 행·요약·폴링·메뉴 전부 재사용, 변경 최소 | `RepoStatus` 일부 필드가 SVN에서 무의미 | VCS별 표시 차이가 배지 수준일 때 |
| B. SVN 전용 목록·전용 UI 분리 | 각 VCS에 최적 표현 | UI·상태·설정 이중화, 유지비 2배 | SVN 전용 기능(lock, externals)까지 갈 때 |
| C. TSVNCache / SubWCRev 연동 | 네트워크 없이 빠른 로컬 상태 | 공개 API 아님, 원격 대비 behind 판정 불가 | 로컬 dirty만 필요할 때 |
| D. 보류(git 전용 유지) | 비용 0 | 실사용 환경의 절반을 못 본다 | SVN 사용 빈도가 낮을 때 |

## Decision

- **Verdict: `ADOPT_WITH_CHANGES`**
- 방향은 수용한다. 단 아래 수정안을 전제로 한다.

### Modified approach

1. **한 목록 통합**: `RepoEntry`에 `kind: git | svn` 추가. `#[serde(default)]`로 기존 `repos.json`은 git으로 읽힌다(하위 호환).
2. **스캔 디스패치**: `svn_scan.rs`를 신설하되 결과는 기존 `RepoStatus`로 맞춘다. `scan_repos`가 `kind`로 분기.
3. **의미 없는 필드 처리**: SVN은 `ahead` 항상 0, `sync_state`는 `synced | behind | error`만 사용. `no_upstream`/`diverged`는 SVN에서 내보내지 않는다.
4. **CLI 부재 대응**: 개발 머신엔 설치됐지만 배포본 사용자에겐 없을 수 있다. git의 `GIT_NOT_FOUND` 패턴처럼 `svn.exe`를 못 찾으면 해당 행만 `SVN_NOT_FOUND` 오류로 표시하고 위젯 전체는 계속 동작한다. `git_program()`처럼 PATH에서 `svn.exe`를 직접 찾아 `CREATE_NO_WINDOW`로 띄운다.
5. **성능 가드**: SVN 원격 조회는 타임아웃을 두고, 순차 스캔이 늘어지면 스캔 병렬화를 **선행 과제**로 먼저 처리한다.
6. **열기 대상**: SVN 행에서는 Fork/Git Bash를 숨기고 탐색기를 폴백으로 둔다 (TortoiseSVN 연동은 후속).

### What we will do now

- 이 판정 기록과 INDEX 등재. 구현은 사용자 확인 후 Systems Analyst 단계(스펙 보강)부터.

### What we will not do

- SVN commit/update 실행
- externals·lock·changelist 등 SVN 고유 세부 상태
- SVN 전용 UI·전용 설정 목록
- 판정 확인 전 코드 변경

## Follow-up

- Spec / implementation owner: Systems Analyst (spec 0.2 — VCS 축 추가) → Implementer
- 선행 과제: (a) ~~`svn.exe` 확보~~ → **해소** (1.14.5, `C:/Program Files/TortoiseSVN/bin/svn.exe`, PATH 등록됨), (b) 스캔 병렬화 여부 결정 — **미결**
- Revisit when: n/a (`ADOPT_WITH_CHANGES`)
- Logged in INDEX: yes
