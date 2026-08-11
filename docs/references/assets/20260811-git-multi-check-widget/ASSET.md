# Reference Asset

- Asset id: `20260811-git-multi-check-widget`
- Date: 2026-08-11
- Similarity: inspired
- Status: draft

## Summary

여러 Git 레포를 등록해 두고, **로컬 변경·push 필요·pull 필요** 상태를 한 위젯에서 상시 확인한다.

## Sources

| Source | Role |
|--------|------|
| 사용자 요청 (2026-08-11) | 핵심 가치: 멀티 레포 상태 한곳에서 확인 |
| `cursor-usage-widget` | UI 셸·Tauri 패턴·시작.bat·우클릭 메뉴 |
| `rag/scripts/canon-auto-sync.ps1` | `git fetch` + ahead/behind 판정 참고 |
| (폐기) 초기 Tauri scaffold | `scan_repos` 초안 — 재사용하지 않음 |

## Reuse hints

- 창: 투명·always-on-top·frameless·`data-tauri-drag-region`
- 행 단위 리스트: usage 위젯의 `track` 대신 **repo row** (이름 + 브랜치 + 상태 배지)
- 폴링 5분 + 우클릭 새로고침
- 설정: JSON 파일 (`repos.json`)

## Child docs

- `reference-brief.md`
- `system-spec.md`
