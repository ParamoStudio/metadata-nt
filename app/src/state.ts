/**
 * Frontend state container (Task 2: placeholder).
 *
 * Later tasks own this module:
 * - Task 4:  selection list keyed by opaque IDs from the Rust registry
 * - Task 10: per-file job state mirroring backend events
 * - Task 12: view binding
 *
 * Rule: this module holds application state only — never document content,
 * never absolute source paths (the backend does not send them).
 */

import type { PublicSelectedFile } from "./types";

export interface AppState {
  files: PublicSelectedFile[];
}

export function initialState(): AppState {
  return { files: [] };
}
