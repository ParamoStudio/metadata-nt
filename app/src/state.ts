/**
 * IPC data layer — every backend call the UI may make, typed.
 *
 * The surface is deliberately semantic (HANDOFF §11): no generic exec/shell/
 * read/write/open commands exist; paths never cross the boundary (opaque IDs
 * in, display data out).
 */

import { invoke } from "@tauri-apps/api/core";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import type {
  DiagnosticsDto,
  FileJobResult,
  InspectionDto,
  JobFinishedEvent,
  JobLogEvent,
  JobSettingsDto,
  JobStatusEvent,
  OutputRootChangedEvent,
  PublicSelectedFile,
} from "./types";

export function runtimeDiagnostics(): Promise<DiagnosticsDto> {
  return invoke<DiagnosticsDto>("runtime_diagnostics");
}

export function listSelection(): Promise<PublicSelectedFile[]> {
  return invoke<PublicSelectedFile[]>("list_selection");
}

export function selectFiles(): Promise<void> {
  return invoke("select_files");
}

export function selectFolder(): Promise<void> {
  return invoke("select_folder");
}

export function removeItems(ids: string[]): Promise<number> {
  return invoke<number>("remove_items", { ids });
}

export function chooseOutputRoot(): Promise<void> {
  return invoke("choose_output_root");
}

export function outputRootInfo(): Promise<string | null> {
  return invoke<string | null>("output_root_info");
}

export function startCleanJob(ids: string[], settings: JobSettingsDto): Promise<string> {
  return invoke<string>("start_clean_job", { ids, settings });
}

export function cancelJob(): Promise<boolean> {
  return invoke<boolean>("cancel_job");
}

export function setInplaceArmed(armed: boolean): Promise<void> {
  return invoke("set_inplace_armed", { armed });
}

export function openMat2Site(): Promise<void> {
  return invoke("open_mat2_site");
}

export function openDangerzoneSite(): Promise<void> {
  return invoke("open_dangerzone_site");
}

export function openPrivacytoolsSite(): Promise<void> {
  return invoke("open_privacytools_site");
}

export function revealOutput(jobId: string): Promise<number> {
  return invoke<number>("reveal_output", { jobId });
}

export function inspectSelection(id: string): Promise<InspectionDto> {
  return invoke<InspectionDto>("inspect_selection", { id });
}

export function mat2Version(): Promise<string> {
  return invoke<string>("mat2_version");
}

export function mat2Formats(): Promise<string> {
  return invoke<string>("mat2_formats");
}

export function mat2CheckDependencies(): Promise<string> {
  return invoke<string>("mat2_check_dependencies");
}

export function mat2Help(): Promise<string> {
  return invoke<string>("mat2_help");
}

export function onSelectionChanged(cb: () => void): Promise<UnlistenFn> {
  return listen("selection-changed", cb);
}

export function onJobStatus(cb: (e: JobStatusEvent) => void): Promise<UnlistenFn> {
  return listen<JobStatusEvent>("job-status", (ev) => cb(ev.payload));
}

export function onJobLog(cb: (line: string) => void): Promise<UnlistenFn> {
  return listen<JobLogEvent>("job-log", (ev) => cb(ev.payload.line));
}

export function onJobFileResult(cb: (r: FileJobResult) => void): Promise<UnlistenFn> {
  return listen<FileJobResult>("job-file-result", (ev) => cb(ev.payload));
}

export function onJobFinished(cb: (e: JobFinishedEvent) => void): Promise<UnlistenFn> {
  return listen<JobFinishedEvent>("job-finished", (ev) => cb(ev.payload));
}

export function onOutputRootChanged(cb: (name: string) => void): Promise<UnlistenFn> {
  return listen<OutputRootChangedEvent>("output-root-changed", (ev) =>
    cb(ev.payload.displayName),
  );
}

export function onDragEnter(cb: () => void): Promise<UnlistenFn> {
  return listen("drag-enter", cb);
}

export function onDragLeave(cb: () => void): Promise<UnlistenFn> {
  return listen("drag-leave", cb);
}
