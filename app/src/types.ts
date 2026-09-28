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

/** Terminal per-file job result emitted as `job-file-result`. */
export interface FileJobResult {
  id: string;
  display_name: string;
  status: FileStatus;
  detail: string;
  diffs: MetadataDiff[];
  summary: DiffSummary | null;
  committed: boolean;
}

/** Payload of `start_clean_job` (camelCase per serde rename_all). */
export interface JobSettingsDto {
  lightweight: boolean;
  verbose: boolean;
  unknownMembers: "abort" | "omit" | "keep";
  output: "beside" | "custom";
  inplace: boolean;
}

export interface JobStatusEvent {
  id: string;
  status: FileStatus;
}

export interface JobLogEvent {
  line: string;
}

export interface JobFinishedEvent {
  jobId: string;
  cancelled: boolean;
}

export interface OutputRootChangedEvent {
  displayName: string;
}

/** Result of `inspect_selection` (Rust InspectionDto). */
export interface InspectionDto {
  supported: boolean;
  mimetype: string | null;
  error: string | null;
  entries: MetadataEntry[];
}

export interface DependencyStatus {
  name: string;
  found: boolean;
  required: boolean;
}

/** Result of `runtime_diagnostics` (Rust DiagnosticsDto). */
export interface DiagnosticsDto {
  available: boolean;
  fatal: boolean;
  version: string | null;
  dependencies: DependencyStatus[];
  missing_required: string[];
  missing_optional: string[];
  error: string | null;
  app_version: string;
}
