/**
 * MAT2 Wrapper frontend entry point.
 *
 * Hard rules for this codebase (HANDOFF.md §18, IMPLEMENTATION_PLAN.md Task 6/12):
 * - untrusted text (filenames, process output) is rendered exclusively through
 *   textContent / createElement — never innerHTML, never insertAdjacentHTML;
 * - no remote assets, no fetch/XHR/WebSocket — the app is offline by design;
 * - no document previews or thumbnails.
 */

import * as ipc from "./state";
import type {
  FileJobResult,
  FileStatus,
  InspectionDto,
  JobSettingsDto,
  PublicSelectedFile,
  SyntheticOptions,
  TripwireOptions,
} from "./types";

interface UiState {
  files: PublicSelectedFile[];
  results: Map<string, FileJobResult>;
  checked: Set<string>;
  filter: string;
  selectedId: string | null;
  inspections: Map<string, InspectionDto>;
  inspectingId: string | null;
  jobRunning: boolean;
  customRootName: string | null;
  lastJobId: string | null;
  hasCommittedOutputs: boolean;
  runtimeFatal: boolean;
}

const state: UiState = {
  files: [],
  results: new Map(),
  checked: new Set(),
  filter: "ALL",
  selectedId: null,
  inspections: new Map(),
  inspectingId: null,
  jobRunning: false,
  customRootName: null,
  lastJobId: null,
  hasCommittedOutputs: false,
  runtimeFatal: true,
};

const MAX_LOG_LINES = 500;

// Session-only tripwire flags (spec §5: no persistent acknowledgement; both
// reset on every launch because nothing is stored).
let tripwireDisclosedThisSession = false;
let tripwireConfirmed = false;

const EMAIL_SHAPE = /^[^\s@]+@[^\s@]+\.[^\s@]{2,}$/;

let confirmResolver: ((value: boolean) => void) | null = null;

function confirmDialog(title: string, text: string, okLabel: string): Promise<boolean> {
  return new Promise((resolve) => {
    confirmResolver = resolve;
    mustEl("confirm-title").textContent = title;
    mustEl("confirm-text").textContent = text;
    const ok = mustEl("confirm-ok") as HTMLButtonElement;
    ok.textContent = okLabel;
    mustEl("confirm-overlay").classList.remove("hidden");
    ok.focus();
  });
}

function closeConfirm(result: boolean): void {
  mustEl("confirm-overlay").classList.add("hidden");
  const resolve = confirmResolver;
  confirmResolver = null;
  if (resolve) resolve(result);
}

function el(id: string): HTMLElement | null {
  return document.getElementById(id);
}

function mustEl(id: string): HTMLElement {
  const node = el(id);
  if (!node) throw new Error(`missing element #${id}`);
  return node;
}

function inputEl(id: string): HTMLInputElement | null {
  return el(id) as HTMLInputElement | null;
}

function statusClass(status: FileStatus): string {
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

function extKey(f: PublicSelectedFile): string {
  return (f.extension ?? "OTHER").toUpperCase();
}

function looksPreviouslyProcessed(f: PublicSelectedFile): boolean {
  const name = f.display_name.toLowerCase();
  const rel = (f.relative_path ?? "").toLowerCase();
  return (
    name.includes(".cleaned.") ||
    name.includes(".cleaned-") ||
    rel.includes("mat2 output")
  );
}

function localStamp(): string {
  const d = new Date();
  const p = (n: number) => String(n).padStart(2, "0");
  return `${p(d.getHours())}:${p(d.getMinutes())}:${p(d.getSeconds())}`;
}

function appendLog(line: string): void {
  const log = mustEl("log");
  log.appendChild(document.createTextNode(`${line}\n`));
  while (log.childNodes.length > MAX_LOG_LINES) {
    log.removeChild(log.firstChild!);
  }
  log.scrollTop = log.scrollHeight;
}

function logInfo(message: string): void {
  appendLog(`[${localStamp()}] ${message}`);
}

function logError(context: string, err: unknown): void {
  appendLog(`[${localStamp()}] ERROR ${context}: ${String(err)}`);
}

function visibleFiles(): PublicSelectedFile[] {
  if (state.filter === "ALL") return state.files;
  return state.files.filter((f) => extKey(f) === state.filter);
}

function checkedCount(): number {
  return state.files.filter((f) => state.checked.has(f.id)).length;
}

function renderFilters(): void {
  const wrap = mustEl("extension-filters");
  wrap.replaceChildren();
  if (state.files.length === 0) {
    mustEl("group-toggles").classList.add("hidden");
    return;
  }
  const counts = new Map<string, number>();
  for (const f of state.files) {
    const k = extKey(f);
    counts.set(k, (counts.get(k) ?? 0) + 1);
  }
  const makeChip = (label: string, key: string, count: number) => {
    const btn = document.createElement("button");
    btn.type = "button";
    btn.textContent = `${label} ${count}`;
    if (state.filter === key) btn.classList.add("active");
    btn.addEventListener("click", () => {
      state.filter = key;
      renderFilters();
      renderFiles();
    });
    return btn;
  };
  wrap.append(makeChip("ALL", "ALL", state.files.length));
  for (const key of [...counts.keys()].sort()) {
    wrap.append(makeChip(key, key, counts.get(key)!));
  }
  mustEl("group-toggles").classList.toggle("hidden", state.filter === "ALL");
}

function renderFiles(): void {
  const list = mustEl("file-list") as HTMLUListElement;
  const dropZone = mustEl("drop-zone");
  list.replaceChildren();
  dropZone.classList.toggle("hidden", state.files.length > 0);
  mustEl("file-list-header").classList.toggle("hidden", state.files.length === 0);
  const countEl = el("file-list-count");
  if (countEl) {
    const n = state.files.length;
    countEl.textContent = `${n} file${n === 1 ? "" : "s"} · ${checkedCount()} checked`;
  }

  for (const file of visibleFiles()) {
    const li = document.createElement("li");
    li.dataset.id = file.id;
    li.tabIndex = 0;
    if (file.id === state.selectedId) li.classList.add("selected");

    const checkbox = document.createElement("input");
    checkbox.type = "checkbox";
    checkbox.checked = state.checked.has(file.id);
    checkbox.setAttribute("aria-label", `Include ${file.display_name}`);
    checkbox.addEventListener("change", () => {
      if (checkbox.checked) {
        state.checked.add(file.id);
      } else {
        state.checked.delete(file.id);
      }
      updateProcessBar();
    });

    const name = document.createElement("span");
    name.className = "file-name";
    name.textContent = file.relative_path ?? file.display_name;
    name.title = file.relative_path ?? file.display_name;

    const status = document.createElement("span");
    status.className = `file-status ${statusClass(file.status)}`.trim();
    status.textContent = file.status;

    li.append(checkbox, name, status);

    if (looksPreviouslyProcessed(file)) {
      const note = document.createElement("span");
      note.className = "file-note";
      note.textContent = "Appears previously processed";
      li.append(note);
    }

    const select = () => selectRow(file.id);
    li.addEventListener("click", (e) => {
      if (e.target !== checkbox) select();
    });
    li.addEventListener("keydown", (e) => {
      if (e.key === "Enter" || e.key === " ") {
        e.preventDefault();
        select();
      }
    });
    list.append(li);
  }
  updateProcessBar();
}

function selectRow(id: string): void {
  state.selectedId = id;
  renderFiles();
  void renderInspection();
}

function setJobRunning(running: boolean): void {
  state.jobRunning = running;
  mustEl("btn-process").classList.toggle("hidden", running);
  mustEl("btn-cancel").classList.toggle("hidden", !running);
  const lockIds = [
    "btn-add-files",
    "btn-add-folder",
    "btn-remove-selected",
    "btn-inspect-only",
    "btn-choose-output",
    "btn-synth-preview",
    "btn-tripwire-enable",
    "btn-tripwire-cancel",
    "diag-version",
    "diag-formats",
    "diag-deps",
    "diag-help",
  ];
  for (const id of lockIds) {
    const node = inputEl(id);
    if (node) node.disabled = running;
  }
  for (const scope of [mustEl("settings-section"), mustEl("advanced-overlay")]) {
    for (const input of scope.querySelectorAll<HTMLInputElement | HTMLSelectElement>(
      "input, select",
    )) {
      input.disabled = running;
    }
  }
  updateProcessBar();
}

function updateProcessBar(): void {
  const btn = inputEl("btn-process");
  if (!btn) return;
  const count = checkedCount();
  btn.textContent = count === 1 ? "PROCESS 1 FILE" : `PROCESS ${count} FILES`;
  btn.disabled = count === 0 || state.jobRunning || state.runtimeFatal;
}

async function runStartupDiagnostics(): Promise<void> {
  const statusEl = el("mat2-status-value");
  const versionEl = el("version-info");
  try {
    const d = await ipc.runtimeDiagnostics();
    state.runtimeFatal = d.fatal;
    if (d.fatal) {
      if (statusEl) {
        statusEl.textContent = "Problem";
        statusEl.className = "problem";
      }
      const reason = d.available
        ? `MAT2 runtime is incomplete\nMissing required dependencies: ${d.missing_required.join(", ")}\nProcessing is disabled until diagnostics pass.`
        : `MAT2 runtime unavailable\n${d.error ?? "unknown reason"}\nProcessing is disabled until diagnostics pass.`;
      showInspectionMessage("error", reason);
      logInfo(`MAT2 runtime problem: ${d.error ?? `missing required: ${d.missing_required.join(", ")}`}`);
    } else {
      if (statusEl) {
        statusEl.textContent = "Ready";
        statusEl.className = "ok";
      }
      if (versionEl && d.version) {
        versionEl.textContent = `${d.version} · App ${d.app_version}`;
      }
      logInfo(`MAT2 runtime ready: ${d.version ?? "version unknown"}`);
      if (d.missing_optional.length > 0) {
        logInfo(
          `Optional components unavailable (affected formats will report Unsupported): ${d.missing_optional.join(", ")}`,
        );
      }
    }
  } catch (err) {
    state.runtimeFatal = true;
    if (statusEl) {
      statusEl.textContent = "Problem";
      statusEl.className = "problem";
    }
    logError("startup diagnostics", err);
  }
  updateProcessBar();
}

function showInspectionMessage(kind: "ok" | "warning" | "error", text: string): void {
  const box = mustEl("inspection-message");
  box.replaceChildren();
  box.className = `message ${kind}`;
  box.textContent = text;
  box.classList.remove("hidden");
}

function hideInspectionParts(): void {
  mustEl("inspection-placeholder").classList.add("hidden");
  mustEl("inspection-message").classList.add("hidden");
  mustEl("meta-table").classList.add("hidden");
  mustEl("diff-table").classList.add("hidden");
  mustEl("diff-summary").classList.add("hidden");
  mustEl("epistemic-note").classList.add("hidden");
}

function renderInspectionHeader(file: PublicSelectedFile, statusText: string | null): void {
  const header = mustEl("inspection-header");
  const nameEl = mustEl("inspection-file-name");
  const statusEl = mustEl("inspection-file-status");
  nameEl.textContent = file.relative_path ?? file.display_name;
  if (statusText) {
    statusEl.textContent = statusText;
    statusEl.className = `file-status ${statusClass(file.status)}`.trim();
    statusEl.classList.remove("hidden");
  } else {
    statusEl.textContent = "";
    statusEl.classList.add("hidden");
  }
  header.classList.remove("hidden");
}

function renderMetaTable(entries: InspectionDto["entries"]): void {
  const table = mustEl("meta-table");
  const body = mustEl("meta-body");
  body.replaceChildren();
  for (const e of entries) {
    const tr = document.createElement("tr");
    const k = document.createElement("td");
    k.textContent = e.key;
    const v = document.createElement("td");
    v.textContent = e.display_value;
    tr.append(k, v);
    body.append(tr);
  }
  table.classList.remove("hidden");
}

function normKey(key: string): string {
  const parts = key.toLowerCase().split(/[^a-z0-9]+/).filter(Boolean);
  return parts[parts.length - 1] ?? key.toLowerCase();
}

function renderDiff(result: FileJobResult): void {
  const table = mustEl("diff-table") as HTMLTableElement;
  const body = mustEl("diff-body");
  const headRow = table.tHead?.rows[0];
  const synthActive = result.synthetic_state !== "not_requested";
  body.replaceChildren();

  if (headRow) {
    headRow.replaceChildren();
    const labels = synthActive
      ? ["Field", "Original", "Cleaned", "Synthetic", "Status"]
      : ["Field", "Before", "After", "Status"];
    for (const label of labels) {
      const th = document.createElement("th");
      th.scope = "col";
      th.textContent = label;
      headRow.append(th);
    }
  }

  const synthByNorm = new Map<string, string>();
  for (const f of result.synthetic_fields) {
    synthByNorm.set(normKey(f.field), f.value);
  }
  const matchedNorms = new Set<string>();

  for (const d of result.diffs) {
    const tr = document.createElement("tr");
    const key = document.createElement("td");
    key.textContent = d.key;
    const before = document.createElement("td");
    before.textContent = d.before ?? "Not detected";
    const after = document.createElement("td");
    after.textContent = d.after ?? "Removed";
    tr.append(key, before, after);

    if (synthActive) {
      const nk = normKey(d.key);
      const cell = document.createElement("td");
      const synthValue = synthByNorm.get(nk);
      if (synthValue !== undefined) {
        matchedNorms.add(nk);
        cell.textContent = synthValue;
        cell.className = "synth-value";
      } else {
        cell.textContent = "—";
      }
      tr.append(cell);
    }

    const status = document.createElement("td");
    status.textContent = d.status;
    status.className = `status-${d.status.toLowerCase()}`;
    tr.append(status);
    body.append(tr);
  }

  if (synthActive) {
    for (const f of result.synthetic_fields) {
      const nk = normKey(f.field);
      if (matchedNorms.has(nk)) continue;
      const tr = document.createElement("tr");
      const key = document.createElement("td");
      key.textContent = f.field;
      const before = document.createElement("td");
      before.textContent = "—";
      const after = document.createElement("td");
      after.textContent = "—";
      const synthCell = document.createElement("td");
      synthCell.textContent = f.value;
      synthCell.className = "synth-value";
      const status = document.createElement("td");
      status.textContent = "Synthetic";
      status.className = "status-synthetic";
      tr.append(key, before, after, synthCell, status);
      body.append(tr);
    }
  }
  table.classList.remove("hidden");

  const summary = mustEl("diff-summary");
  summary.classList.remove("hidden");
  const lines: string[] = [];
  if (result.summary) {
    lines.push(`Detected before: ${result.summary.detected_before}`);
    lines.push(`Removed: ${result.summary.removed}`);
    lines.push(`Changed: ${result.summary.changed}`);
    lines.push(
      result.summary.still_detectable === 0
        ? "No metadata detectable by MAT2"
        : `Still detectable by MAT2: ${result.summary.still_detectable}`,
    );
  } else {
    lines.push(result.detail);
  }
  switch (result.synthetic_state) {
    case "applied_verified":
      lines.push("");
      lines.push("Original identifying metadata removed");
      lines.push("Synthetic metadata added and verified");
      break;
    case "failed_kept_clean":
      lines.push("");
      lines.push("MAT2 cleaning succeeded.");
      lines.push("Synthetic metadata could not be applied.");
      lines.push("Clean output is available.");
      break;
    case "unavailable_format":
      lines.push("");
      lines.push("Synthetic mode unavailable for this format — cleaned normally.");
      break;
    case "not_requested":
      break;
  }
  switch (result.tripwire_state) {
    case "planted_verified":
      lines.push("Investigation Tripwire ✓ Planted");
      break;
    case "failed_creation":
    case "failed_kept_synthetic":
      lines.push("Investigation Tripwire ! Could not be created");
      break;
    case "unavailable_format":
      lines.push("Investigation Tripwire unavailable for this format");
      break;
    case "not_requested":
      break;
  }
  summary.textContent = lines.join("\n");
  if (result.summary?.still_detectable === 0 || result.synthetic_state === "applied_verified") {
    mustEl("epistemic-note").classList.remove("hidden");
  }
}

async function renderInspection(): Promise<void> {
  hideInspectionParts();
  const id = state.selectedId;
  if (!id) {
    mustEl("inspection-header").classList.add("hidden");
    mustEl("inspection-placeholder").classList.remove("hidden");
    return;
  }
  const file = state.files.find((f) => f.id === id);
  if (!file) {
    mustEl("inspection-header").classList.add("hidden");
    mustEl("inspection-placeholder").classList.remove("hidden");
    return;
  }

  const result = state.results.get(id);
  if (result) {
    renderInspectionHeader(file, result.status);
    if (result.diffs.length > 0 || result.summary) {
      renderDiff(result);
    } else {
      showInspectionMessage(
        result.status === "Processed" ? "ok" : result.status === "Warning" ? "warning" : "error",
        result.detail,
      );
      if (result.status === "Processed") {
        mustEl("epistemic-note").classList.remove("hidden");
      }
    }
    return;
  }

  renderInspectionHeader(file, null);
  const cached = state.inspections.get(id);
  if (cached) {
    renderInspectionData(cached);
    return;
  }
  if (state.inspectingId === id) {
    showInspectionMessage("ok", "Inspecting…");
    return;
  }

  state.inspectingId = id;
  showInspectionMessage("ok", "Inspecting…");
  try {
    const data = await ipc.inspectSelection(id);
    state.inspections.set(id, data);
    if (state.selectedId === id && !state.results.has(id)) {
      hideInspectionParts();
      renderInspectionHeader(file, null);
      renderInspectionData(data);
    }
  } catch (err) {
    if (state.selectedId === id) {
      hideInspectionParts();
      renderInspectionHeader(file, null);
      showInspectionMessage("error", `Inspection failed: ${String(err)}`);
    }
    logError("inspection", err);
  } finally {
    if (state.inspectingId === id) state.inspectingId = null;
  }
}

function renderInspectionData(data: InspectionDto): void {
  if (data.error) {
    showInspectionMessage("error", data.error);
    return;
  }
  if (!data.supported) {
    showInspectionMessage(
      "warning",
      `MAT2 does not support this format${data.mimetype ? ` (${data.mimetype})` : ""}. It will not be processed.`,
    );
    return;
  }
  if (data.entries.length === 0) {
    showInspectionMessage("ok", "No metadata detectable by MAT2");
    mustEl("epistemic-note").classList.remove("hidden");
    return;
  }
  renderMetaTable(data.entries);
}

async function runSyntheticPreview(): Promise<void> {
  const out = mustEl("synthetic-preview");
  const extLabel = mustEl("synthetic-preview-ext");
  const opts = readSyntheticOptions();
  if (!opts) return;
  const sel = state.selectedId ? state.files.find((f) => f.id === state.selectedId) : undefined;
  const source = sel ?? state.files.find((f) => state.checked.has(f.id));
  if (!source) {
    extLabel.textContent = "Select or check a file first.";
    out.classList.add("hidden");
    return;
  }
  const ext = (source.extension ?? "").toLowerCase();
  if (!ext) {
    extLabel.textContent = `${source.display_name}: no extension — preview needs a known format.`;
    out.classList.add("hidden");
    return;
  }
  extLabel.textContent = `for .${ext} (throwaway sample — the job generates fresh profiles)`;
  out.textContent = "Generating preview…";
  out.classList.remove("hidden");
  try {
    const p = await ipc.syntheticPreview(ext, { ...opts, enabled: true });
    out.textContent = [
      `Archetype      ${p.archetype}`,
      `Device         ${p.device ?? "—"}`,
      `Software       ${p.software ?? "omitted by sparsity rule"}`,
      `Created        ${p.created} (${p.utc_offset})`,
      `Author         ${p.author ?? "—"}`,
      `Location       ${p.location ?? "disabled"}`,
      `GPS            ${p.gps ?? "disabled"}`,
      `Serial         ${p.serial ?? "off"}`,
      `Timezone       ${p.timezone}`,
    ].join("\n");
  } catch (err) {
    out.textContent = `Preview failed: ${String(err)}`;
    logError("synthetic preview", err);
  }
}

function updateOutputCards(): void {
  for (const card of document.querySelectorAll<HTMLElement>(".output-card")) {
    const radio = card.querySelector<HTMLInputElement>('input[name="output"]');
    card.classList.toggle("selected", radio?.checked === true);
  }
}

let advancedReturnFocus: HTMLElement | null = null;

function openAdvanced(): void {
  advancedReturnFocus = document.activeElement as HTMLElement | null;
  mustEl("advanced-overlay").classList.remove("hidden");
  (el("advanced-close") as HTMLButtonElement | null)?.focus();
}

function closeAdvanced(): void {
  mustEl("advanced-overlay").classList.add("hidden");
  if (advancedReturnFocus?.isConnected) advancedReturnFocus.focus();
  advancedReturnFocus = null;
}

function updateOutputLock(): void {
  const inplaceOn = inputEl("opt-inplace")?.checked === true;
  const fieldset = mustEl("output-mode") as HTMLFieldSetElement;
  fieldset.disabled = inplaceOn;
  if (inplaceOn) {
    mustEl("custom-output-row").classList.add("hidden");
  }
}

function resetInplaceUi(): void {
  const inplaceBox = inputEl("opt-inplace");
  if (inplaceBox && inplaceBox.checked) {
    inplaceBox.checked = false;
    mustEl("inplace-warning").classList.add("hidden");
    logInfo("In-place mode reset to OFF (arm consumed by the job; off on every launch).");
  }
  updateOutputLock();
}

function readSyntheticOptions(): SyntheticOptions | null {
  const enabled = inputEl("opt-synthetic")?.checked === true;
  const radio = (name: string): string | null =>
    document.querySelector<HTMLInputElement>(`input[name="${name}"]:checked`)?.value ?? null;
  const scope = radio("synth-scope") ?? "per_file";
  const identity = radio("synth-identity") ?? "alias";
  const location = radio("synth-location") ?? "off";
  const technical = radio("synth-technical") ?? "synthetic";
  const serial = radio("synth-serial") ?? "empty";
  if (scope !== "per_file" && scope !== "batch") return null;
  if (identity !== "alias" && identity !== "empty") return null;
  if (location !== "off" && location !== "city" && location !== "gps") return null;
  if (technical !== "synthetic" && technical !== "empty") return null;
  if (serial !== "empty" && serial !== "generate") return null;

  let tripwire: TripwireOptions | null = null;
  if (enabled && inputEl("opt-tripwire")?.checked === true) {
    if (!tripwireConfirmed) {
      showInspectionMessage(
        "error",
        "Confirm the Investigation Tripwire panel (Enable Tripwire) before processing.",
      );
      return null;
    }
    const emailEl = inputEl("tripwire-email") as HTMLInputElement | null;
    const redirectEl = inputEl("tripwire-redirect") as HTMLInputElement | null;
    const email = (emailEl?.value ?? "").trim();
    const redirectUrl = (redirectEl?.value ?? "").trim() || "https://archive.org/";
    if (!EMAIL_SHAPE.test(email) || email.length > 254) {
      showInspectionMessage("error", "Investigation Tripwire requires a valid alert email.");
      return null;
    }
    if (!redirectUrl.startsWith("https://")) {
      showInspectionMessage("error", "Redirect destination must be an https:// URL.");
      return null;
    }
    tripwire = { enabled: true, email, redirectUrl };
  }

  return {
    enabled,
    profileScope: scope,
    identityMode: identity,
    locationMode: location,
    technicalMode: technical,
    serialMode: serial,
    tripwire,
  };
}

function readSettings(): JobSettingsDto | null {
  const mode = document.querySelector<HTMLInputElement>('input[name="mode"]:checked');
  const output = document.querySelector<HTMLInputElement>('input[name="output"]:checked');
  const verbose = inputEl("opt-verbose");
  const unknown = el("opt-unknown-members") as HTMLSelectElement | null;
  const inplace = inputEl("opt-inplace");
  if (!mode || !output || !verbose || !unknown || !inplace) return null;
  const unknownMembers = unknown.value;
  if (unknownMembers !== "abort" && unknownMembers !== "omit" && unknownMembers !== "keep") {
    return null;
  }
  if (!inplace.checked && output.value === "custom" && !state.customRootName) {
    showInspectionMessage("error", "Choose a custom output folder first.");
    logInfo("Custom output mode selected but no folder chosen yet.");
    return null;
  }
  const synthetic = readSyntheticOptions();
  if (inplace.checked && synthetic?.enabled) {
    showInspectionMessage(
      "error",
      "Synthetic metadata cannot be combined with in-place mode. Decoys are never written to originals.",
    );
    return null;
  }
  return {
    lightweight: mode.value === "lightweight",
    verbose: verbose.checked,
    unknownMembers,
    output: output.value === "custom" ? "custom" : "beside",
    inplace: inplace.checked,
    synthetic,
  };
}

async function refresh(): Promise<void> {
  try {
    const files = await ipc.listSelection();
    const ids = new Set(files.map((f) => f.id));
    for (const id of [...state.checked]) {
      if (!ids.has(id)) state.checked.delete(id);
    }
    for (const id of [...state.results.keys()]) {
      if (!ids.has(id)) state.results.delete(id);
    }
    for (const id of [...state.inspections.keys()]) {
      if (!ids.has(id)) state.inspections.delete(id);
    }
    if (state.selectedId && !ids.has(state.selectedId)) {
      state.selectedId = null;
    }
    for (const f of files) {
      if (!state.checked.has(f.id) && !state.files.some((old) => old.id === f.id)) {
        state.checked.add(f.id);
      }
    }
    state.files = files;
    renderFilters();
    renderFiles();
    void renderInspection();
  } catch (err) {
    logError("refresh", err);
  }
}

async function runInspectOnly(): Promise<void> {
  const ids = state.files.filter((f) => state.checked.has(f.id)).map((f) => f.id);
  if (ids.length === 0) {
    logInfo("Inspect only: no files checked.");
    return;
  }
  for (const id of ids) {
    const file = state.files.find((f) => f.id === id);
    const label = file ? file.display_name : id;
    try {
      const data = await ipc.inspectSelection(id);
      state.inspections.set(id, data);
      if (data.error) {
        logInfo(`${label}: inspection error — ${data.error}`);
      } else if (!data.supported) {
        logInfo(`${label}: format not supported by MAT2${data.mimetype ? ` (${data.mimetype})` : ""}`);
      } else {
        logInfo(`${label}: ${data.entries.length} metadata fields detectable by MAT2`);
      }
    } catch (err) {
      logError(`inspect ${label}`, err);
    }
  }
  if (ids.length === 1 && ids[0]) {
    selectRow(ids[0]);
  }
}

function bindDiagnostics(buttonId: string, title: string, cmd: () => Promise<string>): void {
  mustEl(buttonId).addEventListener("click", async () => {
    appendLog(`[${localStamp()}] --- ${title} ---`);
    try {
      const out = await cmd();
      for (const line of out.split("\n")) {
        appendLog(line);
      }
    } catch (err) {
      logError(title, err);
    }
  });
}

function bind(id: string, handler: () => void): void {
  el(id)?.addEventListener("click", handler);
}

async function startJob(): Promise<void> {
  const ids = state.files.filter((f) => state.checked.has(f.id)).map((f) => f.id);
  if (ids.length === 0 || state.jobRunning) return;
  if (state.runtimeFatal) {
    logInfo("Processing is disabled: MAT2 runtime diagnostics failed.");
    return;
  }
  const settings = readSettings();
  if (!settings) return;
  if (settings.inplace) {
    const confirmed = await confirmDialog(
      `Confirm destructive processing of ${ids.length} original files`,
      "This changes the selected originals. No wrapper-created backup is guaranteed.",
      "Replace originals",
    );
    if (!confirmed) {
      logInfo("Destructive job aborted at confirmation.");
      return;
    }
  }
  setJobRunning(true);
  state.hasCommittedOutputs = false;
  updateRevealButton();
  logInfo(`Job started: ${ids.length} file(s), mode ${settings.lightweight ? "lightweight" : "maximum removal"}, ${settings.inplace ? "IN PLACE (destructive)" : `output ${settings.output}`}`);
  try {
    await ipc.startCleanJob(ids, settings);
  } catch (err) {
    logError("start job", err);
    setJobRunning(false);
  }
}

function updateRevealButton(): void {
  const btn = mustEl("btn-reveal");
  btn.classList.toggle("hidden", !(state.lastJobId && state.hasCommittedOutputs));
}

function boot(): void {
  void runStartupDiagnostics();

  bind("btn-add-files", () => ipc.selectFiles().catch((e) => logError("select files", e)));
  bind("btn-add-folder", () => ipc.selectFolder().catch((e) => logError("select folder", e)));
  bind("btn-remove-selected", () => {
    void ipc.removeItems([...state.checked]).catch((e) => logError("remove", e));
  });
  bind("btn-process", () => void startJob());
  bind("btn-cancel", () => {
    ipc.cancelJob().catch((e) => logError("cancel", e));
    logInfo("Cancellation requested…");
  });
  bind("btn-inspect-only", () => void runInspectOnly());
  bind("btn-choose-output", () => ipc.chooseOutputRoot().catch((e) => logError("choose output", e)));

  bind("btn-group-check", () => {
    for (const f of visibleFiles()) state.checked.add(f.id);
    renderFiles();
  });
  bind("btn-group-uncheck", () => {
    for (const f of visibleFiles()) state.checked.delete(f.id);
    renderFiles();
  });

  bind("btn-reveal", () => {
    const jobId = state.lastJobId;
    if (!jobId) return;
    ipc
      .revealOutput(jobId)
      .then((n) => logInfo(`Revealed ${n} output folder(s) in Finder.`))
      .catch((err) => logError("reveal output", err));
  });
  bind("link-mat2", () => ipc.openMat2Site().catch((e) => logError("open MAT2 site", e)));
  bind("link-dangerzone", () =>
    ipc.openDangerzoneSite().catch((e) => logError("open Dangerzone site", e)),
  );
  bind("link-privacytools", () =>
    ipc.openPrivacytoolsSite().catch((e) => logError("open PrivacyTools site", e)),
  );

  bindDiagnostics("diag-version", "MAT2 version", ipc.mat2Version);
  bindDiagnostics("diag-formats", "Supported formats", ipc.mat2Formats);
  bindDiagnostics("diag-deps", "Dependency check", ipc.mat2CheckDependencies);
  bindDiagnostics("diag-help", "MAT2 help", ipc.mat2Help);

  const unknownSelect = el("opt-unknown-members") as HTMLSelectElement | null;
  unknownSelect?.addEventListener("change", () => {
    mustEl("unknown-members-keep-warning").classList.toggle("hidden", unknownSelect.value !== "keep");
  });

  const inplaceBox = inputEl("opt-inplace");
  inplaceBox?.addEventListener("change", () => {
    void (async () => {
      if (inplaceBox.checked) {
        const ok = await confirmDialog(
          "Replace original files?",
          "Normal mode preserves the original and produces a cleaned copy. In-place mode changes the selected original files.",
          "Enable in-place",
        );
        if (!ok) {
          inplaceBox.checked = false;
          return;
        }
        try {
          await ipc.setInplaceArmed(true);
        } catch (err) {
          logError("arm in-place", err);
          inplaceBox.checked = false;
          return;
        }
        mustEl("inplace-warning").classList.remove("hidden");
        logInfo("Destructive in-place mode ARMED for the next job only.");
      } else {
        try {
          await ipc.setInplaceArmed(false);
        } catch (err) {
          logError("disarm in-place", err);
        }
        mustEl("inplace-warning").classList.add("hidden");
        logInfo("Destructive in-place mode disabled.");
      }
      const sb = inputEl("opt-synthetic");
      if (sb) {
        if (inplaceBox.checked) {
          sb.checked = false;
          mustEl("synthetic-options").classList.add("hidden");
        }
        sb.disabled = inplaceBox.checked;
      }
      updateOutputLock();
    })();
  });

  bind("confirm-ok", () => closeConfirm(true));
  bind("confirm-cancel", () => closeConfirm(false));
  mustEl("confirm-overlay").addEventListener("click", (e) => {
    if (e.target === mustEl("confirm-overlay")) closeConfirm(false);
  });
  bind("btn-advanced", openAdvanced);
  bind("advanced-close", closeAdvanced);
  mustEl("advanced-overlay").addEventListener("click", (e) => {
    if (e.target === mustEl("advanced-overlay")) closeAdvanced();
  });
  document.addEventListener("keydown", (e) => {
    if (e.key !== "Escape") return;
    if (!mustEl("confirm-overlay").classList.contains("hidden")) {
      closeConfirm(false);
      return;
    }
    if (!mustEl("tripwire-info-overlay").classList.contains("hidden")) {
      mustEl("tripwire-info-overlay").classList.add("hidden");
      return;
    }
    if (!mustEl("advanced-overlay").classList.contains("hidden")) {
      closeAdvanced();
    }
  });

  const synthBox = inputEl("opt-synthetic");
  synthBox?.addEventListener("change", () => {
    mustEl("synthetic-options").classList.toggle("hidden", !synthBox.checked);
    if (synthBox.checked && inplaceBox && inplaceBox.checked) {
      synthBox.checked = false;
      mustEl("synthetic-options").classList.add("hidden");
      showInspectionMessage(
        "error",
        "Disable in-place mode first — synthetic metadata is never written to originals.",
      );
      return;
    }
    if (!synthBox.checked) {
      const tw = inputEl("opt-tripwire");
      if (tw) tw.checked = false;
      tripwireConfirmed = false;
      mustEl("tripwire-panel").classList.add("hidden");
    }
    if (inplaceBox) inplaceBox.disabled = synthBox.checked;
    logInfo(synthBox.checked ? "Synthetic metadata mode enabled (decoys, cleaned copies only)." : "Synthetic metadata mode disabled.");
  });

  const tripwireBox = inputEl("opt-tripwire");
  tripwireBox?.addEventListener("change", () => {
    mustEl("tripwire-panel").classList.toggle("hidden", !tripwireBox.checked);
    if (!tripwireBox.checked) tripwireConfirmed = false;
  });
  bind("btn-tripwire-cancel", () => {
    if (tripwireBox) tripwireBox.checked = false;
    tripwireConfirmed = false;
    mustEl("tripwire-panel").classList.add("hidden");
  });
  bind("btn-tripwire-enable", () => {
    const emailEl = inputEl("tripwire-email") as HTMLInputElement | null;
    const redirectEl = inputEl("tripwire-redirect") as HTMLInputElement | null;
    const email = (emailEl?.value ?? "").trim();
    const redirect = (redirectEl?.value ?? "").trim() || "https://archive.org/";
    if (!EMAIL_SHAPE.test(email) || email.length > 254) {
      showInspectionMessage("error", "Enter a valid alert email to enable the tripwire.");
      return;
    }
    if (!redirect.startsWith("https://")) {
      showInspectionMessage("error", "Redirect destination must be an https:// URL.");
      return;
    }
    const finish = () => {
      tripwireConfirmed = true;
      logInfo("Investigation Tripwire enabled (one token per output file; alerts go to the service, never into files).");
    };
    if (!tripwireDisclosedThisSession) {
      void confirmDialog(
        "Detection Canary uses Canarytokens.org",
        "To create the tripwire, this application will contact Canarytokens.org and send:\n\n• the alert email you provide\n• a random non-sensitive reference\n• the redirect destination\n\nYour file, filename, original metadata and file path are never sent.\n\nLike any direct web connection, Canarytokens.org can observe the network connection used to create the token.",
        "Continue",
      ).then((ok) => {
        if (ok) {
          tripwireDisclosedThisSession = true;
          finish();
        }
      });
    } else {
      finish();
    }
  });
  bind("btn-tripwire-info", () => mustEl("tripwire-info-overlay").classList.remove("hidden"));
  bind("tw-info-close", () => mustEl("tripwire-info-overlay").classList.add("hidden"));
  mustEl("tripwire-info-overlay").addEventListener("click", (e) => {
    if (e.target === mustEl("tripwire-info-overlay")) {
      mustEl("tripwire-info-overlay").classList.add("hidden");
    }
  });
  bind("link-canarytokens", () => ipc.openCanarytokensSite().catch((e) => logError("open Canarytokens", e)));
  bind("link-canary-docs", () => ipc.openCanaryDocs().catch((e) => logError("open Fast Redirect docs", e)));
  bind("link-canary-repo", () => ipc.openCanaryRepo().catch((e) => logError("open canarytokens repo", e)));
  bind("link-canary-audit", () => ipc.openCanaryAudit().catch((e) => logError("open security audit", e)));

  for (const radio of document.querySelectorAll<HTMLInputElement>('input[name="synth-location"]')) {
    radio.addEventListener("change", () => {
      const gps = document.querySelector<HTMLInputElement>('input[name="synth-location"]:checked')?.value === "gps";
      mustEl("synthetic-gps-warning").classList.toggle("hidden", !gps);
    });
  }
  bind("btn-synth-preview", () => void runSyntheticPreview());

  for (const radio of document.querySelectorAll<HTMLInputElement>('input[name="output"]')) {
    radio.addEventListener("change", () => {
      const custom = document.querySelector<HTMLInputElement>('input[name="output"]:checked')?.value === "custom";
      mustEl("custom-output-row").classList.toggle("hidden", !custom);
      updateOutputCards();
    });
  }
  updateOutputCards();

  const dropZone = mustEl("drop-zone");
  dropZone.addEventListener("click", () => ipc.selectFiles().catch((e) => logError("select files", e)));
  dropZone.addEventListener("keydown", (e) => {
    if (e.key === "Enter" || e.key === " ") {
      e.preventDefault();
      void ipc.selectFiles().catch((err) => logError("select files", err));
    }
  });

  void ipc.onSelectionChanged(() => void refresh());
  void ipc.onJobStatus((e) => {
    const file = state.files.find((f) => f.id === e.id);
    if (file) file.status = e.status;
    renderFiles();
  });
  void ipc.onJobLog((line) => appendLog(line));
  void ipc.onJobFileResult((r) => {
    state.results.set(r.id, r);
    if (r.committed) state.hasCommittedOutputs = true;
    const file = state.files.find((f) => f.id === r.id);
    if (file) file.status = r.status;
    renderFiles();
    if (state.selectedId === r.id) void renderInspection();
  });
  void ipc.onJobFinished((e) => {
    setJobRunning(false);
    resetInplaceUi();
    state.lastJobId = e.jobId;
    updateRevealButton();
    logInfo(e.cancelled ? "Job cancelled." : "Job finished.");
    void refresh();
  });
  void ipc.onOutputRootChanged((name) => {
    state.customRootName = name;
    const label = el("custom-output-path");
    if (label) label.textContent = name;
    logInfo(`Custom output folder: ${name}`);
  });
  void ipc.onDragEnter(() => dropZone.classList.add("dragover"));
  void ipc.onDragLeave(() => dropZone.classList.remove("dragover"));

  void (async () => {
    try {
      const root = await ipc.outputRootInfo();
      if (root) {
        state.customRootName = root;
        const label = el("custom-output-path");
        if (label) label.textContent = root;
      }
    } catch (err) {
      logError("output root info", err);
    }
  })();

  logInfo("metadata'nt UI ready. Files stay on this device.");
  void refresh();
}

document.addEventListener("DOMContentLoaded", boot);
