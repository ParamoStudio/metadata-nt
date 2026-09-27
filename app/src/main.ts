/**
 * MAT2 Wrapper frontend entry point.
 *
 * Task 2: static shell only — no IPC calls, no product behavior.
 *
 * Hard rules for this codebase (HANDOFF.md §18, IMPLEMENTATION_PLAN.md Task 6/12):
 * - untrusted text (filenames, process output) is rendered exclusively through
 *   textContent / createElement — never innerHTML, never insertAdjacentHTML;
 * - no remote assets, no fetch/XHR/WebSocket — the app is offline by design;
 * - no document previews or thumbnails.
 */

function boot(): void {
  const statusValue = document.getElementById("mat2-status-value");
  const versionInfo = document.getElementById("version-info");
  if (statusValue) {
    // Task 15 replaces this with real runtime diagnostics from the backend.
    statusValue.textContent = "Not connected (shell build)";
  }
  if (versionInfo) {
    versionInfo.textContent = "";
  }
}

document.addEventListener("DOMContentLoaded", boot);
