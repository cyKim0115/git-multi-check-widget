# Git Multi-Check Widget

Windows 바탕화면용 플로팅 위젯. 설정한 여러 Git 레포의 **브랜치·dirty 상태**를 한눈에 표시한다.

`cursor-usage-widget`과 같은 Tauri 2 + Vite + TypeScript 스택.

## 목표

- 로컬 `C:\Users\cykim\repo` 등에 있는 레포를 목록으로 스캔
- 각 레포: 이름, 현재 브랜치, 변경 파일 수, clean/dirty
- 주기적 폴링 + 우클릭 새로고침
- 설정 파일로 스캔 대상 경로 관리 (`config/repos.default.json` → 사용자 `%LOCALAPPDATA%\GitMultiCheckWidget\repos.json`)

## 일반 사용자

1. **`시작.bat`** 더블클릭
2. 첫 실행 시 릴리스 빌드 후 `%LOCALAPPDATA%\GitMultiCheckWidget\git-multi-check-widget.exe` 실행

## 개발 실행

```powershell
npm install
npm run tauri dev
```

요구: Windows, Node.js, Rust, Visual Studio C++ Build Tools, WebView2, Git CLI.

## 스크립트

| 명령 | 용도 |
|------|------|
| `시작.bat` / `npm run start:app` | 설치본 실행 |
| `npm run tauri dev` | Vite + debug |
| `npm run build:app` | 릴리스 빌드 |
