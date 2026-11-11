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
  /**
   * 저장된 사진의 식별자 목록. 첫 번째가 목록에 보이는 대표 사진이다.
   * 새로 고른 사진은 저장할 때까지 data URL로 둔다.
   */
  photos: string[];
  attachments: Attachment[];
  expiresOn: string | null;
  purchasedOn: string | null;
  purchaseStore: string;
  purchasePrice: number | null;
  warrantyUntil: string | null;
}
/** 영수증·보증서·설명서처럼 아이템에 함께 보관하는 파일. */
export interface Attachment {
  id: string;
  name: string;
  mime: string;
  size: number;
  /** 새로 고른 파일만 저장할 때까지 data URL로 가진다. */
  data?: string;
}
export const MAX_PHOTOS = 10;
export const MAX_ATTACHMENTS = 10;
export const ATTACHMENT_MAX_BYTES = 10 * 1024 * 1024;
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

export function fileSize(bytes: number) {
  return bytes < 1024 * 1024
    ? `${Math.max(1, Math.round(bytes / 1024)).toLocaleString("ko-KR")}KB`
    : `${(bytes / 1024 / 1024).toFixed(1)}MB`;
}

export const snoozeOptions = [
  { label: "1시간", hours: 1 },
  { label: "하루", days: 1 },
  { label: "3일", days: 3 },
  { label: "일주일", days: 7 },
] as const;
/**
 * 미룬 뒤의 예정 시각. 날짜 단위는 원래 예정 시각의 시·분을 유지한다.
 * 이미 지난 일정은 오늘을 기준으로, 아직 남은 일정은 예정일을 기준으로 미룬다.
 */
export function snoozedTime(
  dueAt: string,
  option: (typeof snoozeOptions)[number],
  now = new Date(),
) {
  const due = new Date(dueAt);
  if ("hours" in option)
    return new Date(
      Math.max(due.getTime(), now.getTime()) + option.hours * 3_600_000,
    ).toISOString();
  const next = new Date(due);
  if (due < now)
    next.setFullYear(now.getFullYear(), now.getMonth(), now.getDate());
  next.setDate(next.getDate() + option.days);
  return next.toISOString();
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
