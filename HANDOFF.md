# HANDOFF — Git Multi-Check Widget

## 현재 상태 (2026-08-11)

- 레포 생성: `cyKim0115/git-multi-check-widget`
- 로컬 경로: `C:\Users\cykim\repo\git-multi-check-widget`
- Tauri 2 scaffold + 기본 `scan_repos` Rust 커맨드 + 목록 UI 스켈레톤

## 다음 작업 후보

1. **설정 UI** — repos.json 편집, 경로 추가/제거
2. **폴링 간격** — cursor-usage-widget처럼 5분 기본, 컨텍스트 메뉴 새로고침
3. **시각 polish** — dirty/clean 색상, 브랜치 아이콘, 창 높이 자동 조절
4. **시작프로그램** — `auto-launch` + LOCALAPPDATA 설치본 경로 (usage 위젯 패턴)
5. **멀티 루트 스캔** — `C:\Users\cykim\repo` 아래 자동 발견 vs 명시 allowlist
6. **알림** — dirty 레포 N개일 때 배지 (선택)

## 참고

- 형제 위젯: `C:\Users\cykim\repo\cursor-usage-widget`
- Cursor 에이전트 allowlist 스캔 규칙: `rag` + `TeenipingTycoon` (user rules)

## 개발 메모

- dev Vite 포트 **1421** (usage 위젯 1420과 충돌 방지)
- Git은 `git -C <path> status --porcelain` / `branch --show-current` 호출
