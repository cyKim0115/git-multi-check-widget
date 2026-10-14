# Idea Evaluation

- Date: 2026-10-14
- Idea id: `20261014-quick-pull-and-section-toggle`
- Status: `decided`
- Verdict: `ADOPT_WITH_CHANGES`
- Related: `docs/decisions/ideas/20260914-svn-change-tracking.md`

## Proposal (user)

1. Git·SVN 두 영역 모두 설정에서 켜고 끌 수 있게 한다 (SVN만 토글이 있었다).
2. **빠른 풀 버튼** — 각 영역 제목줄 맨 오른쪽. pull 받을 저장소가 1개 이상일 때만 보인다.
   - Git: ff 우선, 안 되면 merge. 실패한 저장소는 그대로 두고(충돌이면 abort) 다음으로 넘어간다.
   - 끝나면 실패한 저장소와 메시지만 팝업으로 보여 주고, 풀한 저장소들을 새로고침한다. 전부 성공이면 팝업 없이 새로고침만.
   - SVN: 사용자가 쓰던 `SVN-Update-All.bat`(작업본마다 `svn update --non-interactive --accept postpone`, 실패 시 로그 남기고 건너뜀)과 같은 용도.

## Scores

| Axis | Result | Evidence |
|------|--------|----------|
| Feasibility | **Pass** | 스캔과 같은 CLI 호출 패턴(`git_scan.rs`의 `new_git_command`, `svn_scan.rs`의 `run_svn_blocking`)으로 구현된다. ff·merge·abort 판정은 `merge --ff-only` → `merge-base --is-ancestor` → `merge` → `MERGE_HEAD` 확인으로 결정적으로 나뉜다. |
| Direction fit | **Pass** | "여러 저장소 상태를 한 곳에서" 보는 위젯에서, 본 상태(↓)를 바로 해소하는 동작이라 자연스러운 확장이다. 영역 토글은 기존 SVN 토글과 대칭이다. |
| Efficiency | **Partial** | 행·스캔·우클릭 흐름은 그대로 재사용한다. 다만 풀이 스캔과 같은 저장소에서 동시에 돌면 `fetch`와 `merge`가 경합하므로 프런트에 조정 로직(스캔 중단·대기·재실행)이 필요하다. |

## Decision

- **Verdict: `ADOPT_WITH_CHANGES`** — 요구대로 채택하되 아래를 함께 정한다.

### Modified approach

1. **영역 제목 상시 표시**: 예전엔 Git만 켜졌을 때 `Git` 라벨을 숨겼다. 버튼이 라벨 줄에 살기 때문에, 켜진 영역은 항상 제목줄을 가진다. 둘 다 끄면 안내 문구만 남는다.
2. **설정 키**: `git_enabled`(기본 `true`, `#[serde(default)]`로 기존 `repos.json` 호환). 끄면 목록은 지우지 않고 영역·스캔만 뺀다.
3. **Git 풀 순서** (`pull.rs::pull_git`)
   - 진행 중인 merge/rebase/cherry-pick/revert가 있으면 **건너뛴다** — 실패 시 abort가 사용자의 작업까지 날리지 않게. fetch 뒤에 한 번 더 본다(그 사이 Fork에서 시작한 merge).
   - `fetch`(현재 브랜치가 추적하는 원격) → upstream을 커밋으로 다시 확인(prune으로 사라졌으면 실패) → behind 0이면 "이미 최신".
   - `merge --ff-only --no-autostash` 성공이면 끝. HEAD가 upstream의 조상인데 실패했다면 ff 자체가 막힌 것(덮어쓸 로컬 변경, 열린 파일 등)이므로 merge로 넘어가지 않는다.
   - 갈라진 경우에만 `merge --ff --no-edit --no-autostash`. 대상은 fetch 직후 확인해 둔 **커밋**이고(메시지는 원격 브랜치 이름), 실패 후 남은 `MERGE_HEAD`가 그 커밋일 때만 `merge --abort` — 다른 도구가 그 사이 fetch해도 어긋나지 않는다.
   - **도중에 멈춘 ff·merge 되돌리기**: Unity·엑셀이 잡은 파일, LFS 실패 등으로 쓰다 멈추면 HEAD는 그대로인데 일부 파일만 upstream 것이 된다. 이번 풀이 바꾸는 경로(HEAD..upstream)를 기준으로, 시작 전 `status`에 없었고(사용자 변경 아님) 지금은 정확히 upstream blob인 파일만 HEAD로 되돌리거나, upstream에만 있는 새 파일이면 지우고 새로 생긴 빈 폴더도 지운다. 시작 전부터 디스크에 있던 파일(무시된 파일 포함)은 지우지 않는다. assume-unchanged처럼 `status`에 안 보이는 파일도 경로 기준이라 잡힌다. 못 되돌린 경로는 메시지에 남긴다.
   - **성공했지만 못 지운 파일**: upstream이 지운 파일을 다른 프로그램이 열고 있으면 git은 경고만 하고 성공한다. 그런 파일이 남으면 팝업에 오르도록 실패로 알린다.
   - `--no-autostash`: `merge.autostash` 설정이 로컬 변경을 치웠다 다시 얹으며 충돌을 내고도 성공으로 끝나는 일을 막는다.
   - `GIT_TERMINAL_PROMPT=0`, `GIT_MERGE_AUTOEDIT=no` — 지켜보는 사람이 없으니 프롬프트·편집기를 기다리며 멈추지 않는다.
4. **SVN** (`pull.rs::update_svn`): 배치와 같은 `svn update --non-interactive --accept postpone`.
   - 작업본 안에서 대상 없이 돌린다. 경로를 인자로 주면 `art@2024`의 `@`를 peg revision으로 읽고, ANSI에 없는 글자(`ü`)는 비슷한 글자로 바꿔 옆 작업본을 업데이트한다.
   - SVN에는 abort가 없어 충돌은 작업본에 남는다 — 그래서 `Summary of conflicts`가 나오면 (외부 항목 오류로 실패해도) **실패로 보고**한다.
   - 전체 시간 제한은 두지 않는다(대형 아트 작업본은 몇 분도 정상이고, 도중에 끊으면 작업본이 잠긴다). 대신 **10분 동안 출력도 svn 프로세스의 I/O(`GetProcessIoCounters`)도 없으면** 멈춘 것으로 보고 끝낸다 — `svn://`은 자체 네트워크 타임아웃이 없다. 폴더 크기는 신호로 못 쓴다: NTFS는 열린 파일의 크기를 닫을 때에야 목록에 반영해, 큰 파일 하나를 받는 동안 멈춘 것처럼 보인다. 끝낸 뒤 터널 프로그램(svn+ssh)이 파이프를 잡고 있어도 5초 넘게 기다리지 않는다.
   - svn 출력의 경로는 ANSI 코드 페이지(CP949)라 UTF-8이 아니면 ANSI로 읽는다.
5. **결과 팝업**: 미리 만들어 둔 숨김 창 `pull-report`. Git·SVN 풀이 따로 끝나도, 팝업이 열려 있는 동안 온 실패는 이어 붙인다. 풀마다 **시도한 저장소 목록**을 함께 보내, 이번에 성공한 저장소는 열린 팝업에서도 빠지고 남은 실패가 없으면 팝업이 닫힌다. 목록은 이벤트에 실어 보내 팝업이 바쁜 백엔드를 기다리지 않는다.
6. **동시성**: 풀·스캔 명령은 모두 스레드 풀(`#[tauri::command(async)]`)에서 돈다. 조정은 **영역별**이다 — 한 영역이 풀하는 동안 그 영역의 새로고침(폴링·설정 저장)만 미뤘다가 풀이 끝나면 돌리고, 다른 영역은 그대로 새로고침된다. 풀을 누를 때 그 영역에서 돌던 스캔은 멈추고(세대 번호), **풀할 저장소에 걸린 스캔만** 끝나기를 기다린 뒤 시작한다 — 다른 저장소의 fetch가 로그인 창에 막혀 있어도 풀은 진행된다. 설정 변경(영역 끄기, 저장소 추가·삭제)은 풀 중에도 바로 반영한다.
7. **새로고침 대상**: 시도한 저장소 전부. Git은 방금 fetch했으니 로컬만, SVN은 남은 behind를 알려면 서버 조회가 필요해 `do_fetch`로 다시 읽는다.
8. **SVN의 서버 상태**: git과 달리 로컬에 남는 서버 정보가 없어, 서버 없이 읽으면 늘 behind 0이다. 작업본마다 처음 스캔할 때 한 번 서버에 묻고, 그 뒤 로컬 스캔은 마지막 답을 이어 쓴다 — 그래야 시작 직후와 설정 저장 뒤에도 업데이트 버튼이 맞게 뜬다.

### What we will not do

- SVN 충돌 자동 해결(`mine-conflict` 등) — 데이터 손실 위험
- rebase 방식 풀, 커밋·푸시 등 쓰기 동작 확대
- 배치의 실패 로그 파일(`SVN-Update-Logs`) — 팝업이 메시지를 보여 준다

## Follow-up

- 검증: `cargo test --lib` 22건(실제 git·svn 임시 저장소로 ff·merge·충돌 abort·로컬 변경 보호·진행 중 merge 보존·잠긴 파일로 멈춘 ff 되돌리기·무시된 파일/assume-unchanged/새 폴더·못 지운 파일·autostash·prune된 upstream·SVN 충돌·`@`/`ü` 작업본·멈춤 감시), 실제 앱 E2E 35개 확인(WebView2 DevTools 프로토콜로 버튼 클릭·팝업 누적/정리·영역 토글·창 높이·설정 창·스캔 중 풀 경합).
- 다관점 리뷰(워크플로) 1차 22건 중 17건, 2차(수정분) 13건 중 11건 확인 → 전부 반영.
- 참고 배치 `D:\SVN-Update-All.bat`은 기능 구현 후 사용자 요청대로 지움(휴지통).
- Logged in INDEX: yes
