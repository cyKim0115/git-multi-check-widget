import { invoke } from "@tauri-apps/api/core";
import { emit } from "@tauri-apps/api/event";
import { getCurrentWebviewWindow } from "@tauri-apps/api/webviewWindow";
import type { RepoConfig, ValidateResult } from "./types";

let configDraft: RepoConfig = { repos: [] };
let lastValidate: ValidateResult | null = null;

function $(id: string): HTMLElement {
  const el = document.getElementById(id);
  if (!el) throw new Error(`missing #${id}`);
  return el;
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
    await getCurrentWebviewWindow().close();
  } catch {
    window.close();
  }
}

async function saveAndClose() {
  await invoke("set_config", { config: configDraft });
  await emit("config-saved");
  await closeWindow();
}

async function boot() {
  configDraft = (await invoke("get_config")) as RepoConfig;
  renderSettingsList();
  resetAddForm();

  $("btn-test").addEventListener("click", () => void runTest());
  $("btn-add").addEventListener("click", () => addRepoFromForm());
  $("btn-save-close").addEventListener("click", () => void saveAndClose());
  $("btn-cancel").addEventListener("click", () => void closeWindow());

  window.addEventListener("keydown", (e) => {
    if (e.key === "Escape") void closeWindow();
  });
}

void boot();
