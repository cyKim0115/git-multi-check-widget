import { invoke } from "@tauri-apps/api/core";
import { listen } from "@tauri-apps/api/event";
import type { PullFailure } from "./types";

const PREVIEW = new URLSearchParams(window.location.search).has("preview");

const PREVIEW_FAILURES: PullFailure[] = [
  {
    vcs: "git",
    name: "TeenipingTycoon",
    path: "C:\\Users\\cykim\\repo\\TeenipingTycoon",
    message:
      "충돌이 나서 머지를 취소했습니다 (merge --abort).\nCONFLICT (content): Merge conflict in Assets/Scripts/Game.cs\nAutomatic merge failed; fix conflicts and then commit the result.",
  },
  {
    vcs: "svn",
    name: "ArtSource",
    path: "D:\\svn\\ArtSource",
    message: "svn update 실패\nsvn: E155004: Run 'svn cleanup' to remove locks (type 'svn help cleanup' for details)",
  },
];

const VCS_LABEL: Record<PullFailure["vcs"], string> = { git: "Git", svn: "SVN" };

function $(id: string): HTMLElement {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing #${id}`);
  return el;
}

function render(failures: PullFailure[]) {
  $("pull-report-title").textContent = `풀하지 못한 저장소 ${failures.length}개`;

  const list = $("pull-report-list");
  list.replaceChildren();
  for (const failure of failures) {
    const li = document.createElement("li");
    li.className = "pull-report-item";

    const head = document.createElement("div");
    head.className = "pull-report-item-head";
    const chip = document.createElement("span");
    chip.className = `pull-report-vcs pull-report-vcs--${failure.vcs}`;
    chip.textContent = VCS_LABEL[failure.vcs];
    const name = document.createElement("strong");
    name.className = "pull-report-name";
    name.textContent = failure.name;
    head.append(chip, name);

    const path = document.createElement("span");
    path.className = "pull-report-path";
    path.textContent = failure.path;

    // Selectable so the git/svn output can be copied into a terminal or a search.
    const message = document.createElement("pre");
    message.className = "pull-report-message";
    message.textContent = failure.message;

    li.append(head, path, message);
    list.append(li);
  }
}

async function load() {
  if (PREVIEW) {
    render(PREVIEW_FAILURES);
    return;
  }
  render(await invoke<PullFailure[]>("get_pull_report"));
}

async function close() {
  try {
    await invoke("close_pull_report");
  } catch {
    /* browser preview */
  }
}

async function boot() {
  $("btn-pull-report-close").addEventListener("click", () => void close());
  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape") void close();
  });

  try {
    // The list rides on the event, so the popup never waits on a busy backend to repaint.
    await listen<PullFailure[]>("pull-report-open", (event) => render(event.payload));
  } catch {
    /* browser preview */
  }
  try {
    await load();
  } catch {
    /* nothing to show until the first pull fails */
  }
}

void boot();
