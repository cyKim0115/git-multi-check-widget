import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import { normalizeOpenTarget, renderOpenTargetPicker } from "./open-target-picker";
import { DEFAULT_OPEN_TARGET } from "./open-targets";
import type { RepoConfig, RepoEntry, ValidateResult, Vcs } from "./types";

let configDraft: RepoConfig = {
  repos: [],
  open_target: DEFAULT_OPEN_TARGET,
  svn_enabled: false,
  svn_repos: [],
};
let autostartBusy = false;

/**
 * Git and SVN keep separate lists and separate add forms, so every list/form
 * helper is addressed by kind instead of being duplicated.
 */
type KindUi = {
  listId: string;
  emptyId: string;
  nameInputId: string;
  pathInputId: string;
  addBtnId: string;
  resultId: string;
  dropZoneId: string;
  validateCommand: string;
};

const UI: Record<Vcs, KindUi> = {
  git: {
    listId: "settings-repo-list",
    emptyId: "settings-empty",
    nameInputId: "input-name",
    pathInputId: "input-url",
    addBtnId: "btn-add",
    resultId: "test-result",
    dropZoneId: "path-drop-zone",
    validateCommand: "validate_repo",
  },
  svn: {
    listId: "settings-svn-list",
    emptyId: "settings-svn-empty",
    nameInputId: "input-svn-name",
    pathInputId: "input-svn-path",
    addBtnId: "btn-svn-add",
    resultId: "svn-test-result",
    dropZoneId: "svn-path-drop-zone",
    validateCommand: "validate_svn_repo",
  },
};

const lastValidate: Record<Vcs, ValidateResult | null> = { git: null, svn: null };

function entriesFor(kind: Vcs): RepoEntry[] {
  if (kind === "svn") {
    configDraft.svn_repos ??= [];
    return configDraft.svn_repos;
  }
  return configDraft.repos;
}

async function getAutostartEnabled(): Promise<boolean> {
  try {
    return await invoke<boolean>("is_autostart_enabled");
  } catch {
    return false;
  }
}

async function getIsDevBuild(): Promise<boolean> {
  try {
    return await invoke<boolean>("is_dev_build");
  } catch {
    return false;
  }
}

async function renderAutostartSettings() {
  const checkbox = $("autostart-checkbox") as HTMLInputElement;
  const hint = $("autostart-hint");
  const isDevBuild = await getIsDevBuild();

  if (isDevBuild) {
    checkbox.checked = false;
    checkbox.disabled = true;
    hint.textContent = "개발 빌드에서는 사용할 수 없습니다. 「시작.bat」으로 설치한 뒤 릴리스에서 설정하세요.";
    hint.classList.remove("hidden");
    return;
  }

  checkbox.disabled = autostartBusy;
  hint.classList.add("hidden");
  checkbox.checked = await getAutostartEnabled();
}

async function setAutostart(enabled: boolean) {
  if (autostartBusy) return;
  autostartBusy = true;
  const checkbox = $("autostart-checkbox") as HTMLInputElement;
  checkbox.disabled = true;

  try {
    if (enabled) {
      await invoke("enable_autostart");
    } else {
      await invoke("disable_autostart");
    }
    checkbox.checked = await getAutostartEnabled();
  } catch (e) {
    checkbox.checked = await getAutostartEnabled();
    window.alert(String(e));
  } finally {
    autostartBusy = false;
    const isDevBuild = await getIsDevBuild();
    checkbox.disabled = isDevBuild;
  }
}

function $(id: string): HTMLElement {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing #${id}`);
  return el;
}

function showBootError(message: string) {
  document.body.innerHTML = `
    <div class="settings-window" style="padding:24px">
      <h1>설정 로드 실패</h1>
      <p class="test-result fail">${message}</p>
      <button id="btn-error-close" class="btn secondary wide" type="button">닫기</button>
    </div>`;
  document.getElementById("btn-error-close")?.addEventListener("click", () => {
    void closeWindow();
  });
}

function reorderEntries(kind: Vcs, fromIndex: number, toIndex: number) {
  if (fromIndex === toIndex) return;
  const entries = entriesFor(kind);
  const [entry] = entries.splice(fromIndex, 1);
  let insertAt = toIndex;
  if (toIndex > fromIndex) insertAt -= 1;
  entries.splice(insertAt, 0, entry);
}

function getRepoDropIndex(list: HTMLElement, clientY: number): number {
  const items = [...list.querySelectorAll<HTMLElement>(".settings-repo-item")];
  for (let i = 0; i < items.length; i++) {
    const rect = items[i].getBoundingClientRect();
    if (clientY < rect.top + rect.height / 2) return i;
  }
  return items.length;
}

function bindRepoReorderHandle(kind: Vcs, handle: HTMLButtonElement, fromIndex: number) {
  handle.addEventListener("pointerdown", (e) => {
    if (e.button !== 0) return;
    e.preventDefault();

    const list = $(UI[kind].listId);
    const pointerId = e.pointerId;
    const items = () => [...list.querySelectorAll<HTMLElement>(".settings-repo-item")];
    let dropIndex = fromIndex;

    handle.setPointerCapture(pointerId);
    items()[fromIndex]?.classList.add("settings-repo-item--dragging");

    const clearMarkers = () => {
      items().forEach((el) => {
        el.classList.remove("settings-repo-item--dragging", "settings-repo-item--drop-target");
      });
    };

    const onMove = (moveEvent: PointerEvent) => {
      if (moveEvent.pointerId !== pointerId) return;
      dropIndex = getRepoDropIndex(list, moveEvent.clientY);
      const markerIndex = Math.min(dropIndex, items().length - 1);
      items().forEach((el, i) => {
        el.classList.toggle(
          "settings-repo-item--drop-target",
          dropIndex !== fromIndex && i === markerIndex,
        );
      });
    };

    const onUp = (upEvent: PointerEvent) => {
      if (upEvent.pointerId !== pointerId) return;
      handle.releasePointerCapture(pointerId);
      window.removeEventListener("pointermove", onMove);
      window.removeEventListener("pointerup", onUp);
      clearMarkers();

      dropIndex = getRepoDropIndex(list, upEvent.clientY);
      if (dropIndex === fromIndex) return;
      reorderEntries(kind, fromIndex, dropIndex);
      renderSettingsList(kind);
    };

    window.addEventListener("pointermove", onMove);
    window.addEventListener("pointerup", onUp);
  });
}

function renderSettingsList(kind: Vcs) {
  const ui = UI[kind];
  const list = $(ui.listId);
  const empty = $(ui.emptyId);
  const entries = entriesFor(kind);
  list.replaceChildren();

  if (entries.length === 0) {
    empty.classList.remove("hidden");
    return;
  }
  empty.classList.add("hidden");

  entries.forEach((repo, index) => {
    const li = document.createElement("li");
    li.className = "settings-repo-item";
    li.dataset.index = String(index);

    const dragHandle = document.createElement("button");
    dragHandle.type = "button";
    dragHandle.className = "repo-drag-handle";
    dragHandle.setAttribute("aria-label", `${repo.name} 순서 변경`);
    dragHandle.textContent = "⋮⋮";
    bindRepoReorderHandle(kind, dragHandle, index);

    const meta = document.createElement("div");
    meta.className = "settings-repo-meta";
    const title = document.createElement("strong");
    title.textContent = repo.name;
    const path = document.createElement("span");
    path.textContent = repo.path;
    if (repo.url) {
      const url = document.createElement("span");
      url.className = "settings-repo-url";
      url.textContent = repo.url;
      meta.append(title, path, url);
    } else {
      meta.append(title, path);
    }

    const actions = document.createElement("div");
    actions.className = "settings-repo-actions";

    const remove = document.createElement("button");
    remove.type = "button";
    remove.className = "btn danger small";
    remove.textContent = "삭제";
    remove.addEventListener("click", () => {
      entriesFor(kind).splice(index, 1);
      renderSettingsList(kind);
    });

    actions.append(remove);
    li.append(dragHandle, meta, actions);
    list.append(li);
  });
}

function resetAddForm(kind: Vcs) {
  const ui = UI[kind];
  ($(ui.nameInputId) as HTMLInputElement).value = "";
  ($(ui.pathInputId) as HTMLInputElement).value = "";
  ($(ui.addBtnId) as HTMLButtonElement).disabled = true;
  lastValidate[kind] = null;
  $(ui.resultId).textContent = "테스트로 유효성을 확인하세요.";
  $(ui.resultId).className = "test-result";
}

async function loadConfigDraft() {
  configDraft = (await invoke("get_config")) as RepoConfig;
  configDraft.open_target = normalizeOpenTarget(configDraft.open_target);
  configDraft.svn_enabled ??= false;
  configDraft.svn_repos ??= [];
}

function renderOpenTargetSettings() {
  renderOpenTargetPicker($("open-target-picker"), configDraft.open_target ?? DEFAULT_OPEN_TARGET, (target) => {
    configDraft.open_target = target;
  });
}

function renderSvnSettings() {
  const checkbox = $("svn-enabled-checkbox") as HTMLInputElement;
  checkbox.checked = configDraft.svn_enabled ?? false;
  $("svn-settings-body").classList.toggle("hidden", !checkbox.checked);
}

async function runTest(kind: Vcs) {
  const ui = UI[kind];
  const input = ($(ui.pathInputId) as HTMLInputElement).value.trim();
  const resultEl = $(ui.resultId);
  const addBtn = $(ui.addBtnId) as HTMLButtonElement;
  resultEl.textContent = "테스트 중…";
  resultEl.className = "test-result pending";
  addBtn.disabled = true;
  lastValidate[kind] = null;

  try {
    const result = (await invoke(ui.validateCommand, { input })) as ValidateResult;
    lastValidate[kind] = result;
    resultEl.textContent = result.message;
    resultEl.className = `test-result ${result.ok ? "ok" : "fail"}`;

    if (result.ok) {
      addBtn.disabled = false;
      const nameInput = $(ui.nameInputId) as HTMLInputElement;
      if (!nameInput.value.trim() && result.suggested_name) {
        nameInput.value = result.suggested_name;
      }
    }
  } catch (e) {
    resultEl.textContent = String(e);
    resultEl.className = "test-result fail";
  }
}

function addRepoFromForm(kind: Vcs) {
  const ui = UI[kind];
  const validated = lastValidate[kind];
  if (!validated?.ok || !validated.resolved_path) return;

  const nameInput = ($(ui.nameInputId) as HTMLInputElement).value.trim();
  const name = nameInput || validated.suggested_name || "repo";
  const path = validated.resolved_path;
  const entries = entriesFor(kind);

  if (entries.some((r) => r.path.toLowerCase() === path.toLowerCase())) {
    $(ui.resultId).textContent = "이미 등록된 경로입니다.";
    $(ui.resultId).className = "test-result fail";
    return;
  }

  entries.push({ name, path, url: validated.remote_url });
  renderSettingsList(kind);
  resetAddForm(kind);
}

async function closeWindow() {
  try {
    await invoke("close_settings_window");
  } catch {
    /* browser preview */
  }
}

async function saveAndClose() {
  await invoke("set_config", { config: configDraft });
  await emit("config-saved");
  await closeWindow();
}

async function refreshView() {
  await loadConfigDraft();
  renderSettingsList("git");
  renderSettingsList("svn");
  renderOpenTargetSettings();
  renderSvnSettings();
  await renderAutostartSettings();
  resetAddForm("git");
  resetAddForm("svn");
}

function isPointInElement(el: HTMLElement, x: number, y: number): boolean {
  const rect = el.getBoundingClientRect();
  const dpr = window.devicePixelRatio || 1;
  const logicalX = x / dpr;
  const logicalY = y / dpr;
  return (
    logicalX >= rect.left &&
    logicalX <= rect.right &&
    logicalY >= rect.top &&
    logicalY <= rect.bottom
  );
}

function applyDroppedPath(kind: Vcs, path: string) {
  const ui = UI[kind];
  ($(ui.pathInputId) as HTMLInputElement).value = path;
  ($(ui.addBtnId) as HTMLButtonElement).disabled = true;
  lastValidate[kind] = null;
  $(ui.resultId).textContent = "드롭된 경로를 테스트 중…";
  $(ui.resultId).className = "test-result pending";
  void runTest(kind);
}

/** Both add forms accept folder drops, so the drop is routed by hit-test. */
function bindPathDropZones() {
  const zones = (Object.keys(UI) as Vcs[]).map((kind) => ({
    kind,
    el: $(UI[kind].dropZoneId),
    active: false,
  }));

  const setActive = (zone: (typeof zones)[number], active: boolean) => {
    if (zone.active === active) return;
    zone.active = active;
    zone.el.classList.toggle("path-drop-zone--active", active);
  };

  const visible = (zone: (typeof zones)[number]) => zone.el.offsetParent !== null;

  void getCurrentWebview()
    .onDragDropEvent((event) => {
      const { payload } = event;

      if (payload.type === "enter" || payload.type === "over") {
        const position = "position" in payload ? payload.position : null;
        if (position) {
          for (const zone of zones) {
            setActive(zone, visible(zone) && isPointInElement(zone.el, position.x, position.y));
          }
        }
        return;
      }

      if (payload.type === "leave") {
        zones.forEach((zone) => setActive(zone, false));
        return;
      }

      if (payload.type !== "drop") return;

      const position = payload.position;
      const hit = zones.find((zone) => visible(zone) && isPointInElement(zone.el, position.x, position.y));
      zones.forEach((zone) => setActive(zone, false));
      if (!hit) return;

      const path = payload.paths[0];
      if (!path) {
        $(UI[hit.kind].resultId).textContent = "드롭된 항목에서 경로를 읽을 수 없습니다.";
        $(UI[hit.kind].resultId).className = "test-result fail";
        return;
      }

      applyDroppedPath(hit.kind, path);
    })
    .catch(() => {
      /* browser preview without Tauri webview */
    });
}

function bindUi() {
  $("btn-test").addEventListener("click", () => void runTest("git"));
  $("btn-add").addEventListener("click", () => addRepoFromForm("git"));
  $("btn-svn-test").addEventListener("click", () => void runTest("svn"));
  $("btn-svn-add").addEventListener("click", () => addRepoFromForm("svn"));
  $("btn-save-close").addEventListener("click", () => void saveAndClose());
  $("btn-cancel").addEventListener("click", () => void closeWindow());
  bindPathDropZones();

  const svnCheckbox = $("svn-enabled-checkbox") as HTMLInputElement;
  svnCheckbox.addEventListener("change", () => {
    configDraft.svn_enabled = svnCheckbox.checked;
    renderSvnSettings();
  });

  const autostartCheckbox = $("autostart-checkbox") as HTMLInputElement;
  autostartCheckbox.addEventListener("change", () => {
    void setAutostart(autostartCheckbox.checked);
  });

  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape") void closeWindow();
  });
}

async function boot() {
  try {
    await refreshView();
    bindUi();
    await listen("settings-open", () => {
      void refreshView();
    });
  } catch (e) {
    showBootError(String(e));
  }
}

void boot();
