/**
 * Shared frontend types — mirror of the Rust public IPC model.
 * Keep in sync with app/src-tauri/src/model.rs.
 */

/** Per-file lifecycle status, per INTERFACE.md §7 (+ synthetic add-on §22). */
export type FileStatus =
  | "Ready"
  | "Inspecting"
  | "Queued"
  | "Processing"
  | "Verifying"
  | "SyntheticWriting"
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
  synthetic_state: SyntheticState;
  synthetic_fields: SyntheticField[];
  synthetic_note: string | null;
  tripwire_state: TripwireState;
  tripwire_note: string | null;
}

/** Synthetic add-on (owner-approved scope extension). */
export type SyntheticState =
  | "not_requested"
  | "applied_verified"
  | "unavailable_format"
  | "failed_kept_clean";

export interface SyntheticField {
  field: string;
  value: string;
}

/** Typed choices only — the backend constructs every writer argument. */
export interface SyntheticOptions {
  enabled: boolean;
  profileScope: "per_file" | "batch";
  identityMode: "alias" | "empty";
  locationMode: "off" | "city" | "gps";
  technicalMode: "synthetic" | "empty";
  serialMode: "empty" | "generate";
  tripwire: TripwireOptions | null;
}

/** Investigation Tripwire (owner-approved add-on; Canarytokens.org Fast
 * Redirect). The email is alert configuration for the service ONLY — the
 * backend never writes it into files, logs or results. */
export interface TripwireOptions {
  enabled: boolean;
  email: string;
  redirectUrl: string;
}

export type TripwireState =
  | "not_requested"
  | "planted_verified"
  | "failed_creation"
  | "failed_kept_synthetic"
  | "unavailable_format";

export interface SyntheticPreview {
  archetype: string;
  format: string;
  device: string | null;
  software: string | null;
  author: string | null;
  created: string;
  timezone: string;
  utc_offset: string;
  location: string | null;
  gps: string | null;
  serial: string | null;
  sparsity_present: Record<string, boolean>;
}

export interface SyntheticPackInfo {
  ok: boolean;
  sha256?: string;
  pinned?: boolean;
  warnings?: string[];
  counts?: Record<string, number>;
  error?: string;
}

/** Payload of `start_clean_job` (camelCase per serde rename_all). */
export interface JobSettingsDto {
  lightweight: boolean;
  verbose: boolean;
  unknownMembers: "abort" | "omit" | "keep";
  output: "beside" | "custom";
  inplace: boolean;
  synthetic: SyntheticOptions | null;
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

/** Result of `update_settings_get` / `update_settings_set` (Rust UpdateSettings). */
export interface UpdateSettingsDto {
  onboardingCompleted: boolean;
  automaticUpdateChecksEnabled: boolean;
  updateCheckIntervalDays: number;
  lastUpdateCheckAt: number | null;
}

/** Result of `update_check_now` / `update_auto_check_if_due` (Rust UpdateCheckResult). */
export type UpdateCheckResultDto =
  | "current"
  | "failed"
  | { updateAvailable: { version: string; tag: string; current: string } };
