import { useEffect, useId, useRef, useState } from "react";
import { AlarmClock } from "lucide-react";
import { dateTime, snoozedTime, snoozeOptions, type Task } from "./types";

/** 현재 회차의 예정일만 뒤로 미루는 버튼과 선택 목록. */
export default function SnoozeMenu({
  task,
  disabled,
  labeled = false,
  onSnooze,
}: {
  task: Task;
  disabled: boolean;
  /** 아이콘만 있는 버튼 대신 글자가 있는 버튼으로 보여 준다. */
  labeled?: boolean;
  onSnooze: (task: Task, dueAt: string) => void;
}) {
  const [open, setOpen] = useState(false);
  const root = useRef<HTMLDivElement>(null);
  const trigger = useRef<HTMLButtonElement>(null);
  const menuId = useId();
  // 목록을 여는 순간의 시각으로 계산해, 보여 준 시각 그대로 미룬다.
  const [choices, setChoices] = useState<{ label: string; dueAt: string }[]>(
    [],
  );

  useEffect(() => {
    if (!open) return;
    root.current?.querySelector<HTMLButtonElement>("[role=menuitem]")?.focus();
    const outside = (event: PointerEvent) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    document.addEventListener("pointerdown", outside);
    return () => document.removeEventListener("pointerdown", outside);
  }, [open]);

  function toggle() {
    if (!open) {
      const now = new Date();
      setChoices(
        snoozeOptions.map((option) => ({
          label: option.label,
          dueAt: snoozedTime(task.dueAt, option, now),
        })),
      );
    }
    setOpen(!open);
  }
  function close(refocus: boolean) {
    setOpen(false);
    if (refocus) trigger.current?.focus();
  }
  function move(event: React.KeyboardEvent, offset: number) {
    event.preventDefault();
    const items = [
      ...(root.current?.querySelectorAll<HTMLButtonElement>(
        "[role=menuitem]",
      ) ?? []),
    ];
    const index = items.indexOf(document.activeElement as HTMLButtonElement);
    items[(index + offset + items.length) % items.length]?.focus();
  }

  return (
    <div
      className={`snooze ${open ? "open" : ""}`}
      ref={root}
      onKeyDown={(event) => {
        if (!open) return;
        if (event.key === "Escape") {
          // 달력의 날짜 창 안에서 열렸을 때 창까지 닫히지 않게 한다.
          event.preventDefault();
          event.stopPropagation();
          close(true);
        } else if (event.key === "ArrowDown") move(event, 1);
        else if (event.key === "ArrowUp") move(event, -1);
        else if (event.key === "Tab") setOpen(false);
      }}
    >
      <button
        ref={trigger}
        type="button"
        className={labeled ? "button secondary" : "icon-button snooze-button"}
        disabled={disabled}
        title={labeled ? undefined : "미루기"}
        aria-label={`${task.title} 미루기`}
        aria-haspopup="menu"
        aria-expanded={open}
        aria-controls={open ? menuId : undefined}
        onClick={toggle}
      >
        <AlarmClock size={labeled ? 16 : 17} />
        {labeled && "미루기"}
      </button>
      {open && (
        <div
          className="snooze-menu"
          id={menuId}
          role="menu"
          aria-label={`${task.title} 미루기`}
        >
          {choices.map((choice) => (
            <button
              key={choice.label}
              type="button"
              role="menuitem"
              onClick={() => {
                close(true);
                onSnooze(task, choice.dueAt);
              }}
            >
              <strong>{choice.label}</strong>
              <span>{dateTime(choice.dueAt)}</span>
            </button>
          ))}
        </div>
      )}
    </div>
  );
}
