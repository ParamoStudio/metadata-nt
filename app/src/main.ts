/**
 * MAT2 Wrapper frontend entry point.
 *
 * Hard rules for this codebase (HANDOFF.md §18, IMPLEMENTATION_PLAN.md Task 6/12):
 * - untrusted text (filenames, process output) is rendered exclusively through
 *   textContent / createElement — never innerHTML, never insertAdjacentHTML;
 * - no remote assets, no fetch/XHR/WebSocket — the app is offline by design;
 * - no document previews or thumbnails.
 */

import { listen } from "@tauri-apps/api/event";
import {
  initialState,
  refreshSelection,
  requestRemove,
  requestSelectFiles,
  requestSelectFolder,
} from "./state";
import type { PublicSelectedFile } from "./types";

const state = initialState();
const checkedIds = new Set<string>();

function el(id: string): HTMLElement | null {
  return document.getElementById(id);
}

function statusClass(status: PublicSelectedFile["status"]): string {
  switch (status) {
    case "Processed":
      return "success";
    case "Warning":
      return "warning";
    case "Failed":
    case "Unsupported":
      return "error";
    default:
      return "";
  }
}

function renderFileList(): void {
  const list = el("file-list") as HTMLUListElement | null;
  const dropZone = el("drop-zone");
  if (!list) return;
  list.replaceChildren();

  if (dropZone) {
    dropZone.classList.toggle("hidden", state.files.length > 0);
  }

  for (const file of state.files) {
    const li = document.createElement("li");
    li.dataset.id = file.id;

    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.checked = checkedIds.has(file.id) || file.status === "Ready";
    checkbox.setAttribute("aria-label", `Include ${file.display_name}`);
    checkbox.addEventListener("change", () => {
      if (checkbox.checked) {
        checkedIds.add(file.id);
      } else {
        checkedIds.delete(file.id);
      }
      updateProcessButton();
    });
    if (checkbox.checked) checkedIds.add(file.id);

    const name = document.createElement("span");
    name.className = "file-name";
    name.textContent = file.relative_path ?? file.display_name;

    const status = document.createElement("span");
    status.className = `file-status ${statusClass(file.status)}`.trim();
    status.textContent = file.status;

    li.append(checkbox, name, status);
    list.append(li);
  }
  updateProcessButton();
}

function updateProcessButton(): void {
  const btn = el("btn-process") as HTMLButtonElement | null;
  if (!btn) return;
  const count = [...checkedIds].filter((id) =>
    state.files.some((f) => f.id === id),
  ).length;
  btn.textContent = count === 1 ? "PROCESS 1 FILE" : `PROCESS ${count} FILES`;
  btn.disabled = count === 0;
}

async function refresh(): Promise<void> {
  await refreshSelection(state);
  for (const id of [...checkedIds]) {
    if (!state.files.some((f) => f.id === id)) checkedIds.delete(id);
  }
  renderFileList();
}

function bindButton(id: string, handler: () => void): void {
  el(id)?.addEventListener("click", handler);
}

function boot(): void {
  const statusValue = el("mat2-status-value");
  if (statusValue) {
    statusValue.textContent = "Not connected (Task 15 wires diagnostics)";
  }

  bindButton("btn-add-files", () => void requestSelectFiles());
  bindButton("btn-add-folder", () => void requestSelectFolder());
  bindButton("btn-remove-selected", () =>
    void requestRemove([...checkedIds]).then(refresh),
  );

  const dropZone = el("drop-zone");
  dropZone?.addEventListener("dragover", (e) => {
    e.preventDefault();
    dropZone.classList.add("dragover");
  });
  dropZone?.addEventListener("dragleave", () => {
    dropZone.classList.remove("dragover");
  });

  void listen("selection-changed", () => void refresh());
  void refresh();
}

document.addEventListener("DOMContentLoaded", boot);
