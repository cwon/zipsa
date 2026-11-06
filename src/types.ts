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
  updaterReady: boolean;
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
  | { kind: "task"; value?: Task; spaceId?: string; itemId?: string };

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
