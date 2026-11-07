import { useEffect, useState, type ReactNode } from "react";
import { invoke, isTauri } from "@tauri-apps/api/core";
import { disable, enable, isEnabled } from "@tauri-apps/plugin-autostart";
import {
  isPermissionGranted,
  requestPermission,
} from "@tauri-apps/plugin-notification";
import {
  ArrowDownToLine,
  ArrowRight,
  ArrowUpRight,
  ArrowUpFromLine,
  Bell,
  Check,
  CheckCheck,
  ChevronDown,
  ChevronRight,
  Circle,
  Clock3,
  ExternalLink,
  FolderPlus,
  HardDrive,
  History,
  House,
  Leaf,
  LoaderCircle,
  MapPin,
  MessageSquare,
  Monitor,
  Moon,
  Package,
  Plus,
  RefreshCw,
  Search,
  ShieldCheck,
  SkipForward,
  Sun,
  Trash2,
  Undo2,
  X,
} from "lucide-react";
import EntityEditor, { Modal } from "./Editor";
import TaskCalendar from "./Calendar";
import { useTheme, type ThemePreference } from "./theme";
import {
  dateTime,
  expiryLabel,
  localDate,
  reminderLabel,
  scheduleLabel,
  spacePath,
  taskTarget,
  type AppInfo,
  type Editor,
  type Item,
  type ReleaseCheck,
  type Snapshot,
  type Space,
  type Task,
  type TaskRecord,
} from "./types";

type Page =
  "today" | "calendar" | "spaces" | "items" | "tasks" | "history" | "settings";
const pageTitles: Record<Page, string> = {
  today: "오늘의 집사",
  calendar: "달력",
  spaces: "내 공간",
  items: "모든 아이템",
  tasks: "할 일",
  history: "수행 기록",
  settings: "설정",
};
const pageIndex: Record<Page, string> = {
  today: "01",
  calendar: "02",
  spaces: "03",
  items: "04",
  tasks: "05",
  history: "06",
  settings: "07",
};
const themeOptions: [ThemePreference, typeof Monitor, string][] = [
  ["system", Monitor, "시스템 설정"],
  ["light", Sun, "라이트"],
  ["dark", Moon, "다크"],
];

function ThemePreview({ mode }: { mode: ThemePreference }) {
  const pane = (tone: "light" | "dark") => (
    <span className={`tp-pane ${tone}`}>
      <span className="tp-side">
        <i />
        <i />
        <i />
      </span>
      <span className="tp-body">
        <i className="tp-title" />
        <span className="tp-cards">
          <i />
          <i />
        </span>
      </span>
    </span>
  );
  return (
    <span
      className={`theme-preview ${mode === "system" ? "split" : ""}`}
      aria-hidden="true"
    >
      {mode === "system" ? (
        <>
          {pane("light")}
          {pane("dark")}
        </>
      ) : (
        pane(mode)
      )}
    </span>
  );
}

function Eyebrow({ index, children }: { index?: string; children: ReactNode }) {
  return (
    <div className="eyebrow">
      {index && <span className="eyebrow-index">{index}</span>}
      <span>{children}</span>
    </div>
  );
}

function Empty({
  icon = <Leaf size={30} />,
  title,
  description,
  action,
}: {
  icon?: ReactNode;
  title: string;
  description: string;
  action?: ReactNode;
}) {
  return (
    <div className="empty-state">
      <div className="empty-icon">{icon}</div>
      <h3>{title}</h3>
      <p>{description}</p>
      {action}
    </div>
  );
}

export default function App() {
  const [data, setData] = useState<Snapshot | null>(null);
  const [info, setInfo] = useState<AppInfo | null>(null);
  const [loadError, setLoadError] = useState("");
  const [page, setPage] = useState<Page>("today");
  const [selectedId, setSelectedId] = useState<string | null>(null);
  const [collapsed, setCollapsed] = useState(new Set<string>());
  const [query, setQuery] = useState("");
  const [editor, setEditor] = useState<Editor | null>(null);
  const [deletion, setDeletion] = useState<{
    kind: Editor["kind"];
    id: string;
  } | null>(null);
  const [deletionError, setDeletionError] = useState("");
  const [busy, setBusy] = useState(false);
  const [notice, setNotice] = useState<{
    text: string;
    error?: boolean;
  } | null>(null);
  const [now, setNow] = useState(new Date());
  const [taskFilter, setTaskFilter] = useState("active");
  const [itemFilter, setItemFilter] = useState("all");
  const [autoStart, setAutoStart] = useState(false);
  const [autoStartAvailable, setAutoStartAvailable] = useState(false);
  const [updateBusy, setUpdateBusy] = useState(false);
  const [updateMessage, setUpdateMessage] = useState("");
  const [updateError, setUpdateError] = useState(false);
  const [releaseCheck, setReleaseCheck] = useState<ReleaseCheck | null>(null);
  const [feedbackBusy, setFeedbackBusy] = useState(false);
  const theme = useTheme();

  async function load() {
    setLoadError("");
    if (!isTauri()) {
      setLoadError(
        "집사는 설치형 데스크톱 앱이에요. 터미널에서 npm run desktop으로 실행해 주세요.",
      );
      return;
    }
    try {
      const [snapshot, appInfo] = await Promise.all([
        invoke<Snapshot>("load_data"),
        invoke<AppInfo>("app_info"),
      ]);
      setData(snapshot);
      setInfo(appInfo);
    } catch (e) {
      setLoadError(String(e));
    }
  }
  useEffect(() => {
    void load();
    if (isTauri())
      void isEnabled()
        .then((value) => {
          setAutoStart(value);
          setAutoStartAvailable(true);
        })
        .catch(() => {});
    const clock = window.setInterval(() => setNow(new Date()), 30_000);
    return () => {
      window.clearInterval(clock);
    };
  }, []);

  const notifyError = (e: unknown) =>
    setNotice({ text: String(e), error: true });
  function navigate(next: Page, id: string | null = null) {
    setPage(next);
    setSelectedId(id);
    setQuery("");
  }
  function openEditor(next: Editor) {
    if (next.kind !== "space" && !data?.spaces.length) {
      setEditor({ kind: "space", requiredFor: next.kind });
      return;
    }
    setEditor(next);
  }
  async function save(kind: Editor["kind"], value: Space | Item | Task) {
    setBusy(true);
    try {
      setData(await invoke<Snapshot>(`save_${kind}`, { [kind]: value }));
      setEditor(null);
      setNotice({ text: "저장했어요." });
    } finally {
      setBusy(false);
    }
  }
  async function finish(task: Task, outcome: "completed" | "skipped") {
    if (busy) return;
    setBusy(true);
    try {
      setData(
        await invoke<Snapshot>("finish_task", {
          id: task.id,
          expectedDue: task.dueAt,
          outcome,
        }),
      );
      setNotice({
        text:
          outcome === "completed"
            ? "완료했어요. 수행 기록에 남겨 두었습니다."
            : "이번 회차를 건너뛰었어요.",
      });
    } catch (e) {
      notifyError(e);
    } finally {
      setBusy(false);
    }
  }
  async function reopen(record: TaskRecord) {
    if (busy) return;
    setBusy(true);
    try {
      setData(
        await invoke<Snapshot>("reopen_task", {
          id: record.taskId,
          recordId: record.id,
        }),
      );
      if (page === "tasks") setTaskFilter("active");
      setNotice({ text: "미완료로 되돌렸어요. 이전 예정일로 복원했습니다." });
    } catch (e) {
      notifyError(e);
    } finally {
      setBusy(false);
    }
  }
  async function deleteEntity() {
    if (!deletion) return;
    setBusy(true);
    try {
      setData(await invoke<Snapshot>("delete_entity", deletion));
      setEditor(null);
      setDeletion(null);
      setNotice({ text: "삭제했어요. 기존 수행 기록은 보관됩니다." });
    } catch (e) {
      setDeletionError(String(e));
    } finally {
      setBusy(false);
    }
  }
  async function toggleNotifications() {
    if (!data) return;
    setBusy(true);
    try {
      const enabled = !data.settings.notifications;
      if (
        enabled &&
        !(await isPermissionGranted()) &&
        (await requestPermission()) !== "granted"
      )
        throw new Error("시스템 설정에서 집사의 알림을 허용해 주세요.");
      setData(
        await invoke<Snapshot>("save_settings", {
          settings: { ...data.settings, notifications: enabled },
        }),
      );
    } catch (e) {
      notifyError(e);
    } finally {
      setBusy(false);
    }
  }
  async function toggleAutoStart() {
    setBusy(true);
    try {
      if (autoStart) await disable();
      else await enable();
      setAutoStart(await isEnabled());
    } catch (e) {
      notifyError(e);
    } finally {
      setBusy(false);
    }
  }
  async function backup(restore: boolean) {
    setBusy(true);
    try {
      if (restore) {
        const result = await invoke<{
          data: Snapshot;
          recoveryPath: string;
        } | null>("import_backup");
        if (result) {
          setData(result.data);
          setSelectedId(null);
          setNotice({
            text: `복원했어요. 교체 전 백업: ${result.recoveryPath}`,
          });
        }
      } else {
        const path = await invoke<string | null>("export_backup");
        if (path)
          setNotice({ text: `사진과 수행 기록을 포함해 백업했어요. ${path}` });
      }
    } catch (e) {
      notifyError(e);
    } finally {
      setBusy(false);
    }
  }
  async function checkUpdates() {
    setUpdateBusy(true);
    setUpdateError(false);
    setReleaseCheck(null);
    setUpdateMessage("최신 버전을 확인하고 있어요.");
    try {
      const result = await invoke<ReleaseCheck>("check_latest_release");
      setReleaseCheck(result);
      setUpdateMessage(
        !result.latestVersion
          ? "아직 공개된 정식 릴리스가 없어요."
          : result.updateAvailable
            ? `v${result.latestVersion} 새 버전이 있어요.`
            : "새로운 버전이 없어요. 현재 버전을 계속 사용하시면 됩니다.",
      );
    } catch (e) {
      setUpdateError(true);
      setUpdateMessage(`최신 버전을 확인하지 못했어요. ${String(e)}`);
    } finally {
      setUpdateBusy(false);
    }
  }
  async function openDownloadPage(latest: boolean) {
    try {
      await invoke("open_download_page", { latest });
    } catch (e) {
      notifyError(e);
    }
  }
  async function openFeedbackForm() {
    setFeedbackBusy(true);
    try {
      await invoke("open_feedback_form");
    } catch (e) {
      notifyError(e);
    } finally {
      setFeedbackBusy(false);
    }
  }

  if (!data)
    return (
      <main className="loading-screen">
        <div className="brand-mark" aria-hidden="true">
          집
        </div>
        <h1>
          집사 <span>Zipsa</span>
        </h1>
        {loadError ? (
          <>
            <p role="alert">{loadError}</p>
            <button className="button primary" onClick={() => void load()}>
              다시 시도
            </button>
          </>
        ) : (
          <p>
            <LoaderCircle className="spin" size={18} />내 공간을 불러오는
            중이에요.
          </p>
        )}
      </main>
    );

  const selected = data.spaces.find((s) => s.id === selectedId);
  const normalizedQuery = query.trim().toLocaleLowerCase();
  const matches = (...parts: (string | null | undefined)[]) =>
    !normalizedQuery ||
    parts.join(" ").toLocaleLowerCase().includes(normalizedQuery);
  const todayEnd = new Date(now);
  todayEnd.setHours(23, 59, 59, 999);
  const sevenDays = new Date(now);
  sevenDays.setDate(sevenDays.getDate() + 7);
  const activeTasks = data.tasks
    .filter((t) => t.active)
    .sort((a, b) => Date.parse(a.dueAt) - Date.parse(b.dueAt));
  const dueTasks = activeTasks.filter((t) => new Date(t.dueAt) <= todayEnd);
  const upcomingTasks = activeTasks.filter(
    (t) => new Date(t.dueAt) > todayEnd && new Date(t.dueAt) <= sevenDays,
  );
  const expiringItems = data.items
    .filter((i) => i.expiresOn && i.expiresOn <= localDate(sevenDays))
    .sort((a, b) => a.expiresOn!.localeCompare(b.expiresOn!));
  const inSelected = (id: string) => {
    if (!selected) return true;
    let current = data.spaces.find((s) => s.id === id);
    const seen = new Set<string>();
    while (current && !seen.has(current.id)) {
      seen.add(current.id);
      if (current.id === selected.id) return true;
      current = data.spaces.find((s) => s.id === current?.parentId);
    }
    return false;
  };
  const scopedItems = data.items.filter((i) => inSelected(i.spaceId));
  const scopedTasks = data.tasks.filter((t) =>
    t.spaceId
      ? inSelected(t.spaceId)
      : scopedItems.some((i) => i.id === t.itemId),
  );
  const completedToday = data.records.filter(
    (r) =>
      r.outcome === "completed" &&
      localDate(new Date(r.completedAt)) === localDate(now),
  ).length;
  const latestRecords = new Map<string, TaskRecord>();
  for (const record of data.records) latestRecords.set(record.taskId, record);

  function tree(parentId: string | null, depth = 0): ReactNode {
    return data!.spaces
      .filter((s) => s.parentId === parentId)
      .map((space) => {
        const hasChildren = data!.spaces.some((s) => s.parentId === space.id);
        const isCollapsed = collapsed.has(space.id);
        return (
          <div key={space.id}>
            <div
              className={`tree-row ${page === "spaces" && selectedId === space.id && !normalizedQuery ? "active" : ""}`}
              style={{ paddingLeft: 10 + depth * 15 }}
            >
              {hasChildren ? (
                <button
                  type="button"
                  className="tree-chevron"
                  aria-label={`${space.name} ${isCollapsed ? "펼치기" : "접기"}`}
                  aria-expanded={!isCollapsed}
                  onClick={() =>
                    setCollapsed((previous) => {
                      const next = new Set(previous);
                      if (next.has(space.id)) next.delete(space.id);
                      else next.add(space.id);
                      return next;
                    })
                  }
                >
                  {isCollapsed ? (
                    <ChevronRight size={13} />
                  ) : (
                    <ChevronDown size={13} />
                  )}
                </button>
              ) : (
                <span className="tree-chevron" />
              )}
              <button
                className="tree-label"
                onClick={() => navigate("spaces", space.id)}
              >
                <span>{space.icon || "📦"}</span>
                <span>{space.name}</span>
              </button>
            </div>
            {!isCollapsed && tree(space.id, depth + 1)}
          </div>
        );
      });
  }
  function taskRows(tasks: Task[], emptyMessage = "등록된 할 일이 없어요.") {
    if (!tasks.length)
      return (
        <div className="inline-empty">
          <CheckCheck size={20} />
          <span>{emptyMessage}</span>
        </div>
      );
    return (
      <div className="task-list">
        {tasks.map((task) => {
          const record = latestRecords.get(task.id);
          return (
            <div
              className={`task-row ${!task.active ? "task-done" : ""}`}
              key={task.id}
            >
              <button
                className="complete-button"
                disabled={busy || (!task.active && !record)}
                aria-label={`${task.title} ${task.active ? "완료" : record ? "미완료로 되돌리기" : "일정 비활성"}`}
                title={!task.active && record ? "미완료로 되돌리기" : undefined}
                onClick={() =>
                  task.active
                    ? void finish(task, "completed")
                    : record && void reopen(record)
                }
              >
                {task.active ? (
                  <span className="tick" />
                ) : record ? (
                  <span className="tick done">
                    <Check size={14} strokeWidth={3} />
                  </span>
                ) : (
                  <span className="tick idle">
                    <Clock3 size={13} />
                  </span>
                )}
              </button>
              <button
                className="task-body"
                onClick={() => openEditor({ kind: "task", value: task })}
              >
                <strong>{task.title}</strong>
                <span>
                  <MapPin size={12} />
                  {taskTarget(data!, task)}
                </span>
              </button>
              <div className="task-meta">
                <span className="schedule-tag">{scheduleLabel(task)}</span>
                {task.reminderMinutes && (
                  <span
                    className="reminder-tag"
                    title={`미리 알림 ${reminderLabel(task.reminderMinutes)}`}
                  >
                    <Bell size={12} />
                    {reminderLabel(task.reminderMinutes)}
                  </span>
                )}
                <span
                  className={
                    task.active && new Date(task.dueAt) < now ? "overdue" : ""
                  }
                >
                  {task.active ? dateTime(task.dueAt) : "종료됨"}
                </span>
              </div>
              {task.active && (
                <button
                  className="icon-button skip-button"
                  disabled={busy}
                  title="이번 회차 건너뛰기"
                  aria-label={`${task.title} 이번 회차 건너뛰기`}
                  onClick={() => void finish(task, "skipped")}
                >
                  <SkipForward size={17} />
                </button>
              )}
              <button
                className="icon-button task-delete"
                disabled={busy}
                title="할 일 삭제"
                aria-label={`${task.title} 삭제`}
                onClick={() => {
                  setDeletionError("");
                  setDeletion({ kind: "task", id: task.id });
                }}
              >
                <Trash2 size={16} />
              </button>
            </div>
          );
        })}
      </div>
    );
  }
  function itemCards(items: Item[]) {
    return (
      <div className="item-grid">
        {items.map((item) => (
          <article className="item-card" key={item.id}>
            <button
              className="item-open"
              onClick={() => openEditor({ kind: "item", value: item })}
            >
              <div className={`item-image ${item.photo ? "" : "no-image"}`}>
                {item.photo ? (
                  <img src={item.photo} alt={item.name} loading="lazy" />
                ) : (
                  <Package size={38} strokeWidth={1.15} />
                )}
                {item.expiresOn && (
                  <span
                    className={`expiry-pill ${item.expiresOn <= localDate(now) ? "urgent" : ""}`}
                  >
                    {expiryLabel(item.expiresOn)}
                  </span>
                )}
              </div>
              <div className="item-info">
                <div className="item-title">
                  <h3>{item.name}</h3>
                  <span>
                    {item.quantity.toLocaleString("ko-KR")}
                    {item.unit}
                  </span>
                </div>
                <p>
                  <MapPin size={12} />
                  {spacePath(data!.spaces, item.spaceId)}
                </p>
                {item.purchaseStore && (
                  <p className="purchase-info">
                    {item.purchaseStore}
                    {item.purchasePrice !== null &&
                      ` · ${item.purchasePrice.toLocaleString("ko-KR")}원`}
                  </p>
                )}
                {item.notes && <p className="item-note">{item.notes}</p>}
              </div>
            </button>
            <button
              className="item-add-task"
              onClick={() => openEditor({ kind: "task", itemId: item.id })}
            >
              <Plus size={13} />할 일 연결
            </button>
          </article>
        ))}
      </div>
    );
  }
  function spaceCards(spaces: Space[]) {
    return (
      <div className="space-grid">
        {spaces.map((space) => (
          <button
            className="space-card"
            key={space.id}
            onClick={() => navigate("spaces", space.id)}
          >
            <span className="space-emoji">{space.icon || "📦"}</span>
            <div>
              <h3>{space.name}</h3>
              <p>
                하위 공간{" "}
                {data!.spaces.filter((s) => s.parentId === space.id).length} ·
                아이템{" "}
                {data!.items.filter((i) => i.spaceId === space.id).length}
              </p>
            </div>
            <ArrowRight size={16} />
          </button>
        ))}
      </div>
    );
  }
  const addSpace = (
    <button
      className="button primary"
      onClick={() => openEditor({ kind: "space" })}
    >
      <Plus size={16} />첫 공간 만들기
    </button>
  );

  return (
    <div className="app-shell">
      <aside className="sidebar">
        <button className="brand" onClick={() => navigate("today")}>
          <span className="brand-mark" aria-hidden="true">
            집
          </span>
          <span>
            집사<small>Zipsa</small>
          </span>
        </button>
        <div className="sidebar-caption">내 공간을 돌보는 작은 습관</div>
        <nav className="main-nav" aria-label="주 메뉴">
          {(
            [
              ["today", "오늘의 집사"],
              ["calendar", "달력"],
              ["spaces", "내 공간"],
              ["items", "모든 아이템"],
              ["tasks", "할 일"],
              ["history", "수행 기록"],
            ] as const
          ).map(([key, label]) => (
            <button
              key={key}
              className={`nav-item ${page === key && !normalizedQuery ? "active" : ""}`}
              onClick={() => navigate(key)}
            >
              <span className="nav-index" aria-hidden="true">
                {pageIndex[key]}
              </span>
              <span>{label}</span>
              {key === "tasks" && dueTasks.length > 0 && (
                <span className="nav-count">{dueTasks.length}</span>
              )}
            </button>
          ))}
        </nav>
        <div className="tree-heading">
          <span>나의 공간</span>
          <button
            className="icon-button"
            aria-label="최상위 공간 추가"
            onClick={() => openEditor({ kind: "space" })}
          >
            <Plus size={15} />
          </button>
        </div>
        <nav className="space-tree" aria-label="공간 트리">
          {data.spaces.length ? (
            tree(null)
          ) : (
            <button
              className="tree-placeholder"
              onClick={() => openEditor({ kind: "space" })}
            >
              첫 공간을 만들어 보세요
              <Plus size={14} />
            </button>
          )}
        </nav>
        <div className="sidebar-bottom">
          <div className="local-label">
            <span className="status-dot" />
            <span>내 컴퓨터에만 저장</span>
            <HardDrive size={14} />
          </div>
          <button
            className={`nav-item ${page === "settings" && !normalizedQuery ? "active" : ""}`}
            onClick={() => navigate("settings")}
          >
            <span className="nav-index" aria-hidden="true">
              {pageIndex.settings}
            </span>
            설정<span className="version">v{info?.version ?? "0.1.0"}</span>
          </button>
        </div>
      </aside>
      <div className="workspace">
        <header className="topbar">
          <div className="breadcrumb">
            <span className="crumb-root">집사</span>
            <span className="crumb-sep" aria-hidden="true">
              /
            </span>
            <span className="crumb-current">
              {selected && page === "spaces"
                ? spacePath(data.spaces, selected.id)
                : pageTitles[page]}
            </span>
          </div>
          <div className="topbar-actions">
            <label className="global-search">
              <Search size={16} />
              <input
                aria-label="공간, 아이템, 할 일 검색"
                placeholder="공간, 아이템, 할 일 검색"
                value={query}
                onChange={(e) => setQuery(e.target.value)}
              />
              {query && (
                <button
                  className="icon-button"
                  aria-label="검색 지우기"
                  onClick={() => setQuery("")}
                >
                  <X size={14} />
                </button>
              )}
            </label>
            <button
              className="icon-button theme-toggle"
              aria-label={
                theme.resolved === "dark"
                  ? "라이트 테마로 전환"
                  : "다크 테마로 전환"
              }
              title={
                theme.resolved === "dark"
                  ? "라이트 테마로 전환"
                  : "다크 테마로 전환"
              }
              onClick={() =>
                theme.setPreference(
                  theme.resolved === "dark" ? "light" : "dark",
                )
              }
            >
              {theme.resolved === "dark" ? (
                <Sun size={17} />
              ) : (
                <Moon size={17} />
              )}
            </button>
          </div>
        </header>
        <main
          className={`main-content ${page === "calendar" && !normalizedQuery ? "calendar-content" : ""}`}
        >
          {normalizedQuery ? (
            <>
              <div className="page-heading">
                <div>
                  <Eyebrow>SEARCH</Eyebrow>
                  <h1>‘{query.trim()}’ 검색 결과</h1>
                  <p>이름, 메모, 구매처와 보관 위치를 함께 찾아요.</p>
                </div>
              </div>
              <section className="section">
                <div className="section-heading">
                  <h2>공간</h2>
                </div>
                {spaceCards(
                  data.spaces.filter((s) =>
                    matches(s.name, s.notes, spacePath(data.spaces, s.id)),
                  ),
                )}
              </section>
              <section className="section">
                <div className="section-heading">
                  <h2>아이템</h2>
                </div>
                {itemCards(
                  data.items.filter((i) =>
                    matches(
                      i.name,
                      i.notes,
                      i.purchaseStore,
                      spacePath(data.spaces, i.spaceId),
                    ),
                  ),
                )}
              </section>
              <section className="section panel">
                <div className="section-heading">
                  <h2>할 일</h2>
                </div>
                {taskRows(
                  data.tasks.filter((t) =>
                    matches(t.title, t.notes, taskTarget(data, t)),
                  ),
                  "검색된 할 일이 없어요.",
                )}
              </section>
            </>
          ) : page === "today" ? (
            <>
              <div className="page-heading">
                <div>
                  <Eyebrow index={pageIndex.today}>
                    TODAY · {localDate(now).split("-").join(".")}{" "}
                    {new Intl.DateTimeFormat("ko-KR", {
                      weekday: "short",
                    }).format(now)}
                  </Eyebrow>
                  <h1>오늘도, 편안한 내 공간.</h1>
                  <p>
                    어디에 무엇이 있는지, 언제 돌봐야 하는지. 집사가 기억할게요.
                  </p>
                </div>
                <button
                  className="button primary"
                  onClick={() => openEditor({ kind: "task" })}
                >
                  <Plus size={16} />할 일 추가
                </button>
              </div>
              <div className="stats-grid">
                {(
                  [
                    [
                      "관리하는 공간",
                      data.spaces.length,
                      "곳",
                      () => navigate("spaces"),
                    ],
                    [
                      "보관 중인 아이템",
                      data.items.length,
                      "종류",
                      () => navigate("items"),
                    ],
                    [
                      "오늘까지 할 일",
                      dueTasks.length,
                      "개",
                      () => {
                        navigate("tasks");
                        setTaskFilter("due");
                      },
                    ],
                    [
                      "오늘 완료한 일",
                      completedToday,
                      "개",
                      () => navigate("history"),
                    ],
                  ] as const
                ).map(([label, value, unit, onClick]) => (
                  <button
                    key={label}
                    className={`stat-card ${label === "오늘까지 할 일" && value > 0 ? "attention" : ""}`}
                    onClick={onClick}
                  >
                    <span className="stat-label">{label}</span>
                    <strong>
                      {value}
                      <small>{unit}</small>
                    </strong>
                    <ArrowUpRight size={16} />
                  </button>
                ))}
              </div>
              {!data.spaces.length ? (
                <section className="welcome panel">
                  <Empty
                    icon={<House size={38} strokeWidth={1.3} />}
                    title="집사와 첫 공간을 만들어 볼까요?"
                    description="우리 집부터 책장의 작은 칸까지, 나만의 방식으로 정리해 보세요."
                    action={addSpace}
                  />
                  <div className="welcome-example">
                    <span>🏠 우리 집</span>
                    <ChevronRight size={15} />
                    <span>🍳 주방</span>
                    <ChevronRight size={15} />
                    <span>🧊 냉장고</span>
                    <ChevronRight size={15} />
                    <span>🥛 우유</span>
                  </div>
                </section>
              ) : (
                <>
                  <section className="section panel">
                    <div className="section-heading">
                      <h2>
                        오늘 챙길 일
                        <span className="count">{dueTasks.length}</span>
                      </h2>
                      <button
                        className="text-button"
                        onClick={() => navigate("tasks")}
                      >
                        모든 할 일<ArrowRight size={14} />
                      </button>
                    </div>
                    {taskRows(
                      dueTasks,
                      "오늘 챙길 일을 모두 마쳤어요. 편안하게 쉬세요.",
                    )}
                  </section>
                  <div className="dashboard-columns">
                    <section className="section panel">
                      <div className="section-heading">
                        <h2>다가오는 일정</h2>
                        <span className="subtle">7일 이내</span>
                      </div>
                      {taskRows(
                        upcomingTasks.slice(0, 5),
                        "다가오는 7일 동안 예정된 일이 없어요.",
                      )}
                    </section>
                    <section className="section panel">
                      <div className="section-heading">
                        <h2>유통기한 확인</h2>
                        <span className="subtle">7일 이내·기한 경과</span>
                      </div>
                      {expiringItems.length ? (
                        <div className="expiry-list">
                          {expiringItems.slice(0, 6).map((i) => (
                            <button
                              key={i.id}
                              onClick={() =>
                                openEditor({ kind: "item", value: i })
                              }
                            >
                              <span className="expiry-mini">
                                {i.photo ? (
                                  <img src={i.photo} alt="" />
                                ) : (
                                  <Package size={20} />
                                )}
                              </span>
                              <span>
                                <strong>{i.name}</strong>
                                <small>
                                  {spacePath(data.spaces, i.spaceId)}
                                </small>
                              </span>
                              <span
                                className={
                                  i.expiresOn! <= localDate(now)
                                    ? "overdue"
                                    : "expiry-text"
                                }
                              >
                                {expiryLabel(i.expiresOn!)}
                              </span>
                            </button>
                          ))}
                        </div>
                      ) : (
                        <div className="inline-empty">
                          <Leaf size={20} />
                          유통기한이 임박한 아이템이 없어요.
                        </div>
                      )}
                    </section>
                  </div>
                  <section className="section">
                    <div className="section-heading">
                      <h2>나의 공간</h2>
                      <button
                        className="text-button"
                        onClick={() => openEditor({ kind: "space" })}
                      >
                        <Plus size={14} />
                        공간 추가
                      </button>
                    </div>
                    {spaceCards(data.spaces.filter((s) => !s.parentId))}
                  </section>
                </>
              )}
            </>
          ) : page === "calendar" ? (
            <TaskCalendar
              data={data}
              now={now}
              busy={busy}
              onAdd={(dueAt) => openEditor({ kind: "task", dueAt })}
              onEdit={(task) => openEditor({ kind: "task", value: task })}
              onFinish={finish}
              onReopen={reopen}
            />
          ) : page === "spaces" ? (
            <>
              <div className="page-heading">
                <div>
                  <Eyebrow index={pageIndex.spaces}>SPACES</Eyebrow>
                  <h1>
                    {selected && (
                      <span className="title-emoji">{selected.icon}</span>
                    )}
                    {selected?.name ?? "내 공간"}
                  </h1>
                  <p>
                    {selected?.notes ||
                      "작은 칸부터 집 전체까지, 공간을 차곡차곡 연결해요."}
                  </p>
                </div>
                <div className="heading-actions">
                  {selected && (
                    <button
                      className="button secondary"
                      onClick={() =>
                        openEditor({ kind: "space", value: selected })
                      }
                    >
                      공간 수정
                    </button>
                  )}
                  <button
                    className="button primary"
                    onClick={() =>
                      openEditor({ kind: "space", parentId: selected?.id })
                    }
                  >
                    <FolderPlus size={16} />
                    {selected ? "하위 공간 추가" : "공간 추가"}
                  </button>
                </div>
              </div>
              {!data.spaces.length ? (
                <Empty
                  icon={<House size={34} />}
                  title="모든 정리는 공간에서 시작해요"
                  description="집이나 사무실을 만들고 그 안에 더 작은 공간을 추가해 보세요."
                  action={addSpace}
                />
              ) : (
                <>
                  <section className="section">
                    <div className="section-heading">
                      <h2>{selected ? "하위 공간" : "최상위 공간"}</h2>
                    </div>
                    {spaceCards(
                      data.spaces.filter(
                        (s) => s.parentId === (selected?.id ?? null),
                      ),
                    )}
                  </section>
                  <section className="section">
                    <div className="section-heading">
                      <h2>
                        아이템
                        <span className="count">{scopedItems.length}</span>
                        {selected && (
                          <span className="subtle">하위 공간 포함</span>
                        )}
                      </h2>
                      <button
                        className="text-button"
                        onClick={() =>
                          openEditor({ kind: "item", spaceId: selected?.id })
                        }
                      >
                        <Plus size={14} />
                        아이템 추가
                      </button>
                    </div>
                    {scopedItems.length ? (
                      itemCards(scopedItems)
                    ) : (
                      <div className="inline-empty">
                        아이템을 추가해 보세요.
                      </div>
                    )}
                  </section>
                  <section className="section panel">
                    <div className="section-heading">
                      <h2>
                        연결된 할 일
                        <span className="count">
                          {scopedTasks.filter((t) => t.active).length}
                        </span>
                      </h2>
                      <button
                        className="text-button"
                        onClick={() =>
                          openEditor({ kind: "task", spaceId: selected?.id })
                        }
                      >
                        <Plus size={14} />할 일 추가
                      </button>
                    </div>
                    {taskRows(scopedTasks.filter((t) => t.active))}
                  </section>
                </>
              )}
            </>
          ) : page === "items" ? (
            <>
              <div className="page-heading">
                <div>
                  <Eyebrow index={pageIndex.items}>ITEMS</Eyebrow>
                  <h1>모든 아이템</h1>
                  <p>물건의 위치와 구매정보를 한눈에 확인하세요.</p>
                </div>
                <button
                  className="button primary"
                  onClick={() => openEditor({ kind: "item" })}
                >
                  <Plus size={16} />
                  아이템 추가
                </button>
              </div>
              <div className="filter-bar">
                <div className="tabs">
                  {[
                    ["all", "전체"],
                    ["soon", "7일 이내"],
                    ["expired", "기한 경과"],
                  ].map(([key, label]) => (
                    <button
                      key={key}
                      className={itemFilter === key ? "active" : ""}
                      onClick={() => setItemFilter(key)}
                    >
                      {label}
                    </button>
                  ))}
                </div>
                <span className="subtle">전체 {data.items.length}종류</span>
              </div>
              {data.items.length ? (
                itemCards(
                  data.items.filter(
                    (i) =>
                      itemFilter === "all" ||
                      (i.expiresOn &&
                        (itemFilter === "expired"
                          ? i.expiresOn < localDate(now)
                          : i.expiresOn >= localDate(now) &&
                            i.expiresOn <= localDate(sevenDays))),
                  ),
                )
              ) : (
                <Empty
                  icon={<Package size={34} />}
                  title="물건의 자리를 기억해 둘게요"
                  description="사진과 보관 위치, 유통기한, 구매정보를 함께 기록해 보세요."
                  action={
                    <button
                      className="button primary"
                      onClick={() => openEditor({ kind: "item" })}
                    >
                      <Plus size={16} />
                      아이템 추가
                    </button>
                  }
                />
              )}
            </>
          ) : page === "tasks" ? (
            <>
              <div className="page-heading">
                <div>
                  <Eyebrow index={pageIndex.tasks}>TASKS</Eyebrow>
                  <h1>할 일</h1>
                  <p>공간도, 물건도. 알맞은 때에 돌봐 주세요.</p>
                </div>
                <button
                  className="button primary"
                  onClick={() => openEditor({ kind: "task" })}
                >
                  <Plus size={16} />할 일 추가
                </button>
              </div>
              <div className="filter-bar">
                <div className="tabs">
                  {[
                    ["active", "예정된 일"],
                    ["due", "오늘까지"],
                    ["done", "종료된 일"],
                  ].map(([key, label]) => (
                    <button
                      key={key}
                      className={taskFilter === key ? "active" : ""}
                      onClick={() => setTaskFilter(key)}
                    >
                      {label}
                    </button>
                  ))}
                </div>
                <span className="subtle">
                  반복 일정의 완료 내역은 수행 기록에 남아요.
                </span>
              </div>
              <section className="panel task-panel">
                {taskRows(
                  taskFilter === "due"
                    ? dueTasks
                    : taskFilter === "done"
                      ? data.tasks.filter((t) => !t.active)
                      : activeTasks,
                )}
              </section>
            </>
          ) : page === "history" ? (
            <>
              <div className="page-heading">
                <div>
                  <Eyebrow index={pageIndex.history}>HISTORY</Eyebrow>
                  <h1>수행 기록</h1>
                  <p>내 공간을 돌본 시간이 차곡차곡 쌓여요.</p>
                </div>
              </div>
              {data.records.length ? (
                <section className="panel history-panel">
                  {[...data.records]
                    .sort(
                      (a, b) =>
                        Date.parse(b.completedAt) - Date.parse(a.completedAt),
                    )
                    .map((r) => (
                      <div className="history-row" key={r.id}>
                        <span
                          className={`history-status ${r.outcome === "skipped" ? "skipped" : ""}`}
                        >
                          {r.outcome === "completed" ? (
                            <Check size={18} />
                          ) : (
                            <SkipForward size={18} />
                          )}
                        </span>
                        <div className="history-details">
                          <strong>{r.title}</strong>
                          <p>{r.target}</p>
                          <small>예정 {dateTime(r.scheduledFor)}</small>
                        </div>
                        <div className="history-date">
                          <span>
                            {r.outcome === "completed" ? "완료" : "건너뜀"}
                          </span>
                          <time>
                            {new Date(r.completedAt).toLocaleString("ko-KR")}
                          </time>
                        </div>
                        {latestRecords.get(r.taskId)?.id === r.id &&
                          data.tasks.some((task) => task.id === r.taskId) && (
                            <button
                              className="button secondary history-undo"
                              disabled={busy}
                              aria-label={`${r.title} 미완료로 되돌리기`}
                              onClick={() => void reopen(r)}
                            >
                              <Undo2 size={15} />
                              {r.outcome === "completed"
                                ? "완료 취소"
                                : "건너뛰기 취소"}
                            </button>
                          )}
                      </div>
                    ))}
                </section>
              ) : (
                <Empty
                  icon={<History size={34} />}
                  title="돌본 흔적을 모아 둘게요"
                  description="할 일을 완료하거나 건너뛰면 이곳에서 확인할 수 있어요."
                />
              )}
            </>
          ) : (
            <>
              <div className="page-heading">
                <div>
                  <Eyebrow index={pageIndex.settings}>SETTINGS</Eyebrow>
                  <h1>설정</h1>
                  <p>집사를 내 생활에 맞게 설정하세요.</p>
                </div>
              </div>
              <div className="settings-stack">
                <section className="panel settings-section">
                  <div className="section-heading">
                    <h2>화면 테마</h2>
                  </div>
                  <p className="setting-description">
                    밝은 테마와 다크 테마 중에서 고르거나, 운영체제 설정을
                    따르도록 할 수 있어요.
                  </p>
                  <div
                    className="theme-options"
                    role="radiogroup"
                    aria-label="화면 테마"
                  >
                    {themeOptions.map(([key, Icon, label]) => (
                      <button
                        key={key}
                        role="radio"
                        aria-checked={theme.preference === key}
                        className={`theme-option ${theme.preference === key ? "selected" : ""}`}
                        onClick={() => theme.setPreference(key)}
                      >
                        <ThemePreview mode={key} />
                        <span className="theme-option-label">
                          <Icon size={16} />
                          {label}
                          {theme.preference === key && (
                            <Check className="theme-option-check" size={16} />
                          )}
                        </span>
                      </button>
                    ))}
                  </div>
                </section>
                <section className="panel settings-section">
                  <div className="section-heading">
                    <h2>알림과 실행</h2>
                  </div>
                  <div className="setting-row">
                    <div>
                      <strong>할 일 알림</strong>
                      <p>
                        예정 시간이 되면 시스템 알림을 보내요. 놓친 일정은 다음
                        실행 때 확인해요.
                      </p>
                    </div>
                    <button
                      role="switch"
                      aria-checked={data.settings.notifications}
                      aria-label="할 일 알림"
                      className={`switch ${data.settings.notifications ? "on" : ""}`}
                      disabled={busy}
                      onClick={() => void toggleNotifications()}
                    >
                      <span />
                    </button>
                  </div>
                  <div className="setting-row">
                    <div>
                      <strong>로그인할 때 집사 실행</strong>
                      <p>메뉴 막대·트레이에서 조용히 시작해요.</p>
                    </div>
                    <button
                      role="switch"
                      aria-checked={autoStart}
                      aria-label="로그인할 때 집사 실행"
                      className={`switch ${autoStart ? "on" : ""}`}
                      disabled={busy || !autoStartAvailable}
                      onClick={() => void toggleAutoStart()}
                    >
                      <span />
                    </button>
                  </div>
                  <div className="setting-note">
                    <Clock3 size={16} />
                    <p>
                      창을 닫아도 집사는 계속 실행돼요. 완전히 종료하려면 메뉴
                      막대·트레이에서 ‘집사 종료’를 선택하세요. 컴퓨터가 꺼져
                      있거나 잠든 동안에는 알림을 보낼 수 없어요.
                    </p>
                  </div>
                </section>
                <section className="panel settings-section">
                  <div className="section-heading">
                    <h2>내 데이터와 백업</h2>
                  </div>
                  <div className="privacy-note">
                    <ShieldCheck size={23} />
                    <div>
                      <strong>내 공간의 정보는 내 컴퓨터에만</strong>
                      <p>
                        계정이나 서버 없이 사진과 구매정보까지 모두 로컬에
                        저장해요.
                      </p>
                    </div>
                  </div>
                  <label>
                    저장 위치
                    <input
                      className="data-path"
                      readOnly
                      value={info?.dataPath ?? ""}
                    />
                  </label>
                  <p className="field-hint">
                    앱 잠금과 별도 암호화는 사용하지 않아요. 백업에는
                    사진·일정·수행 기록이 함께 포함돼요.
                  </p>
                  <div className="setting-actions">
                    <button
                      className="button secondary"
                      disabled={busy}
                      onClick={() => void backup(false)}
                    >
                      <ArrowDownToLine size={16} />
                      백업 내보내기
                    </button>
                    <button
                      className="button secondary"
                      disabled={busy}
                      onClick={() => void backup(true)}
                    >
                      <ArrowUpFromLine size={16} />
                      백업에서 복원
                    </button>
                  </div>
                </section>
                <section className="panel settings-section">
                  <div className="section-heading">
                    <h2>집사 업데이트</h2>
                    <span className="schedule-tag">v{info?.version}</span>
                  </div>
                  <p className="setting-description">
                    최신 버전을 확인하고 GitHub 다운로드 페이지로 이동해요.
                  </p>
                  <p className="field-hint">
                    확인할 때 GitHub에 연결해요. 공간·아이템·할 일 데이터는
                    전송하지 않아요.
                  </p>
                  <div className="setting-actions">
                    <button
                      className="button secondary"
                      disabled={updateBusy}
                      onClick={() => void checkUpdates()}
                    >
                      <RefreshCw
                        size={16}
                        className={updateBusy ? "spin" : ""}
                      />
                      최신 버전 확인
                    </button>
                    {releaseCheck?.updateAvailable && (
                      <button
                        className="button primary"
                        disabled={updateBusy}
                        onClick={() => void openDownloadPage(true)}
                      >
                        <ExternalLink size={16} />
                        다운로드 페이지로 이동
                      </button>
                    )}
                  </div>
                  {updateMessage && (
                    <p
                      className={`update-message ${updateError ? "error" : ""}`}
                      role={updateError ? "alert" : "status"}
                    >
                      {updateMessage}
                    </p>
                  )}
                  {releaseCheck?.updateAvailable && releaseCheck.notes && (
                    <div className="release-notes">
                      <strong>변경 내용</strong>
                      <p>{releaseCheck.notes}</p>
                    </div>
                  )}
                  <button
                    className="text-button release-link"
                    onClick={() => void openDownloadPage(false)}
                  >
                    GitHub 릴리스 보기
                    <ExternalLink size={13} />
                  </button>
                </section>
                {info?.feedbackAvailable && (
                  <section
                    className="panel settings-section"
                    aria-labelledby="feedback-heading"
                  >
                    <div className="section-heading">
                      <h2 id="feedback-heading">의견을 들려주세요</h2>
                    </div>
                    <p className="setting-description">
                      불편한 점이나 바라는 기능을 Google 설문지에 남겨 주세요.
                      답변받을 이메일은 선택해서 적을 수 있어요.
                    </p>
                    <p className="field-hint">
                      폼을 열면 앱 버전 v{info.version}과 운영체제 {info.os}를
                      미리 채워요. 공간·아이템·사진·할 일 정보는 함께 보내지
                      않아요.
                    </p>
                    <p className="field-hint">
                      의견은 브라우저에서 직접 작성하고 제출해요. 제출한 내용은
                      Google Forms에 저장되며 집사 개발자가 확인해요.
                    </p>
                    <div className="setting-actions">
                      <button
                        className="button secondary"
                        disabled={feedbackBusy}
                        onClick={() => void openFeedbackForm()}
                      >
                        <MessageSquare size={16} />
                        {feedbackBusy ? "브라우저 여는 중…" : "의견 보내기"}
                        <ExternalLink size={13} />
                      </button>
                    </div>
                  </section>
                )}
                <div className="app-signature">
                  <span className="brand-mark small" aria-hidden="true">
                    집
                  </span>
                  <strong>집사</strong>
                  <span>Zipsa · 나의 공간, 나의 기록.</span>
                </div>
              </div>
            </>
          )}
          <footer className="page-footer">
            <span>집사 · Zipsa</span>
            <span>내 공간을 아끼는 마음, 집사</span>
          </footer>
        </main>
      </div>
      {notice && (
        <div
          className={`toast ${notice.error ? "error" : ""}`}
          role={notice.error ? "alert" : "status"}
        >
          {notice.error ? <Circle size={17} /> : <Check size={17} />}
          <span>{notice.text}</span>
          <button
            className="icon-button"
            aria-label="메시지 닫기"
            onClick={() => setNotice(null)}
          >
            <X size={15} />
          </button>
        </div>
      )}
      {editor && (
        <EntityEditor
          key={`${editor.kind}-${editor.value?.id ?? "new"}`}
          editor={editor}
          data={data}
          busy={busy}
          onClose={() => setEditor(null)}
          onSave={save}
          onDelete={(kind, id) => {
            setDeletionError("");
            setDeletion({ kind, id });
          }}
        />
      )}
      {deletion && (
        <Modal
          title={
            deletion.kind === "task" ? "할 일을 삭제할까요?" : "삭제할까요?"
          }
          onClose={() => !busy && setDeletion(null)}
        >
          <p className="confirm-description">
            삭제한 항목은 되돌릴 수 없어요. 기존 수행 기록은 남겨 둡니다.
            {deletion.kind !== "task" &&
              " 연결된 공간·아이템·할 일이 있으면 먼저 옮기거나 삭제해야 해요."}
          </p>
          {deletionError && (
            <p className="form-error" role="alert">
              {deletionError}
            </p>
          )}
          <div className="modal-footer">
            <button
              className="button secondary"
              disabled={busy}
              onClick={() => setDeletion(null)}
            >
              취소
            </button>
            <button
              className="button danger"
              disabled={busy}
              onClick={() => void deleteEntity()}
            >
              삭제
            </button>
          </div>
        </Modal>
      )}
    </div>
  );
}
