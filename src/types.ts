import { convertFileSrc } from "@tauri-apps/api/core";

export interface Space {
  id: string;
  parentId: string | null;
  name: string;
  icon: string;
  notes: string;
}
export interface Item {
  id: string;
  spaceId: string;
  name: string;
  quantity: number;
  unit: string;
  notes: string;
  /** 저장된 사진의 식별자. 새로 고른 사진은 저장할 때까지 data URL로 둔다. */
  photo: string | null;
  expiresOn: string | null;
  purchasedOn: string | null;
  purchaseStore: string;
  purchasePrice: number | null;
}
export interface Task {
  id: string;
  spaceId: string | null;
  itemId: string | null;
  title: string;
  notes: string;
  mode: "once" | "calendar" | "after_completion";
  every: number;
  unit: "day" | "week" | "month" | "year";
  anchorAt: string;
  dueAt: string;
  timezone: string;
  active: boolean;
  notifiedDueAt: string | null;
  reminderMinutes: number | null;
  notifiedReminderAt: string | null;
}
export interface CalendarEvent {
  id: string;
  taskId: string;
  title: string;
  target: string;
  scheduledFor: string;
  kind: "pending" | "projected" | "completed" | "skipped";
  recordId: string | null;
  completedAt: string | null;
}
export interface TaskRecord {
  id: string;
  taskId: string;
  title: string;
  target: string;
  scheduledFor: string;
  completedAt: string;
  outcome: "completed" | "skipped";
  taskAfter?: Task | null;
}
export interface Snapshot {
  spaces: Space[];
  items: Item[];
  tasks: Task[];
  records: TaskRecord[];
  settings: { notifications: boolean };
}
export interface AppInfo {
  version: string;
  dataPath: string;
  os: string;
  feedbackAvailable: boolean;
}
export interface AutoBackupStatus {
  lastAt: string | null;
  count: number;
  keep: number;
}
export interface ReleaseCheck {
  latestVersion: string | null;
  updateAvailable: boolean;
  notes: string | null;
}
export type Editor =
  | {
      kind: "space";
      value?: Space;
      parentId?: string;
      /** 공간이 없어 아이템·할 일 대신 공간 추가 창을 연 경우 원래 하려던 작업 */
      requiredFor?: "item" | "task";
    }
  | { kind: "item"; value?: Item; spaceId?: string }
  | {
      kind: "task";
      value?: Task;
      spaceId?: string;
      itemId?: string;
      dueAt?: string;
    };

export const reminderOptions = [
  { minutes: null, label: "없음" },
  { minutes: 10, label: "10분 전" },
  { minutes: 60, label: "1시간 전" },
  { minutes: 1440, label: "하루 전" },
  { minutes: 10080, label: "일주일 전" },
];
export function reminderLabel(minutes: number | null | undefined) {
  return (
    reminderOptions.find((option) => option.minutes === minutes)?.label ??
    "없음"
  );
}
export function reminderTime(
  dueAt: string,
  minutes: number | null | undefined,
) {
  if (!minutes) return null;
  const at = Date.parse(dueAt) - minutes * 60_000;
  return Number.isFinite(at) ? new Date(at).toISOString() : null;
}

/** 저장된 사진은 앱 전용 주소로 불러오고, 새로 고른 사진은 그대로 보여 준다. */
export function photoSrc(photo: string) {
  if (photo.startsWith("data:")) return photo;
  try {
    return convertFileSrc(photo, "zipsa-photo");
  } catch {
    return "";
  }
}

export function spacePath(spaces: Space[], id: string): string {
  const names: string[] = [],
    seen = new Set<string>();
  let current: string | null = id;
  while (current && !seen.has(current)) {
    seen.add(current);
    const space: Space | undefined = spaces.find((s) => s.id === current);
    if (!space) break;
    names.unshift(space.name);
    current = space.parentId;
  }
  return names.join(" / ");
}

export function taskTarget(data: Snapshot, task: Task): string {
  if (task.spaceId) return spacePath(data.spaces, task.spaceId);
  const item = data.items.find((i) => i.id === task.itemId);
  return item ? `${spacePath(data.spaces, item.spaceId)} / ${item.name}` : "";
}

export const units = { day: "일", week: "주", month: "개월", year: "년" };
export function scheduleLabel(task: Task) {
  if (task.mode === "once") return "한 번";
  return `${task.mode === "after_completion" ? "완료 후" : "매"} ${task.every}${units[task.unit]}${task.mode === "after_completion" ? " 뒤" : ""}`;
}
export function localDate(date = new Date()) {
  return `${date.getFullYear()}-${String(date.getMonth() + 1).padStart(2, "0")}-${String(date.getDate()).padStart(2, "0")}`;
}
export function dateTime(value: string) {
  return new Intl.DateTimeFormat("ko-KR", {
    month: "short",
    day: "numeric",
    hour: "2-digit",
    minute: "2-digit",
  }).format(new Date(value));
}
export function localInput(iso: string) {
  const d = new Date(iso);
  return `${localDate(d)}T${String(d.getHours()).padStart(2, "0")}:${String(d.getMinutes()).padStart(2, "0")}`;
}
export function expiryLabel(date: string) {
  const today = new Date(`${localDate()}T00:00:00`);
  const days = Math.round(
    (new Date(`${date}T00:00:00`).getTime() - today.getTime()) / 86400000,
  );
  return days < 0
    ? `${-days}일 지남`
    : days === 0
      ? "오늘까지"
      : `${days}일 남음`;
}
