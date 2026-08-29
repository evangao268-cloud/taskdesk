import { invoke } from "@tauri-apps/api/core";

export type TaskStatus = "needsAction" | "completed";
export type DismissMode = "instant" | "pause" | "engage";

export interface TaskDto {
  localId: string;
  title: string;
  notes: string | null;
  dueDate: string | null;
  status: TaskStatus;
}

export interface DueNudgeDto {
  id: string;
  title: string;
  intervalDays: number;
  daysOverdue: number;
  createTaskOnAck: boolean;
}

export interface NudgeDef {
  id: string;
  title: string;
  intervalDays: number;
  anchorDate: string;
  createTaskOnAck: boolean;
  enabled: boolean;
}

export interface Settings {
  dismissMode: DismissMode;
  pauseSeconds: number;
  autostartEnabled: boolean;
  syncIntervalSecs: number;
  showUndated: boolean;
  googleAccountEmail: string | null;
  defaultTasklistId: string | null;
}

export interface DismissState {
  mode: DismissMode;
  blockedForMs: number;
  engaged: boolean;
  allowed: boolean;
}

export type SyncState = "idle" | "syncing" | "offline" | "auth_error";

export interface SyncSummary {
  state: SyncState;
  pendingOutbox: number;
  lastSyncAt: string | null;
  connected: boolean;
  email: string | null;
}

export interface AuthStatus {
  connected: boolean;
  email: string | null;
  configPresent: boolean;
}

export interface SyncReport {
  pulled: number;
  pushed: number;
  deferred: number;
  state: SyncState;
}

export interface BootView {
  today: TaskDto[];
  overdue: TaskDto[];
  nudges: DueNudgeDto[];
  undated: TaskDto[];
  settings: Settings;
  dismiss: DismissState;
  sync: SyncSummary;
}

export const getBootView = () => invoke<BootView>("get_boot_view");
export const addTask = (title: string, dueDate?: string) =>
  invoke<TaskDto>("add_task", { title, dueDate: dueDate ?? null, notes: null });
export const completeTask = (localId: string) =>
  invoke<TaskDto>("complete_task", { localId });
export const ackNudge = (nudgeId: string, createTask: boolean) =>
  invoke<void>("ack_nudge", { nudgeId, createTask });
export const listNudges = () => invoke<NudgeDef[]>("list_nudges");
export const addNudge = (title: string, intervalDays: number, createTaskOnAck: boolean) =>
  invoke<NudgeDef>("add_nudge", { title, intervalDays, createTaskOnAck });
export const deleteNudge = (id: string) => invoke<void>("delete_nudge", { id });
export const nothingToday = () => invoke<void>("nothing_today");
export const dismissWindow = () => invoke<DismissState>("dismiss_window");
export const getDismissState = () => invoke<DismissState>("get_dismiss_state");
export const getSettings = () => invoke<Settings>("get_settings");
export const updateSettings = (settings: Settings) =>
  invoke<Settings>("update_settings", { settings });
export const getAuthStatus = () => invoke<AuthStatus>("get_auth_status");
export const startGoogleAuth = () => invoke<AuthStatus>("start_google_auth");
export const disconnectGoogle = () => invoke<void>("disconnect_google");
export const syncNow = () => invoke<SyncReport>("sync_now");
