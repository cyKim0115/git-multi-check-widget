import { invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import { getCurrentWebview } from "@tauri-apps/api/webview";
import type { RepoConfig, ValidateResult } from "./types";

let configDraft: RepoConfig = { repos: [] };
let lastValidate: ValidateResult | null = null;

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
    if (repo.url) {
      const url = document.createElement("span");
      url.className = "settings-repo-url";
      url.textContent = repo.url;
      meta.append(title, path, url);
    } else {
      meta.append(title, path);
    }

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

async function loadConfigDraft() {
  configDraft = (await invoke("get_config")) as RepoConfig;
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
  renderSettingsList();
  resetAddForm();
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

function applyDroppedPath(path: string) {
  const input = $("input-url") as HTMLInputElement;
  input.value = path;
  ($("btn-add") as HTMLButtonElement).disabled = true;
  lastValidate = null;
  $("test-result").textContent = "드롭된 경로를 테스트 중…";
  $("test-result").className = "test-result pending";
  void runTest();
}

function bindPathDropZone() {
  const zone = $("path-drop-zone");
  let dragOverZone = false;

  const setActive = (active: boolean) => {
    if (dragOverZone === active) return;
    dragOverZone = active;
    zone.classList.toggle("path-drop-zone--active", active);
  };

  void getCurrentWebview()
    .onDragDropEvent((event) => {
      const { payload } = event;

      if (payload.type === "enter" || payload.type === "over") {
        const position = "position" in payload ? payload.position : null;
        if (position) {
          setActive(isPointInElement(zone, position.x, position.y));
        }
        return;
      }

      if (payload.type === "leave") {
        setActive(false);
        return;
      }

      if (payload.type !== "drop") return;

      setActive(false);
      const position = payload.position;
      if (!isPointInElement(zone, position.x, position.y)) return;

      const path = payload.paths[0];
      if (!path) {
        $("test-result").textContent = "드롭된 항목에서 경로를 읽을 수 없습니다.";
        $("test-result").className = "test-result fail";
        return;
      }

      applyDroppedPath(path);
    })
    .catch(() => {
      /* browser preview without Tauri webview */
    });
}

function bindUi() {
  $("btn-test").addEventListener("click", () => void runTest());
  $("btn-add").addEventListener("click", () => addRepoFromForm());
  $("btn-save-close").addEventListener("click", () => void saveAndClose());
  $("btn-cancel").addEventListener("click", () => void closeWindow());
  bindPathDropZone();

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
