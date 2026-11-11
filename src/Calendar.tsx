import {
  useEffect,
  useMemo,
  useRef,
  useState,
  type CSSProperties,
  type KeyboardEvent,
} from "react";
import { invoke } from "@tauri-apps/api/core";
import {
  Bell,
  CalendarDays,
  Check,
  ChevronLeft,
  ChevronRight,
  Clock3,
  LoaderCircle,
  MapPin,
  Plus,
  Repeat2,
  SkipForward,
  Undo2,
} from "lucide-react";
import { Modal } from "./Editor";
import SnoozeMenu from "./Snooze";
import {
  dateTime,
  localDate,
  reminderLabel,
  reminderTime,
  scheduleLabel,
  type CalendarEvent,
  type Snapshot,
  type Task,
  type TaskRecord,
} from "./types";

const weekdays = ["일", "월", "화", "수", "목", "금", "토"];
const labels: Record<CalendarEvent["kind"], string> = {
  pending: "예정",
  projected: "반복 예정",
  completed: "완료",
  skipped: "건너뜀",
};
const time = (iso: string) =>
  new Intl.DateTimeFormat("ko-KR", {
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  }).format(new Date(iso));
const dayTitle = (key: string) =>
  new Intl.DateTimeFormat("ko-KR", {
    year: "numeric",
    month: "long",
    day: "numeric",
    weekday: "long",
  }).format(new Date(`${key}T12:00:00`));

function monthStart(date: Date) {
  return new Date(date.getFullYear(), date.getMonth(), 1);
}
function shiftedMonth(date: Date, offset: number) {
  const first = new Date(date.getFullYear(), date.getMonth() + offset, 1);
  const lastDay = new Date(
    first.getFullYear(),
    first.getMonth() + 1,
    0,
  ).getDate();
  first.setDate(Math.min(date.getDate(), lastDay));
  return first;
}

export default function TaskCalendar({
  data,
  now,
  busy,
  onAdd,
  onEdit,
  onFinish,
  onSnooze,
  onReopen,
}: {
  data: Snapshot;
  now: Date;
  busy: boolean;
  onAdd: (dueAt: string) => void;
  onEdit: (task: Task) => void;
  onFinish: (task: Task, outcome: "completed" | "skipped") => Promise<void>;
  onSnooze: (task: Task, dueAt: string) => Promise<void>;
  onReopen: (record: TaskRecord) => Promise<void>;
}) {
  const [month, setMonth] = useState(() => monthStart(now));
  const [selectedDay, setSelectedDay] = useState(() => localDate(now));
  const [drawerDay, setDrawerDay] = useState<string | null>(null);
  const [selection, setSelection] = useState<{
    id?: string;
    taskId: string;
    at: number;
  } | null>(null);
  const [events, setEvents] = useState<CalendarEvent[]>([]);
  const [loading, setLoading] = useState(true);
  const [error, setError] = useState("");
  const [retry, setRetry] = useState(0);
  const grid = useRef<HTMLTableElement>(null);
  const pendingFocus = useRef<string | null>(null);
  const monthKey = localDate(month).slice(0, 7);
  const today = localDate(now);
  const { days, start, end, weeks } = useMemo(() => {
    const first = monthStart(month);
    const count = new Date(
      first.getFullYear(),
      first.getMonth() + 1,
      0,
    ).getDate();
    const weeks = Math.max(5, Math.ceil((first.getDay() + count) / 7));
    const beginning = new Date(first);
    beginning.setDate(1 - first.getDay());
    const days = Array.from({ length: weeks * 7 }, (_, index) => {
      const day = new Date(beginning);
      day.setDate(day.getDate() + index);
      return day;
    });
    const after = new Date(beginning);
    after.setDate(after.getDate() + days.length);
    return {
      days,
      start: beginning.toISOString(),
      end: after.toISOString(),
      weeks,
    };
  }, [month]);

  useEffect(() => {
    let current = true;
    setLoading(true);
    setError("");
    setEvents([]);
    invoke<CalendarEvent[]>("calendar_events", { start, end })
      .then((result) => {
        if (current) setEvents(result);
      })
      .catch((cause) => {
        if (current) setError(String(cause));
      })
      .finally(() => {
        if (current) setLoading(false);
      });
    return () => {
      current = false;
    };
  }, [data, start, end, retry]);

  useEffect(() => {
    if (pendingFocus.current) {
      grid.current
        ?.querySelector<HTMLButtonElement>(
          `button[data-date="${pendingFocus.current}"]`,
        )
        ?.focus();
      pendingFocus.current = null;
    }
  }, [month, selectedDay]);

  const byDay = useMemo(() => {
    const map = new Map<string, CalendarEvent[]>();
    for (const event of events) {
      const key = localDate(new Date(event.scheduledFor));
      const group = map.get(key) ?? [];
      group.push(event);
      map.set(key, group);
    }
    return map;
  }, [events]);
  const monthEvents = events.filter((event) =>
    localDate(new Date(event.scheduledFor)).startsWith(monthKey),
  );
  const scheduled = monthEvents.filter(
    (event) => event.kind === "pending" || event.kind === "projected",
  ).length;
  const completed = monthEvents.filter(
    (event) => event.kind === "completed",
  ).length;
  const dayEvents = drawerDay ? (byDay.get(drawerDay) ?? []) : [];
  const selected =
    dayEvents.find((event) => event.id === selection?.id) ??
    dayEvents.find(
      (event) =>
        event.taskId === selection?.taskId &&
        Date.parse(event.scheduledFor) === selection.at,
    ) ??
    dayEvents[0];
  const task = selected
    ? data.tasks.find((task) => task.id === selected.taskId)
    : undefined;
  const record = selected?.recordId
    ? data.records.find((record) => record.id === selected.recordId)
    : undefined;
  const latestRecord = record
    ? [...data.records]
        .reverse()
        .find((other) => other.taskId === record.taskId)
    : undefined;
  const canFinish =
    selected?.kind === "pending" &&
    task?.active &&
    Date.parse(task.dueAt) === Date.parse(selected.scheduledFor);
  const advance =
    selected && task
      ? reminderTime(selected.scheduledFor, task.reminderMinutes)
      : null;
  const isOverdue = (event: CalendarEvent) =>
    event.kind === "pending" && Date.parse(event.scheduledFor) < now.getTime();

  function navigateMonth(offset: number) {
    const next = shiftedMonth(month, offset);
    if (next.getFullYear() < 1 || next.getFullYear() > 9999) return;
    setMonth(monthStart(next));
    setSelectedDay(localDate(next));
    setSelection(null);
  }
  function openDay(key: string, event?: CalendarEvent) {
    setSelectedDay(key);
    setDrawerDay(key);
    setSelection(
      event
        ? {
            id: event.id,
            taskId: event.taskId,
            at: Date.parse(event.scheduledFor),
          }
        : null,
    );
  }
  function addTask(key: string) {
    setDrawerDay(null);
    const at = new Date(`${key}T09:00:00`);
    if (key === today && at < now) {
      at.setTime(
        Math.min(
          now.getTime() + 30 * 60_000,
          new Date(`${key}T23:59:00`).getTime(),
        ),
      );
      at.setSeconds(0, 0);
    }
    onAdd(at.toISOString());
  }
  function focusDay(event: KeyboardEvent<HTMLButtonElement>, day: Date) {
    const next = new Date(day);
    switch (event.key) {
      case "ArrowLeft":
        next.setDate(day.getDate() - 1);
        break;
      case "ArrowRight":
        next.setDate(day.getDate() + 1);
        break;
      case "ArrowUp":
        next.setDate(day.getDate() - 7);
        break;
      case "ArrowDown":
        next.setDate(day.getDate() + 7);
        break;
      case "Home":
        next.setDate(day.getDate() - day.getDay());
        break;
      case "End":
        next.setDate(day.getDate() + 6 - day.getDay());
        break;
      case "PageUp":
        next.setTime(shiftedMonth(day, -1).getTime());
        break;
      case "PageDown":
        next.setTime(shiftedMonth(day, 1).getTime());
        break;
      default:
        return;
    }
    event.preventDefault();
    if (next.getFullYear() < 1 || next.getFullYear() > 9999) return;
    const key = localDate(next);
    pendingFocus.current = key;
    setSelectedDay(key);
    if (
      event.key === "PageUp" ||
      event.key === "PageDown" ||
      next < days[0] ||
      next > days[days.length - 1]
    )
      setMonth(monthStart(next));
  }

  return (
    <section className="calendar-page" aria-label="할 일 달력">
      <div className="page-heading calendar-heading">
        <div>
          <div className="eyebrow">
            <span className="eyebrow-index" aria-hidden="true">
              02
            </span>
            <span>CALENDAR</span>
          </div>
          <h1>
            {month.getFullYear()}년 {month.getMonth() + 1}월
          </h1>
          <p>예정된 일과 돌본 흔적을 한눈에 확인하세요.</p>
        </div>
        <div className="calendar-controls">
          <div
            className="calendar-navigation"
            role="group"
            aria-label="달력 월 이동"
          >
            <button
              className="icon-button"
              aria-label="이전 달"
              onClick={() => navigateMonth(-1)}
            >
              <ChevronLeft size={20} />
            </button>
            <button
              className="calendar-today"
              onClick={() => {
                setMonth(monthStart(now));
                setSelectedDay(today);
              }}
            >
              오늘
            </button>
            <button
              className="icon-button"
              aria-label="다음 달"
              onClick={() => navigateMonth(1)}
            >
              <ChevronRight size={20} />
            </button>
          </div>
          <button
            className="button primary"
            disabled={busy}
            onClick={() => addTask(selectedDay)}
          >
            <Plus size={16} />할 일 추가
          </button>
        </div>
      </div>
      <div className="calendar-summary" aria-live="polite">
        {loading ? (
          <span>
            <LoaderCircle size={14} className="spin" /> 일정을 불러오고 있어요.
          </span>
        ) : error ? (
          <div className="calendar-error" role="alert">
            <span>{error}</span>
            <button
              className="text-button"
              onClick={() => setRetry((value) => value + 1)}
            >
              다시 시도
            </button>
          </div>
        ) : (
          <span>
            이달의 일정 <strong>{scheduled}</strong> · 완료{" "}
            <strong>{completed}</strong>
          </span>
        )}
        <div className="calendar-legend" aria-label="일정 상태 범례">
          <span>
            <i className="event-dot pending" />
            예정
          </span>
          <span>
            <i className="event-dot projected" />
            반복 예정
          </span>
          <span>
            <Check size={13} />
            완료
          </span>
          <span>
            <SkipForward size={13} />
            건너뜀
          </span>
        </div>
      </div>
      <table
        ref={grid}
        className="month-calendar"
        style={{ "--calendar-weeks": weeks } as CSSProperties}
        role="grid"
        aria-label={`${month.getFullYear()}년 ${month.getMonth() + 1}월 할 일 달력`}
        aria-busy={loading}
      >
        <thead>
          <tr>
            {weekdays.map((weekday, index) => (
              <th
                scope="col"
                key={weekday}
                className={
                  index === 0 ? "sunday" : index === 6 ? "saturday" : ""
                }
              >
                {weekday}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {Array.from({ length: weeks }, (_, week) => (
            <tr key={week}>
              {days.slice(week * 7, week * 7 + 7).map((day) => {
                const key = localDate(day);
                const daily = byDay.get(key) ?? [];
                const inMonth = key.startsWith(monthKey);
                return (
                  <td
                    key={key}
                    role="gridcell"
                    aria-selected={selectedDay === key}
                    className={`${!inMonth ? "outside-month" : ""} ${selectedDay === key ? "selected-day" : ""}`}
                    onClick={(event) => {
                      if (event.target === event.currentTarget) openDay(key);
                    }}
                  >
                    <button
                      className="calendar-date"
                      data-date={key}
                      tabIndex={selectedDay === key ? 0 : -1}
                      aria-current={key === today ? "date" : undefined}
                      aria-label={`${dayTitle(key)} 일정 ${daily.length}개`}
                      onClick={() => openDay(key)}
                      onKeyDown={(event) => focusDay(event, day)}
                    >
                      <span
                        className={`calendar-date-number ${key === today ? "is-today" : ""} ${day.getDay() === 0 ? "sunday" : ""}`}
                      >
                        {day.getDate()}
                      </span>
                      {key === today && (
                        <span className="calendar-today-label">오늘</span>
                      )}
                    </button>
                    <div className="calendar-cell-events">
                      {daily.slice(0, 2).map((event) => (
                        <button
                          key={event.id}
                          tabIndex={-1}
                          className={`calendar-event ${event.kind} ${isOverdue(event) ? "overdue" : ""}`}
                          title={`${time(event.scheduledFor)} ${event.title} · ${isOverdue(event) ? "기한 지남" : labels[event.kind]} · ${event.target}`}
                          aria-label={`${time(event.scheduledFor)} ${event.title} ${labels[event.kind]}`}
                          onClick={() => openDay(key, event)}
                        >
                          <span
                            className="calendar-event-mark"
                            aria-hidden="true"
                          >
                            {event.kind === "completed" ? (
                              <Check size={12} />
                            ) : event.kind === "skipped" ? (
                              <SkipForward size={12} />
                            ) : (
                              <i className={`event-dot ${event.kind}`} />
                            )}
                          </span>
                          <time dateTime={event.scheduledFor}>
                            {time(event.scheduledFor)}
                          </time>
                          <span className="calendar-event-title">
                            {event.title}
                          </span>
                        </button>
                      ))}
                      {daily.length > 2 && (
                        <button
                          tabIndex={-1}
                          className="calendar-more"
                          aria-label={`${dayTitle(key)} 일정 ${daily.length - 2}개 더 보기`}
                          onClick={() => openDay(key)}
                        >
                          +{daily.length - 2}개 더 보기
                        </button>
                      )}
                    </div>
                  </td>
                );
              })}
            </tr>
          ))}
        </tbody>
      </table>
      <p className="calendar-help">
        날짜를 선택하면 전체 일정이 열려요. 방향키로 날짜를 이동할 수 있어요.
        일정은 이 컴퓨터의 현지 시각으로 표시해요.
      </p>
      <p className="calendar-help">
        날짜 기준 반복은 미래 회차를 미리 보여줘요. 완료일 기준 반복은 확정된
        다음 회차만 표시해요.
      </p>

      {drawerDay && (
        <Modal
          title={dayTitle(drawerDay)}
          className="calendar-drawer"
          onClose={() => setDrawerDay(null)}
        >
          <div className="calendar-agenda-heading">
            <span>
              전체 일정 <strong>{dayEvents.length}</strong>
            </span>
            <button
              className="text-button"
              disabled={busy}
              onClick={() => addTask(drawerDay)}
            >
              <Plus size={14} />할 일 추가
            </button>
          </div>
          {loading ? (
            <p className="calendar-agenda-empty">
              <LoaderCircle size={20} className="spin" />
              일정을 불러오고 있어요.
            </p>
          ) : error ? (
            <p className="form-error" role="alert">
              {error}
            </p>
          ) : dayEvents.length === 0 ? (
            <div className="calendar-agenda-empty">
              <CalendarDays size={32} />
              <strong>이날 예정된 할 일이 없어요</strong>
              <p>새 할 일을 추가해 일정을 채워 보세요.</p>
            </div>
          ) : (
            <>
              <div
                className="calendar-agenda-list"
                aria-label="선택한 날짜의 일정"
              >
                {dayEvents.map((event) => (
                  <button
                    key={event.id}
                    aria-pressed={selected?.id === event.id}
                    className={`calendar-agenda-item ${selected?.id === event.id ? "active" : ""}`}
                    onClick={() =>
                      setSelection({
                        id: event.id,
                        taskId: event.taskId,
                        at: Date.parse(event.scheduledFor),
                      })
                    }
                  >
                    <time dateTime={event.scheduledFor}>
                      {time(event.scheduledFor)}
                    </time>
                    <span>
                      <strong>{event.title}</strong>
                      <small>{event.target}</small>
                    </span>
                    <span
                      className={`calendar-status ${isOverdue(event) ? "overdue" : event.kind}`}
                    >
                      {isOverdue(event) ? "기한 지남" : labels[event.kind]}
                    </span>
                  </button>
                ))}
              </div>
              {selected && (
                <section
                  className="calendar-detail"
                  aria-label="선택한 일정 상세"
                >
                  <h3>{selected.title}</h3>
                  <p className="calendar-detail-target">
                    <MapPin size={14} />
                    {selected.target}
                  </p>
                  <dl>
                    <dt>예정일</dt>
                    <dd>{dateTime(selected.scheduledFor)}</dd>
                    {selected.completedAt && (
                      <>
                        <dt>
                          {selected.kind === "skipped"
                            ? "건너뛴 시각"
                            : "완료 시각"}
                        </dt>
                        <dd>{dateTime(selected.completedAt)}</dd>
                      </>
                    )}
                    {task && !record && (
                      <>
                        <dt>반복</dt>
                        <dd>{scheduleLabel(task)}</dd>
                        <dt>미리 알림</dt>
                        <dd>
                          {advance ? (
                            <>
                              <Bell size={13} />
                              {dateTime(advance)} ·{" "}
                              {reminderLabel(task.reminderMinutes)}
                            </>
                          ) : (
                            "없음"
                          )}
                        </dd>
                      </>
                    )}
                  </dl>
                  {task?.notes && !record && (
                    <p className="calendar-task-notes">{task.notes}</p>
                  )}
                  {selected.kind === "projected" && task && (
                    <div className="calendar-projection-note">
                      <Repeat2 size={16} />
                      <p>
                        반복 규칙으로 미리 표시한 회차예요. 현재 예정일인{" "}
                        {dateTime(task.dueAt)} 회차를 먼저 처리해 주세요.
                      </p>
                    </div>
                  )}
                  {task?.mode === "after_completion" && !record && (
                    <p className="field-hint">
                      다음 회차는 이 할 일을 완료하거나 건너뛴 시각을 기준으로
                      정해져요.
                    </p>
                  )}
                  <div className="calendar-detail-actions">
                    {canFinish && task && (
                      <>
                        <button
                          className="button primary"
                          disabled={busy || loading}
                          onClick={() => void onFinish(task, "completed")}
                        >
                          <Check size={16} />
                          완료
                        </button>
                        <button
                          className="button secondary"
                          disabled={busy || loading}
                          onClick={() => void onFinish(task, "skipped")}
                        >
                          <SkipForward size={16} />
                          건너뛰기
                        </button>
                        <SnoozeMenu
                          task={task}
                          labeled
                          disabled={busy || loading}
                          onSnooze={(task, dueAt) => void onSnooze(task, dueAt)}
                        />
                      </>
                    )}
                    {record && task && latestRecord?.id === record.id && (
                      <button
                        className="button secondary"
                        disabled={busy || loading}
                        onClick={() => void onReopen(record)}
                      >
                        <Undo2 size={16} />
                        {record.outcome === "completed"
                          ? "완료 취소"
                          : "건너뛰기 취소"}
                      </button>
                    )}
                    {selected.kind === "projected" && task && (
                      <button
                        className="button secondary"
                        disabled={busy || loading}
                        onClick={() => {
                          const key = localDate(new Date(task.dueAt));
                          setMonth(monthStart(new Date(task.dueAt)));
                          setSelectedDay(key);
                          setDrawerDay(key);
                          setSelection({
                            taskId: task.id,
                            at: Date.parse(task.dueAt),
                          });
                        }}
                      >
                        <Clock3 size={16} />
                        현재 회차 보기
                      </button>
                    )}
                    {task && (
                      <button
                        className="button secondary"
                        disabled={busy}
                        onClick={() => {
                          setDrawerDay(null);
                          onEdit(task);
                        }}
                      >
                        {task.mode === "once"
                          ? "할 일 수정"
                          : "반복 할 일 수정"}
                      </button>
                    )}
                  </div>
                  {record && !task && (
                    <p className="field-hint">
                      삭제된 할 일의 수행 기록이에요.
                    </p>
                  )}
                  {record && task && latestRecord?.id !== record.id && (
                    <p className="field-hint">
                      가장 최근 수행 기록부터 취소할 수 있어요.
                    </p>
                  )}
                </section>
              )}
            </>
          )}
        </Modal>
      )}
    </section>
  );
}
