export type OpenTarget =
  | "fork"
  | "cmd"
  | "git_bash"
  | "cursor"
  | "vscode"
  | "explorer";

export type OpenTargetOption = {
  id: OpenTarget;
  label: string;
  icon: string;
  description: string;
};

export const OPEN_TARGET_OPTIONS: OpenTargetOption[] = [
  {
    id: "fork",
    label: "Fork",
    icon: "./icons/fork.png",
    description: "Fork에서 저장소 열기",
  },
  {
    id: "cmd",
    label: "명령 프롬프트",
    icon: "./icons/terminal.png",
    description: "cmd에서 해당 경로로 cd",
  },
  {
    id: "git_bash",
    label: "Git Bash",
    icon: "./icons/git.png",
    description: "Git Bash에서 해당 경로로 시작",
  },
  {
    id: "cursor",
    label: "Cursor",
    icon: "./icons/cursor.png",
    description: "Cursor에서 폴더 열기",
  },
  {
    id: "vscode",
    label: "VS Code",
    icon: "./icons/vscode.png",
    description: "VS Code에서 폴더 열기",
  },
  {
    id: "explorer",
    label: "탐색기",
    icon: "./icons/explorer.png",
    description: "Windows 탐색기에서 폴더 열기",
  },
];

export const DEFAULT_OPEN_TARGET: OpenTarget = "fork";
