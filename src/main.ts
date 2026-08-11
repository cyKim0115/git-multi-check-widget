import { invoke } from "@tauri-apps/api/core";

type SyncState =
  | "synced"
  | "ahead"
  | "behind"
  | "diverged"
  | "no_upstream"
  | "no_remote"
  | "error";

type RepoEntry = {
  name: string;
  path: string;
  url?: string | null;
};

type RepoConfig = {
  repos: RepoEntry[];
};

type RepoStatus = {
  name: string;
  path: string;
  branch: string | null;
  dirty: boolean;
  changed_count: number;
  ahead: number;
  behind: number;
  sync_state: SyncState;
  badge: string;
  error: string | null;
};

type ScanResult = {
  repos: RepoStatus[];
  scanned_at: string;
  summary: string;
};

type ValidateResult = {
  ok: boolean;
  message: string;
  resolved_path: string | null;
  suggested_name: string | null;
  remote_url: string | null;
};

const PREVIEW = new URLSearchParams(window.location.search).has("preview");

const PREVIEW_SCAN: ScanResult = {
  scanned_at: String(Math.floor(Date.now() / 1000)),
  summary: "1 dirty · 1 push · 1 pull · 0 err",
  repos: [
    {
      name: "rag",
      path: "C:\\Users\\cykim\\repo\\rag",
      branch: "main",
      dirty: true,
      changed_count: 3,
      ahead: 0,
      behind: 0,
      sync_state: "synced",
      badge: "dirty ·3",
      error: null,
    },
    {
      name: "TeenipingTycoon",
      path: "C:\\Users\\cykim\\repo\\TeenipingTycoon",
      branch: "develop",
      dirty: false,
      changed_count: 0,
      ahead: 0,
      behind: 2,
      sync_state: "behind",
      badge: "pull ·2",
      error: null,
    },
    {
      name: "system-crew",
      path: "C:\\Users\\cykim\\repo\\system-crew",
      branch: "main",
      dirty: false,
      changed_count: 0,
      ahead: 1,
      behind: 0,
      sync_state: "ahead",
      badge: "push ·1",
      error: null,
    },
  ],
};
const BASE_HEIGHT = 72;

let configDraft: RepoConfig = { repos: [] };
let lastValidate: ValidateResult | null = null;

function $(id: string): HTMLElement {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing #${id}`);
  return el;
}

function formatTime(value: string): string {
  const n = Number(value);
  const date = Number.isFinite(n) ? new Date(n * 1000) : new Date(value);
  return date.toLocaleTimeString("ko-KR", { hour: "2-digit", minute: "2-digit" });
}

function rowClass(repo: RepoStatus): string {
  if (repo.error) return "error";
  if (repo.dirty) return "dirty";
  if (repo.sync_state === "ahead" || repo.sync_state === "diverged") return "push";
  if (repo.sync_state === "behind") return "pull";
  return "clean";
}

async function resizeWindow(repoCount: number) {
  const height = BASE_HEIGHT + Math.max(1, repoCount) * ROW_HEIGHT;
  try {
    await invoke("resize_main_window", { height });
  } catch {
    /* dev without tauri */
  }
}

function renderMainRepos(repos: RepoStatus[]) {
  const list = $("repo-list");
  list.replaceChildren();
  for (const repo of repos) {
    const li = document.createElement("li");
    li.className = `repo-item ${rowClass(repo)}`;

    const name = document.createElement("span");
    name.className = "repo-name";
    name.textContent = repo.name;

    const branch = document.createElement("span");
    branch.className = "repo-branch";
    branch.textContent = repo.error ?? repo.branch ?? "—";

    const badge = document.createElement("span");
    badge.className = `repo-badge ${rowClass(repo)}`;
    badge.textContent = repo.error ? "ERR" : repo.badge;

    li.append(name, branch, badge);
    list.append(li);
  }
  void resizeWindow(repos.length || 1);
}

async function refresh() {
  const statusEl = $("status");
  if (PREVIEW) {
    $("summary").textContent = PREVIEW_SCAN.summary;
    renderMainRepos(PREVIEW_SCAN.repos);
    statusEl.textContent = `updated ${formatTime(PREVIEW_SCAN.scanned_at)}`;
    return;
  }
  statusEl.textContent = "scanning…";
  try {
    const result = (await invoke("scan_all_repos")) as ScanResult;
    $("summary").textContent = result.summary;
    renderMainRepos(result.repos);
    statusEl.textContent = `updated ${formatTime(result.scanned_at)}`;
  } catch (e) {
    statusEl.textContent = `error: ${String(e)}`;
  }
}

function renderSettingsList() {
  const list = $("settings-repo-list");
  const empty = $("settings-empty");
  list.replaceChildren();

  if (configDraft.repos.length === 0) {
    empty.classList.remove("hidden");
    return;
  }
  empty.classList.add("hidden");

  configDraft.repos.forEach((repo, index) => {
    const li = document.createElement("li");
    li.className = "settings-repo-item";

    const meta = document.createElement("div");
    meta.className = "settings-repo-meta";
    const title = document.createElement("strong");
    title.textContent = repo.name;
    const path = document.createElement("span");
    path.textContent = repo.path;
    meta.append(title, path);

    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "btn danger small";
    remove.textContent = "삭제";
    remove.addEventListener("click", () => {
      configDraft.repos.splice(index, 1);
      renderSettingsList();
    });

    li.append(meta, remove);
    list.append(li);
  });
}

function resetAddForm() {
  ($("input-name") as HTMLInputElement).value = "";
  ($("input-url") as HTMLInputElement).value = "";
  ($("btn-add") as HTMLButtonElement).disabled = true;
  lastValidate = null;
  $("test-result").textContent = "테스트로 유효성을 확인하세요.";
  $("test-result").className = "test-result";
}

function openSettings() {
  hideContextMenu();
  $("settings-backdrop").classList.remove("hidden");
  resetAddForm();
  renderSettingsList();
}

function closeSettings() {
  $("settings-backdrop").classList.add("hidden");
}

async function loadConfigDraft() {
  configDraft = (await invoke("get_config")) as RepoConfig;
}

async function saveConfigDraft() {
  await invoke("set_config", { config: configDraft });
}

function hideContextMenu() {
  $("context-menu").classList.add("hidden");
  $("context-backdrop").classList.add("hidden");
}

function showContextMenu(x: number, y: number) {
  const backdrop = $("context-backdrop");
  const menu = $("context-menu");
  backdrop.classList.remove("hidden");
  menu.classList.remove("hidden");

  const rect = menu.getBoundingClientRect();
  const maxX = Math.max(8, window.innerWidth - rect.width - 8);
  const maxY = Math.max(8, window.innerHeight - rect.height - 8);
  menu.style.left = `${Math.min(x, maxX)}px`;
  menu.style.top = `${Math.min(y, maxY)}px`;
}

async function runTest() {
  const input = ($("input-url") as HTMLInputElement).value.trim();
  const resultEl = $("test-result");
  const addBtn = $("btn-add") as HTMLButtonElement;
  resultEl.textContent = "테스트 중…";
  resultEl.className = "test-result pending";
  addBtn.disabled = true;
  lastValidate = null;

  try {
    const result = (await invoke("validate_repo", { input })) as ValidateResult;
    lastValidate = result;
    resultEl.textContent = result.message;
    resultEl.className = `test-result ${result.ok ? "ok" : "fail"}`;

    if (result.ok) {
      addBtn.disabled = false;
      const nameInput = $("input-name") as HTMLInputElement;
      if (!nameInput.value.trim() && result.suggested_name) {
        nameInput.value = result.suggested_name;
      }
    }
  } catch (e) {
    resultEl.textContent = String(e);
    resultEl.className = "test-result fail";
  }
}

function addRepoFromForm() {
  if (!lastValidate?.ok || !lastValidate.resolved_path) return;

  const nameInput = ($("input-name") as HTMLInputElement).value.trim();
  const name = nameInput || lastValidate.suggested_name || "repo";
  const path = lastValidate.resolved_path;

  if (configDraft.repos.some((r) => r.path.toLowerCase() === path.toLowerCase())) {
    $("test-result").textContent = "이미 등록된 경로입니다.";
    $("test-result").className = "test-result fail";
    return;
  }

  configDraft.repos.push({
    name,
    path,
    url: lastValidate.remote_url,
  });
  renderSettingsList();
  resetAddForm();
}

async function boot() {
  if (PREVIEW) {
    configDraft = {
      repos: [
        { name: "rag", path: "C:\\Users\\cykim\\repo\\rag", url: "https://github.com/cyKim0115/rag" },
        { name: "TeenipingTycoon", path: "C:\\Users\\cykim\\repo\\TeenipingTycoon", url: null },
      ],
    };
  } else {
    await loadConfigDraft();
  }

  document.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    showContextMenu(e.clientX, e.clientY);
  });

  $("context-backdrop").addEventListener("click", hideContextMenu);
  $("menu-settings").addEventListener("click", () => void openSettings());
  $("menu-refresh").addEventListener("click", () => {
    hideContextMenu();
    void refresh();
  });
  $("menu-quit").addEventListener("click", async () => {
    hideContextMenu();
    await invoke("quit_app");
  });

  $("settings-close").addEventListener("click", () => closeSettings());
  $("settings-backdrop").addEventListener("click", (e) => {
    if (e.target === $("settings-backdrop")) closeSettings();
  });
  $("settings-modal").addEventListener("click", (e) => e.stopPropagation());

  $("btn-test").addEventListener("click", () => void runTest());
  $("btn-add").addEventListener("click", () => addRepoFromForm());
  $("btn-save-close").addEventListener("click", async () => {
    await saveConfigDraft();
    closeSettings();
    await refresh();
  });

  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape") {
      hideContextMenu();
      closeSettings();
    }
  });

  await refresh();
  if (PREVIEW) {
    openSettings();
    return;
  }
  let interval = 300_000;
  try {
    interval = await invoke<number>("get_poll_interval_ms");
  } catch {
    /* default */
  }
  window.setInterval(() => void refresh(), interval);
}

void boot();
