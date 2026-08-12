import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { formatSummaryIcons, renderStatusIcons, rowClass } from "./status-icons";
import { DEFAULT_OPEN_TARGET } from "./open-targets";
import type { OpenTarget } from "./open-targets";
import type { RepoConfig, RepoEntry, RepoStatus } from "./types";

const PREVIEW = new URLSearchParams(window.location.search).has("preview");

let openTarget: OpenTarget = DEFAULT_OPEN_TARGET;

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
const SCROLL_ACCEL_BASE = 0.07;
const SCROLL_ACCEL_STEP = 0.035;
const SCROLL_ACCEL_BURST_MAX = 7;
const WHEEL_BURST_MS = 140;
const SCROLL_FRICTION = 0.92;
const SCROLL_MIN_VELOCITY = 0.18;
const SCROLL_MAX_VELOCITY = 28;

let scanGeneration = 0;
let scrollFadeBound = false;
let scrollVelocity = 0;
let scrollBurst = 0;
let lastWheelAt = 0;
let scrollAnimId: number | null = null;
let lastRepos: RepoStatus[] = [];
let windowAutoSized = false;
let userResizedWindow = false;
let programmaticResize = false;
let lastAutoSizedRepoCount = 0;
let contextRepoTarget: RepoStatus | null = null;

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

    if (!repo.loading && !repo.error) {
      li.classList.add("repo-item--openable");
      li.title = "우클릭하여 메뉴";
      li.addEventListener("contextmenu", (e) => {
        e.preventDefault();
        e.stopPropagation();
        contextRepoTarget = repo;
        showRepoContextMenu(e.clientX, e.clientY);
      });
    }

    list.append(li);
  }
  void maybeAutoResizeWindow(repos.length || 1);
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

function maxScrollTop(scroll: HTMLElement): number {
  return Math.max(0, scroll.scrollHeight - scroll.clientHeight);
}

function clampScrollVelocity(velocity: number): number {
  return Math.max(-SCROLL_MAX_VELOCITY, Math.min(SCROLL_MAX_VELOCITY, velocity));
}

function ensureScrollAnimation(scroll: HTMLElement) {
  if (scrollAnimId !== null) return;

  const tick = () => {
    if (Math.abs(scrollVelocity) < SCROLL_MIN_VELOCITY) {
      scrollAnimId = null;
      scrollVelocity = 0;
      updateScrollFade();
      return;
    }

    const max = maxScrollTop(scroll);
    const prev = scroll.scrollTop;
    const next = Math.max(0, Math.min(max, prev + scrollVelocity));
    scroll.scrollTop = next;

    if (next !== prev) {
      scrollVelocity *= SCROLL_FRICTION;
    } else {
      scrollVelocity *= 0.4;
    }

    if (scroll.scrollTop <= 0 || scroll.scrollTop >= max) {
      scrollVelocity *= 0.55;
    }

    updateScrollFade();
    scrollAnimId = requestAnimationFrame(tick);
  };

  scrollAnimId = requestAnimationFrame(tick);
}

function currentScrollAccel(): number {
  return SCROLL_ACCEL_BASE + scrollBurst * SCROLL_ACCEL_STEP;
}

function applyWheelScroll(scroll: HTMLElement, deltaY: number) {
  const now = performance.now();
  if (now - lastWheelAt < WHEEL_BURST_MS) {
    scrollBurst = Math.min(SCROLL_ACCEL_BURST_MAX, scrollBurst + 1);
  } else {
    scrollBurst = 0;
  }
  lastWheelAt = now;

  scrollVelocity = clampScrollVelocity(scrollVelocity + deltaY * currentScrollAccel());
  ensureScrollAnimation(scroll);
}

function setupRepoScroll() {
  if (scrollFadeBound) return;
  scrollFadeBound = true;

  const scroll = $("repo-scroll");
  const wrap = $("repo-scroll-wrap");

  scroll.addEventListener("scroll", updateScrollFade, { passive: true });
  window.addEventListener("resize", () => {
    if (!programmaticResize && windowAutoSized) {
      userResizedWindow = true;
    }
    updateScrollFade();
  });

  wrap.addEventListener(
    "wheel",
    (e) => {
      if (scroll.scrollHeight <= scroll.clientHeight + 1) return;
      e.preventDefault();
      applyWheelScroll(scroll, e.deltaY);
    },
    { passive: false },
  );
}

function renderSummary(repos: RepoStatus[]) {
  const summaryEl = $("summary");
  summaryEl.replaceChildren(formatSummaryIcons(repos));
}

async function maybeAutoResizeWindow(repoCount: number) {
  if (userResizedWindow) return;
  if (windowAutoSized && repoCount === lastAutoSizedRepoCount) return;

  const visibleRows = Math.min(Math.max(1, repoCount), MAX_VISIBLE_ROWS);
  const height = BASE_HEIGHT + visibleRows * ROW_HEIGHT;
  try {
    programmaticResize = true;
    await invoke("resize_main_window", { height });
    windowAutoSized = true;
    lastAutoSizedRepoCount = repoCount;
  } catch {
    /* dev without tauri */
  } finally {
    programmaticResize = false;
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

async function openRepoAtPath(path: string) {
  const statusEl = $("status");
  try {
    await invoke("open_repo", { path, openTarget });
    statusEl.textContent = "opened";
  } catch (e) {
    statusEl.textContent = String(e);
  }
}

async function openRepoInExplorer(path: string) {
  const statusEl = $("status");
  try {
    await invoke("open_repo", { path, openTarget: "explorer" });
    statusEl.textContent = "opened in explorer";
  } catch (e) {
    statusEl.textContent = String(e);
  }
}

async function refreshOneRepo(name: string, path: string) {
  const statusEl = $("status");
  const repos = [...lastRepos];
  const idx = repos.findIndex((r) => r.name === name && r.path === path);
  if (idx === -1) return;

  repos[idx] = { ...repos[idx], refreshing: true };
  renderRepos(repos);
  statusEl.textContent = "refreshing…";

  try {
    const updated = (await invoke("scan_one_repo", {
      name,
      path,
      doFetch: true,
    })) as RepoStatus;
    repos[idx] = { ...updated, refreshing: false, loading: false };
    renderRepos(repos);
    statusEl.textContent = `updated ${formatTime(Math.floor(Date.now() / 1000))}`;
  } catch (e) {
    repos[idx] = {
      ...repos[idx],
      loading: false,
      refreshing: false,
      error: String(e),
      badge: "ERR",
      sync_state: "error",
    };
    renderRepos(repos);
    statusEl.textContent = String(e);
  }
}

async function loadOpenTarget() {
  try {
    const config = (await invoke("get_config")) as RepoConfig;
    openTarget = config.open_target ?? DEFAULT_OPEN_TARGET;
  } catch {
    /* dev without tauri */
  }
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
  $("repo-context-menu").classList.add("hidden");
  $("context-backdrop").classList.add("hidden");
  contextRepoTarget = null;
}

function positionContextMenu(menu: HTMLElement, x: number, y: number) {
  const rect = menu.getBoundingClientRect();
  const maxX = Math.max(8, window.innerWidth - rect.width - 8);
  const maxY = Math.max(8, window.innerHeight - rect.height - 8);
  menu.style.left = `${Math.min(x, maxX)}px`;
  menu.style.top = `${Math.min(y, maxY)}px`;
}

function showWidgetContextMenu(x: number, y: number) {
  const backdrop = $("context-backdrop");
  const menu = $("context-menu");
  $("repo-context-menu").classList.add("hidden");
  backdrop.classList.remove("hidden");
  menu.classList.remove("hidden");
  positionContextMenu(menu, x, y);
}

function showRepoContextMenu(x: number, y: number) {
  const backdrop = $("context-backdrop");
  const menu = $("repo-context-menu");
  $("context-menu").classList.add("hidden");
  backdrop.classList.remove("hidden");
  menu.classList.remove("hidden");
  positionContextMenu(menu, x, y);
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
    if ((e.target as Element).closest(".repo-item")) return;
    e.preventDefault();
    contextRepoTarget = null;
    showWidgetContextMenu(e.clientX, e.clientY);
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
  $("menu-repo-open").addEventListener("click", () => {
    const target = contextRepoTarget;
    hideContextMenu();
    if (target) void openRepoAtPath(target.path);
  });
  $("menu-repo-refresh").addEventListener("click", () => {
    const target = contextRepoTarget;
    hideContextMenu();
    if (target) void refreshOneRepo(target.name, target.path);
  });
  $("menu-repo-explorer").addEventListener("click", () => {
    const target = contextRepoTarget;
    hideContextMenu();
    if (target) void openRepoInExplorer(target.path);
  });

  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape") hideContextMenu();
  });

  try {
    await listen("config-saved", () => {
      void loadOpenTarget();
      void refresh({ doFetch: false });
    });
  } catch {
    /* browser preview */
  }

  await loadOpenTarget();
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
