import { invoke } from "@tauri-apps/api/core";

interface RepoStatus {
  name: string;
  path: string;
  branch: string | null;
  dirty: boolean;
  changed_count: number;
  error: string | null;
}

interface ScanResult {
  repos: RepoStatus[];
  scanned_at: string;
}

const POLL_INTERVAL_MS = 300_000;

const repoList = document.getElementById("repo-list")!;
const summaryEl = document.getElementById("summary")!;
const statusEl = document.getElementById("status")!;
const backdrop = document.getElementById("context-backdrop")!;
const menu = document.getElementById("context-menu")!;
const menuRefresh = document.getElementById("menu-refresh")!;
const menuQuit = document.getElementById("menu-quit")!;

function formatTime(value: string): string {
  try {
    const n = Number(value);
    const date = Number.isFinite(n) ? new Date(n * 1000) : new Date(value);
    return date.toLocaleTimeString("ko-KR", {
      hour: "2-digit",
      minute: "2-digit",
    });
  } catch {
    return value;
  }
}

function renderRepos(repos: RepoStatus[]) {
  repoList.replaceChildren();
  for (const repo of repos) {
    const li = document.createElement("li");
    li.className = "repo-item";

    if (repo.error) {
      li.classList.add("error");
    } else if (repo.dirty) {
      li.classList.add("dirty");
    } else {
      li.classList.add("clean");
    }

    const name = document.createElement("span");
    name.className = "repo-name";
    name.textContent = repo.name;

    const branch = document.createElement("span");
    branch.className = "repo-branch";
    if (repo.error) {
      branch.textContent = repo.error;
    } else {
      branch.textContent = repo.branch ?? "no branch";
    }

    const badge = document.createElement("span");
    badge.className = "repo-badge";
    if (repo.error) {
      badge.classList.add("error");
      badge.textContent = "ERR";
    } else if (repo.dirty) {
      badge.classList.add("dirty");
      badge.textContent = `${repo.changed_count} changed`;
    } else {
      badge.classList.add("clean");
      badge.textContent = "clean";
    }

    li.append(name, branch, badge);
    repoList.append(li);
  }

  const dirtyCount = repos.filter((r) => r.dirty && !r.error).length;
  const errorCount = repos.filter((r) => r.error).length;
  summaryEl.textContent = `${dirtyCount} dirty · ${errorCount} err · ${repos.length} repos`;
}

async function refresh() {
  statusEl.textContent = "scanning…";
  try {
    const result = (await invoke("scan_repos")) as ScanResult;
    renderRepos(result.repos);
    statusEl.textContent = `updated ${formatTime(result.scanned_at)}`;
  } catch (e) {
    statusEl.textContent = `error: ${String(e)}`;
  }
}

function hideMenu() {
  backdrop.classList.add("hidden");
  menu.classList.add("hidden");
}

function showMenu(x: number, y: number) {
  backdrop.classList.remove("hidden");
  menu.classList.remove("hidden");
  menu.style.left = `${x}px`;
  menu.style.top = `${y}px`;
}

document.addEventListener("contextmenu", (e) => {
  e.preventDefault();
  showMenu(e.clientX, e.clientY);
});

backdrop.addEventListener("click", hideMenu);
menuRefresh.addEventListener("click", () => {
  hideMenu();
  refresh();
});
menuQuit.addEventListener("click", async () => {
  hideMenu();
  await invoke("quit_app");
});

refresh();
setInterval(refresh, POLL_INTERVAL_MS);
