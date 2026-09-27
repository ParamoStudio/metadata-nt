/**
 * Frontend state container + IPC access.
 *
 * Holds only data the backend chose to expose (opaque IDs + display fields).
 * Absolute source paths never cross the IPC boundary and are never stored here.
 */

import { invoke } from "@tauri-apps/api/core";
import type { PublicSelectedFile } from "./types";

export interface AppState {
  files: PublicSelectedFile[];
}

export function initialState(): AppState {
  return { files: [] };
}

export async function refreshSelection(state: AppState): Promise<void> {
  state.files = await invoke<PublicSelectedFile[]>("list_selection");
}

export async function requestSelectFiles(): Promise<void> {
  await invoke("select_files");
}

export async function requestSelectFolder(): Promise<void> {
  await invoke("select_folder");
}

export async function requestRemove(ids: string[]): Promise<void> {
  if (ids.length === 0) return;
  await invoke("remove_items", { ids });
}
