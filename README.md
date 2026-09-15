# Git Multi-Check Widget

Windows 바탕화면용 위젯 — 여러 Git 레포의 **브랜치·dirty 상태**를 한눈에 확인.

## 실행

- **일반:** 프로젝트 폴더의 `시작.bat` 더블클릭 → `%LOCALAPPDATA%\GitMultiCheckWidget\` 설치본 실행
- **개발:** `npm install` 후 `npm run tauri dev`

## 기능

- 등록한 레포별 `dirty` / `push` / `pull` 상태 아이콘 표시
- 5분 폴링 (매회 `git fetch`)
- 우클릭 → **설정** (별도 창), 새로고침, 종료
- 설정 → **창**에서 「항상 다른 창 위에 표시」 끄기 — 끄면 일반 창처럼 다른 창이 위로 올라올 수 있습니다

## 형제 프로젝트

- `cursor-usage-widget` — 같은 Tauri 2 + Vite 스택의 데스크톱 위젯

## Open Source Assets / Credits

| Asset | Author | Source | License |
|-------|--------|--------|---------|
| **Git Icon** (앱 아이콘) | Papirus Dev Team | [IconArchive — Papirus Apps Icons](https://www.iconarchive.com/show/papirus-apps-icons-by-papirus-team.html) | [GNU GPLv3](https://www.gnu.org/licenses/gpl-3.0.html) |

- 원본 PNG: `docs/assets/git-icon-papirus-source.png`
- 빌드용 아이콘: `app-icon.png`, `src-tauri/icons/` (스크립트 `npm run make:icon`으로 생성)

**라이선스 구분:** 위 아이콘 에셋만 GPLv3가 적용됩니다. 본 저장소의 애플리케이션 소스코드 라이선스와는 별개이며, 자세한 고지는 [`LICENSE-THIRD-PARTY.md`](LICENSE-THIRD-PARTY.md)를 참고하세요.

## 아이콘 재생성

```powershell
npm run make:icon
npm run build:app
```

`app-icon.png`를 교체한 뒤 위 명령을 실행하면 `src-tauri/icons/`가 갱신됩니다.
