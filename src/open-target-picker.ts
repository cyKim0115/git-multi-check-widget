import { DEFAULT_OPEN_TARGET, OPEN_TARGET_OPTIONS } from "./open-targets";
import type { OpenTarget } from "./open-targets";

export function renderOpenTargetPicker(
  container: HTMLElement,
  selected: OpenTarget,
  onChange: (target: OpenTarget) => void,
) {
  container.replaceChildren();

  const grid = document.createElement("div");
  grid.className = "open-target-grid";

  for (const option of OPEN_TARGET_OPTIONS) {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.className = "open-target-option";
    btn.setAttribute("aria-pressed", option.id === selected ? "true" : "false");
    if (option.id === selected) {
      btn.classList.add("open-target-option--selected");
    }

    const icon = document.createElement("img");
    icon.className = "open-target-icon";
    icon.src = option.icon;
    icon.alt = "";
    icon.width = 24;
    icon.height = 24;

    const label = document.createElement("span");
    label.className = "open-target-label";
    label.textContent = option.label;

    btn.title = option.description;
    btn.append(icon, label);
    btn.addEventListener("click", () => {
      onChange(option.id);
      renderOpenTargetPicker(container, option.id, onChange);
    });

    grid.append(btn);
  }

  container.append(grid);
}

export function normalizeOpenTarget(value: OpenTarget | undefined): OpenTarget {
  if (value && OPEN_TARGET_OPTIONS.some((o) => o.id === value)) {
    return value;
  }
  return DEFAULT_OPEN_TARGET;
}
