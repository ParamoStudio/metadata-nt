/**
 * Shared frontend types — mirror of the Rust public IPC model.
 * Keep in sync with app/src-tauri/src/model.rs.
 *
 * Task 2: shell only. Types are introduced incrementally by later tasks
 * (Task 4: selection, Task 9: diff model, Task 10: job states).
 */

/** Per-file lifecycle status, per INTERFACE.md §7. */
export type FileStatus =
  | "Ready"
  | "Inspecting"
  | "Queued"
  | "Processing"
  | "Verifying"
  | "Processed"
  | "Warning"
  | "Failed"
  | "Unsupported"
  | "Cancelled";

/** Public view of a selected file (opaque id; absolute paths never leave Rust). */
export interface PublicSelectedFile {
  id: string;
  display_name: string;
  extension: string | null;
  relative_path: string | null;
  size: number;
  status: FileStatus;
}
