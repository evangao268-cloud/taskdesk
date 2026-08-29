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
}

export interface Settings {
  dismissMode: DismissMode;
  pauseSeconds: number;
  autostartEnabled: boolean;
  syncIntervalSecs: number;
  showUndated: boolean;
}

export interface DismissState {
  mode: DismissMode;
  blockedForMs: number;
  engaged: boolean;
  allowed: boolean;
}

export interface BootView {
  today: TaskDto[];
  overdue: TaskDto[];
  nudges: DueNudgeDto[];
  undated: TaskDto[];
  settings: Settings;
  dismiss: DismissState;
}

export const getBootView = () => invoke<BootView>("get_boot_view");
export const addTask = (title: string, dueDate?: string) =>
  invoke<TaskDto>("add_task", { title, dueDate: dueDate ?? null, notes: null });
export const completeTask = (localId: string) =>
  invoke<TaskDto>("complete_task", { localId });
export const ackNudge = (nudgeId: string) => invoke<void>("ack_nudge", { nudgeId });
export const nothingToday = () => invoke<void>("nothing_today");
export const dismissWindow = () => invoke<DismissState>("dismiss_window");
export const getDismissState = () => invoke<DismissState>("get_dismiss_state");
export const getSettings = () => invoke<Settings>("get_settings");
export const updateSettings = (settings: Settings) =>
  invoke<Settings>("update_settings", { settings });
