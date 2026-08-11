import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { formatSummaryIcons, renderStatusIcons, rowClass } from "./status-icons";
import type { RepoConfig, RepoEntry, RepoStatus } from "./types";

const PREVIEW = new URLSearchParams(window.location.search).has("preview");

const PREVIEW_SCAN: RepoStatus[] = [
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
];

const BASE_HEIGHT = 72;
const ROW_HEIGHT = 36;
const MAX_VISIBLE_ROWS = 5;

let scanGeneration = 0;
let scrollFadeBound = false;
let lastRepos: RepoStatus[] = [];

function $(id: string): HTMLElement {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing #${id}`);
  return el;
}

function formatTime(unixSeconds: number): string {
  const date = new Date(unixSeconds * 1000);
  return date.toLocaleTimeString("ko-KR", { hour: "2-digit", minute: "2-digit" });
}

function placeholderStatus(entry: RepoEntry): RepoStatus {
  return {
    name: entry.name,
    path: entry.path,
    branch: null,
    dirty: false,
    changed_count: 0,
    ahead: 0,
    behind: 0,
    sync_state: "synced",
    badge: "…",
    error: null,
    loading: true,
  };
}

function renderMainRepos(repos: RepoStatus[]) {
  const list = $("repo-list");
  list.replaceChildren();
  for (const repo of repos) {
    const li = document.createElement("li");
    li.className = `repo-item ${rowClass(repo)}`;

    const nameWrap = document.createElement("span");
    nameWrap.className = "repo-name-wrap";

    const name = document.createElement("span");
    name.className = "repo-name";
    name.textContent = repo.name;
    nameWrap.append(name);

    if (repo.refreshing) {
      const indicator = document.createElement("span");
      indicator.className = "repo-refresh-indicator";
      indicator.setAttribute("role", "status");
      indicator.setAttribute("aria-label", "최신화 중");
      nameWrap.append(indicator);
    }

    const branch = document.createElement("span");
    branch.className = "repo-branch";
    branch.textContent = repo.loading
      ? "…"
      : (repo.error ?? repo.branch ?? "—");

    li.append(nameWrap, branch, renderStatusIcons(repo));
    list.append(li);
  }
  void resizeWindow(repos.length || 1);
  updateScrollFade();
}

function updateScrollFade() {
  requestAnimationFrame(() => {
    const scroll = $("repo-scroll");
    const fadeTop = $("repo-fade-top");
    const fadeBottom = $("repo-fade-bottom");
    const canScroll = scroll.scrollHeight > scroll.clientHeight + 1;
    const atTop = scroll.scrollTop <= 1;
    const atBottom = scroll.scrollTop + scroll.clientHeight >= scroll.scrollHeight - 1;

    fadeTop.classList.toggle("visible", canScroll && !atTop);
    fadeBottom.classList.toggle("visible", canScroll && !atBottom);
  });
}

function setupRepoScroll() {
  if (scrollFadeBound) return;
  scrollFadeBound = true;

  const scroll = $("repo-scroll");
  const wrap = $("repo-scroll-wrap");

  scroll.addEventListener("scroll", updateScrollFade, { passive: true });
  window.addEventListener("resize", updateScrollFade);

  wrap.addEventListener(
    "wheel",
    (e) => {
      if (scroll.scrollHeight <= scroll.clientHeight + 1) return;
      e.preventDefault();
      scroll.scrollTop += e.deltaY;
      updateScrollFade();
    },
    { passive: false },
  );
}

function renderSummary(repos: RepoStatus[]) {
  const summaryEl = $("summary");
  summaryEl.replaceChildren(formatSummaryIcons(repos));
}

async function resizeWindow(repoCount: number) {
  const visibleRows = Math.min(Math.max(1, repoCount), MAX_VISIBLE_ROWS);
  const height = BASE_HEIGHT + visibleRows * ROW_HEIGHT;
  try {
    await invoke("resize_main_window", { height });
  } catch {
    /* dev without tauri */
  }
}

function renderRepos(repos: RepoStatus[]) {
  lastRepos = repos;
  renderSummary(repos);
  renderMainRepos(repos);
}

function findPreviousRepo(entry: RepoEntry): RepoStatus | undefined {
  return lastRepos.find((r) => r.name === entry.name && r.path === entry.path);
}

function reposForRefresh(config: RepoConfig): RepoStatus[] {
  return config.repos.map((entry) => {
    const prev = findPreviousRepo(entry);
    if (prev && !prev.loading) {
      return { ...prev, refreshing: true };
    }
    return placeholderStatus(entry);
  });
}

async function refresh(options?: { doFetch?: boolean }) {
  const statusEl = $("status");
  const doFetch = options?.doFetch ?? false;
  const generation = ++scanGeneration;

  if (PREVIEW) {
    renderRepos(PREVIEW_SCAN);
    statusEl.textContent = `updated ${formatTime(Math.floor(Date.now() / 1000))}`;
    return;
  }

  let config: RepoConfig;
  try {
    config = (await invoke("get_config")) as RepoConfig;
  } catch (e) {
    statusEl.textContent = `error: ${String(e)}`;
    return;
  }

  const repos = reposForRefresh(config);
  renderRepos(repos);
  statusEl.textContent = repos.some((r) => r.refreshing) ? "refreshing…" : "scanning…";

  if (repos.length === 0) {
    statusEl.textContent = "no repos";
    return;
  }

  for (let i = 0; i < config.repos.length; i++) {
    if (generation !== scanGeneration) return;

    const entry = config.repos[i];
    try {
      const updated = (await invoke("scan_one_repo", {
        name: entry.name,
        path: entry.path,
        doFetch,
      })) as RepoStatus;
      if (generation !== scanGeneration) return;
      repos[i] = { ...updated, refreshing: false, loading: false };
      renderRepos(repos);
    } catch (e) {
      if (generation !== scanGeneration) return;
      repos[i] = {
        ...repos[i],
        loading: false,
        refreshing: false,
        error: String(e),
        badge: "ERR",
        sync_state: "error",
      };
      renderRepos(repos);
    }
  }

  if (generation !== scanGeneration) return;
  statusEl.textContent = `updated ${formatTime(Math.floor(Date.now() / 1000))}`;
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
  setupRepoScroll();

  document.addEventListener("contextmenu", (e) => {
    e.preventDefault();
    showContextMenu(e.clientX, e.clientY);
  });

  $("context-backdrop").addEventListener("click", hideContextMenu);
  $("menu-settings").addEventListener("click", () => void openSettings());
  $("menu-refresh").addEventListener("click", () => {
    hideContextMenu();
    void refresh({ doFetch: true });
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
      void refresh({ doFetch: false });
    });
  } catch {
    /* browser preview */
  }

  void refresh({ doFetch: false });

  let interval = 300_000;
  try {
    interval = await invoke<number>("get_poll_interval_ms");
  } catch {
    /* default */
  }
  window.setInterval(() => void refresh({ doFetch: true }), interval);
}

void boot();
