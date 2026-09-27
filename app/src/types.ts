/**
 * Shared frontend types — mirror of the Rust public IPC model.
 * Keep in sync with app/src-tauri/src/model.rs.
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

/** One MAT2-detected metadata field (nested keys flattened with " / "). */
export interface MetadataEntry {
  key: string;
  display_value: string;
}

/** Before/after statuses, per INTERFACE.md §15 (no risk vocabulary). */
export type DiffStatus = "Removed" | "Changed" | "Remaining";

export interface MetadataDiff {
  key: string;
  before: string | null;
  after: string | null;
  status: DiffStatus;
}

export interface DiffSummary {
  detected_before: number;
  removed: number;
  changed: number;
  still_detectable: number;
}
