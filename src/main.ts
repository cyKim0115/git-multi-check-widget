import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import { formatSummaryIcons, renderStatusIcons, rowClass } from "./status-icons";
import { DEFAULT_OPEN_TARGET } from "./open-targets";
import type { OpenTarget } from "./open-targets";
import type { RepoConfig, RepoEntry, RepoStatus, Vcs } from "./types";

const PREVIEW = new URLSearchParams(window.location.search).has("preview");

let openTarget: OpenTarget = DEFAULT_OPEN_TARGET;

const PREVIEW_SCAN: RepoStatus[] = [
  {
    vcs: "git",
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
    vcs: "git",
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
    vcs: "git",
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

const PREVIEW_SVN_SCAN: RepoStatus[] = [
  {
    vcs: "svn",
    name: "ArtSource",
    path: "D:\\svn\\ArtSource",
    branch: "^/trunk/ArtSource",
    dirty: true,
    changed_count: 12,
    ahead: 0,
    behind: 4,
    sync_state: "behind",
    badge: "dirty ·12 · pull ·4",
    error: null,
  },
];

/** Fallbacks only — the real geometry is measured from the rendered DOM. */
const BASE_HEIGHT = 72;
const ROW_HEIGHT = 36;
/** Row budget for a single visible section vs. two stacked ones. */
const MAX_VISIBLE_ROWS = 5;
const MAX_VISIBLE_ROWS_SPLIT = 4;
const SCROLL_ACCEL_BASE = 0.07;
const SCROLL_ACCEL_STEP = 0.035;
const SCROLL_ACCEL_BURST_MAX = 7;
const WHEEL_BURST_MS = 140;
const SCROLL_FRICTION = 0.92;
const SCROLL_MIN_VELOCITY = 0.18;
const SCROLL_MAX_VELOCITY = 28;

let scanGeneration = 0;
let scrollBound = false;
let lastGitRepos: RepoStatus[] = [];
let lastSvnRepos: RepoStatus[] = [];
let svnEnabled = false;
let windowAutoSized = false;
let userResizedWindow = false;
let programmaticResize = false;
let lastAutoSizedSignature = "";
let contextRepoTarget: RepoStatus | null = null;

function $(id: string): HTMLElement {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing #${id}`);
  return el;
}

/**
 * Git and SVN each own a scroll viewport, so the inertia state that used to be
 * module-level is now per-area — otherwise one wheel gesture would drag both.
 */
type ScrollArea = {
  scroll: HTMLElement;
  wrap: HTMLElement;
  fadeTop: HTMLElement;
  fadeBottom: HTMLElement;
  velocity: number;
  burst: number;
  lastWheelAt: number;
  animId: number | null;
};

const scrollAreas: ScrollArea[] = [];

function createScrollArea(prefix: string): ScrollArea {
  return {
    scroll: $(`${prefix}-scroll`),
    wrap: $(`${prefix}-scroll-wrap`),
    fadeTop: $(`${prefix}-fade-top`),
    fadeBottom: $(`${prefix}-fade-bottom`),
    velocity: 0,
    burst: 0,
    lastWheelAt: 0,
    animId: null,
  };
}

function formatTime(unixSeconds: number): string {
  const date = new Date(unixSeconds * 1000);
  return date.toLocaleTimeString("ko-KR", { hour: "2-digit", minute: "2-digit" });
}

function placeholderStatus(entry: RepoEntry, vcs: Vcs): RepoStatus {
  return {
    vcs,
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

function renderRepoList(listId: string, repos: RepoStatus[]) {
  const list = $(listId);
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
}

function updateScrollFade() {
  requestAnimationFrame(() => {
    for (const area of scrollAreas) {
      const canScroll = area.scroll.scrollHeight > area.scroll.clientHeight + 1;
      const atTop = area.scroll.scrollTop <= 1;
      const atBottom =
        area.scroll.scrollTop + area.scroll.clientHeight >= area.scroll.scrollHeight - 1;

      area.fadeTop.classList.toggle("visible", canScroll && !atTop);
      area.fadeBottom.classList.toggle("visible", canScroll && !atBottom);
    }
  });
}

function maxScrollTop(scroll: HTMLElement): number {
  return Math.max(0, scroll.scrollHeight - scroll.clientHeight);
}

function clampScrollVelocity(velocity: number): number {
  return Math.max(-SCROLL_MAX_VELOCITY, Math.min(SCROLL_MAX_VELOCITY, velocity));
}

function ensureScrollAnimation(area: ScrollArea) {
  if (area.animId !== null) return;

  const tick = () => {
    if (Math.abs(area.velocity) < SCROLL_MIN_VELOCITY) {
      area.animId = null;
      area.velocity = 0;
      updateScrollFade();
      return;
    }

    const max = maxScrollTop(area.scroll);
    const prev = area.scroll.scrollTop;
    const next = Math.max(0, Math.min(max, prev + area.velocity));
    area.scroll.scrollTop = next;

    if (next !== prev) {
      area.velocity *= SCROLL_FRICTION;
    } else {
      area.velocity *= 0.4;
    }

    if (area.scroll.scrollTop <= 0 || area.scroll.scrollTop >= max) {
      area.velocity *= 0.55;
    }

    updateScrollFade();
    area.animId = requestAnimationFrame(tick);
  };

  area.animId = requestAnimationFrame(tick);
}

function applyWheelScroll(area: ScrollArea, deltaY: number) {
  const now = performance.now();
  if (now - area.lastWheelAt < WHEEL_BURST_MS) {
    area.burst = Math.min(SCROLL_ACCEL_BURST_MAX, area.burst + 1);
  } else {
    area.burst = 0;
  }
  area.lastWheelAt = now;

  const accel = SCROLL_ACCEL_BASE + area.burst * SCROLL_ACCEL_STEP;
  area.velocity = clampScrollVelocity(area.velocity + deltaY * accel);
  ensureScrollAnimation(area);
}

function setupRepoScroll() {
  if (scrollBound) return;
  scrollBound = true;

  scrollAreas.push(createScrollArea("repo"), createScrollArea("svn"));

  window.addEventListener("resize", () => {
    if (!programmaticResize && windowAutoSized) {
      userResizedWindow = true;
    }
    updateScrollFade();
  });

  for (const area of scrollAreas) {
    area.scroll.addEventListener("scroll", updateScrollFade, { passive: true });
    area.wrap.addEventListener(
      "wheel",
      (e) => {
        if (area.scroll.scrollHeight <= area.scroll.clientHeight + 1) return;
        e.preventDefault();
        applyWheelScroll(area, e.deltaY);
      },
      { passive: false },
    );
  }
}

function renderSummary() {
  const summaryEl = $("summary");
  summaryEl.replaceChildren(formatSummaryIcons([...lastGitRepos, ...lastSvnRepos]));
}

function visibleRowBudget(): number {
  return svnEnabled ? MAX_VISIBLE_ROWS_SPLIT : MAX_VISIBLE_ROWS;
}

function visibleRows(count: number): number {
  return Math.min(Math.max(1, count), visibleRowBudget());
}

function cssPx(value: string): number {
  const parsed = parseFloat(value);
  return Number.isFinite(parsed) ? parsed : 0;
}

/**
 * Row height used to be a constant that undershot the real row, so the window
 * was always a little too short and rows got clipped. Measuring the rendered
 * DOM keeps the window height and the CSS layout agreeing on one set of numbers.
 */
function measureLayout(): { chrome: number; row: number; label: number; gap: number } {
  const widget = document.querySelector<HTMLElement>(".widget");
  const sections = $("sections");

  const measuredChrome = widget ? widget.offsetHeight - sections.offsetHeight : 0;
  const chrome = measuredChrome > 0 ? measuredChrome : BASE_HEIGHT;

  const item = document.querySelector<HTMLElement>(".repo-item");
  const row =
    item && item.parentElement && item.offsetHeight > 0
      ? item.offsetHeight + cssPx(getComputedStyle(item.parentElement).rowGap)
      : ROW_HEIGHT;

  const labelEl = document.querySelector<HTMLElement>(".sections--split .section-label");
  const label = labelEl
    ? labelEl.offsetHeight + cssPx(getComputedStyle(labelEl).marginBottom)
    : 0;

  return { chrome, row, label, gap: cssPx(getComputedStyle(sections).rowGap) };
}

async function maybeAutoResizeWindow() {
  if (userResizedWindow) return;

  const gitCount = Math.max(1, lastGitRepos.length);
  const svnCount = Math.max(1, lastSvnRepos.length);
  const layout = measureLayout();
  const signature = `${gitCount}:${svnEnabled ? svnCount : 0}:${svnEnabled}:${Math.round(layout.row)}`;
  if (windowAutoSized && signature === lastAutoSizedSignature) return;

  let height = layout.chrome + visibleRows(gitCount) * layout.row;

  if (svnEnabled) {
    // Two labels appear only in split mode, plus the gap between sections.
    height += layout.label * 2 + layout.gap + visibleRows(svnCount) * layout.row;
  }

  try {
    programmaticResize = true;
    await invoke("resize_main_window", { height });
    windowAutoSized = true;
    lastAutoSizedSignature = signature;
  } catch {
    /* dev without tauri */
  } finally {
    programmaticResize = false;
  }
}

function renderAll() {
  $("sections").classList.toggle("sections--split", svnEnabled);
  $("svn-section").classList.toggle("hidden", !svnEnabled);

  // The window height is budgeted per section, so the sections must grow in the
  // same proportion — a plain `flex: 1` would split the space evenly instead.
  $("git-section").style.flexGrow = String(visibleRows(lastGitRepos.length));
  $("svn-section").style.flexGrow = String(visibleRows(lastSvnRepos.length));

  renderSummary();
  renderRepoList("repo-list", lastGitRepos);
  if (svnEnabled) {
    renderRepoList("svn-list", lastSvnRepos);
  }

  void maybeAutoResizeWindow();
  updateScrollFade();
}

function findPrevious(previous: RepoStatus[], entry: RepoEntry): RepoStatus | undefined {
  return previous.find((r) => r.name === entry.name && r.path === entry.path);
}

function rowsForRefresh(entries: RepoEntry[], previous: RepoStatus[], vcs: Vcs): RepoStatus[] {
  return entries.map((entry) => {
    const prev = findPrevious(previous, entry);
    if (prev && !prev.loading) {
      return { ...prev, refreshing: true };
    }
    return placeholderStatus(entry, vcs);
  });
}

/** Fork and Git Bash are git-only; an SVN row falls back to the explorer. */
function effectiveOpenTarget(vcs: Vcs): OpenTarget {
  if (vcs === "svn" && (openTarget === "fork" || openTarget === "git_bash")) {
    return "explorer";
  }
  return openTarget;
}

async function openRepoAtPath(path: string, vcs: Vcs) {
  const statusEl = $("status");
  try {
    await invoke("open_repo", { path, openTarget: effectiveOpenTarget(vcs) });
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

function scanCommandFor(vcs: Vcs): string {
  return vcs === "svn" ? "scan_one_svn_repo" : "scan_one_repo";
}

async function refreshOneRepo(target: RepoStatus) {
  const statusEl = $("status");
  const isSvn = target.vcs === "svn";
  const rows = isSvn ? [...lastSvnRepos] : [...lastGitRepos];
  const idx = rows.findIndex((r) => r.name === target.name && r.path === target.path);
  if (idx === -1) return;

  rows[idx] = { ...rows[idx], refreshing: true };
  if (isSvn) lastSvnRepos = rows;
  else lastGitRepos = rows;
  renderAll();
  statusEl.textContent = "refreshing…";

  try {
    const updated = (await invoke(scanCommandFor(target.vcs), {
      name: target.name,
      path: target.path,
      doFetch: true,
    })) as RepoStatus;
    rows[idx] = { ...updated, refreshing: false, loading: false };
    statusEl.textContent = `updated ${formatTime(Math.floor(Date.now() / 1000))}`;
  } catch (e) {
    rows[idx] = {
      ...rows[idx],
      loading: false,
      refreshing: false,
      error: String(e),
      badge: "ERR",
      sync_state: "error",
    };
    statusEl.textContent = String(e);
  }

  if (isSvn) lastSvnRepos = rows;
  else lastGitRepos = rows;
  renderAll();
}

async function loadOpenTarget() {
  try {
    const config = (await invoke("get_config")) as RepoConfig;
    openTarget = config.open_target ?? DEFAULT_OPEN_TARGET;
  } catch {
    /* dev without tauri */
  }
}

/** Scans one section in place, re-rendering after each row like the git flow always has. */
async function scanSection(
  entries: RepoEntry[],
  rows: RepoStatus[],
  vcs: Vcs,
  doFetch: boolean,
  generation: number,
  commit: (rows: RepoStatus[]) => void,
): Promise<boolean> {
  for (let i = 0; i < entries.length; i++) {
    if (generation !== scanGeneration) return false;

    const entry = entries[i];
    try {
      const updated = (await invoke(scanCommandFor(vcs), {
        name: entry.name,
        path: entry.path,
        doFetch,
      })) as RepoStatus;
      if (generation !== scanGeneration) return false;
      rows[i] = { ...updated, refreshing: false, loading: false };
    } catch (e) {
      if (generation !== scanGeneration) return false;
      rows[i] = {
        ...rows[i],
        loading: false,
        refreshing: false,
        error: String(e),
        badge: "ERR",
        sync_state: "error",
      };
    }
    commit(rows);
    renderAll();
  }
  return true;
}

async function refresh(options?: { doFetch?: boolean }) {
  const statusEl = $("status");
  const doFetch = options?.doFetch ?? false;
  const generation = ++scanGeneration;

  if (PREVIEW) {
    svnEnabled = true;
    lastGitRepos = PREVIEW_SCAN;
    lastSvnRepos = PREVIEW_SVN_SCAN;
    renderAll();
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

  svnEnabled = config.svn_enabled ?? false;
  const svnEntries = svnEnabled ? (config.svn_repos ?? []) : [];

  const gitRows = rowsForRefresh(config.repos, lastGitRepos, "git");
  const svnRows = rowsForRefresh(svnEntries, lastSvnRepos, "svn");
  lastGitRepos = gitRows;
  lastSvnRepos = svnRows;
  renderAll();

  const total = gitRows.length + svnRows.length;
  if (total === 0) {
    statusEl.textContent = "no repos";
    return;
  }
  statusEl.textContent = [...gitRows, ...svnRows].some((r) => r.refreshing)
    ? "refreshing…"
    : "scanning…";

  const gitDone = await scanSection(config.repos, gitRows, "git", doFetch, generation, (rows) => {
    lastGitRepos = rows;
  });
  if (!gitDone) return;

  const svnDone = await scanSection(svnEntries, svnRows, "svn", doFetch, generation, (rows) => {
    lastSvnRepos = rows;
  });
  if (!svnDone) return;

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
    if (target) void openRepoAtPath(target.path, target.vcs);
  });
  $("menu-repo-refresh").addEventListener("click", () => {
    const target = contextRepoTarget;
    hideContextMenu();
    if (target) void refreshOneRepo(target);
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
