import type { RepoStatus } from "./types";

type IconKind = "dirty" | "push" | "pull" | "diverged" | "error" | "clean" | "no_upstream" | "pending";

const ICONS: Record<IconKind, string> = {
  dirty: "●",
  push: "↑",
  pull: "↓",
  diverged: "↕",
  error: "!",
  clean: "✓",
  no_upstream: "—",
  pending: "…",
};

function iconEl(kind: IconKind, count?: string, title?: string): HTMLSpanElement {
  const span = document.createElement("span");
  span.className = `status-icon status-icon--${kind}`;
  span.setAttribute("role", "img");
  span.setAttribute("aria-label", title ?? kind);

  const glyph = document.createElement("span");
  glyph.className = "status-icon-glyph";
  glyph.textContent = ICONS[kind];
  span.append(glyph);

  if (count && count.length > 0) {
    const num = document.createElement("span");
    num.className = "status-icon-count";
    num.textContent = count;
    span.append(num);
  }

  return span;
}

/** Primary row tint class (error > dirty > sync). */
export function rowClass(repo: RepoStatus): string {
  if (repo.loading && !repo.refreshing) return "pending";
  if (repo.error) return "error";
  if (repo.dirty) return "dirty";
  if (repo.sync_state === "diverged") return "diverged";
  if (repo.sync_state === "ahead") return "push";
  if (repo.sync_state === "behind") return "pull";
  if (repo.sync_state === "no_upstream" || repo.sync_state === "no_remote") return "muted";
  return "clean";
}

export function renderStatusIcons(repo: RepoStatus): HTMLElement {
  const wrap = document.createElement("span");
  wrap.className = "status-icons";

  if (repo.loading && !repo.refreshing) {
    wrap.append(iconEl("pending", undefined, "상태 확인 중"));
    return wrap;
  }

  if (repo.error) {
    wrap.append(iconEl("error", undefined, repo.error));
    return wrap;
  }

  if (repo.dirty) {
    wrap.append(
      iconEl("dirty", String(repo.changed_count), `로컬 변경 ${repo.changed_count}건`),
    );
  }

  switch (repo.sync_state) {
    case "ahead":
      wrap.append(iconEl("push", String(repo.ahead), `push ${repo.ahead}`));
      break;
    case "behind":
      wrap.append(iconEl("pull", String(repo.behind), `pull ${repo.behind}`));
      break;
    case "diverged":
      wrap.append(
        iconEl("diverged", `${repo.ahead}/${repo.behind}`, `diverged ${repo.ahead}/${repo.behind}`),
      );
      break;
    case "no_upstream":
      wrap.append(iconEl("no_upstream", undefined, "upstream 없음"));
      break;
    default:
      break;
  }

  if (wrap.childNodes.length === 0) {
    wrap.append(iconEl("clean", undefined, "clean"));
  }

  return wrap;
}

export function formatSummaryIcons(repos: RepoStatus[]): HTMLElement {
  const wrap = document.createElement("span");
  wrap.className = "summary-icons";

  const ready = repos.filter((r) => !r.loading);
  if (ready.length === 0) {
    wrap.append(iconEl("pending", undefined, "상태 확인 중"));
    return wrap;
  }

  const dirty = ready.filter((r) => r.dirty && !r.error).length;
  const push = ready.filter(
    (r) => !r.error && (r.sync_state === "ahead" || r.sync_state === "diverged"),
  ).length;
  const pull = ready.filter(
    (r) => !r.error && (r.sync_state === "behind" || r.sync_state === "diverged"),
  ).length;
  const err = ready.filter((r) => r.error).length;

  if (dirty > 0) wrap.append(iconEl("dirty", String(dirty), `${dirty} dirty`));
  if (push > 0) wrap.append(iconEl("push", String(push), `${push} push`));
  if (pull > 0) wrap.append(iconEl("pull", String(pull), `${pull} pull`));
  if (err > 0) wrap.append(iconEl("error", String(err), `${err} error`));
  if (wrap.childNodes.length === 0) wrap.append(iconEl("clean", undefined, "all clean"));

  return wrap;
}
