import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { formatSummaryIcons, renderStatusIcons, rowClass } from "./status-icons";
import type { RepoStatus, ScanResult } from "./types";
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
const ROW_HEIGHT = 36;

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

    li.append(name, branch, renderStatusIcons(repo));
    list.append(li);
  }
  void resizeWindow(repos.length || 1);
}

function renderSummary(repos: RepoStatus[]) {
  const summaryEl = $("summary");
  summaryEl.replaceChildren(formatSummaryIcons(repos));
}
async function resizeWindow(repoCount: number) {
  const height = BASE_HEIGHT + Math.max(1, repoCount) * ROW_HEIGHT;
  try {
    await invoke("resize_main_window", { height });
  } catch {
    /* dev without tauri */
  }
}

async function refresh() {
  const statusEl = $("status");
  if (PREVIEW) {
    renderSummary(PREVIEW_SCAN.repos);
    renderMainRepos(PREVIEW_SCAN.repos);
    statusEl.textContent = `updated ${formatTime(PREVIEW_SCAN.scanned_at)}`;
    return;
  }
  statusEl.textContent = "scanning…";
  try {
    const result = (await invoke("scan_all_repos")) as ScanResult;
    renderSummary(result.repos);
    renderMainRepos(result.repos);
    statusEl.textContent = `updated ${formatTime(result.scanned_at)}`;
  } catch (e) {
    statusEl.textContent = `error: ${String(e)}`;
  }
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

async function openSettings() {
  hideContextMenu();
  try {
    await invoke("open_settings_window");
  } catch (e) {
    window.alert(String(e));
  }
}

async function boot() {
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

  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape") hideContextMenu();
  });

  try {
    await listen("config-saved", () => {
      void refresh();
    });
  } catch {
    /* browser preview */
  }

  await refresh();

  let interval = 300_000;
  try {
    interval = await invoke<number>("get_poll_interval_ms");
  } catch {
    /* default */
  }
  window.setInterval(() => void refresh(), interval);
}

void boot();
