import { useEffect, useId, useRef, useState, type FormEvent } from "react";
import { ImagePlus, Info, Maximize2, Trash2, X } from "lucide-react";
import type { Editor, Item, Snapshot, Space, Task } from "./types";
import {
  dateTime,
  localInput,
  photoSrc,
  reminderOptions,
  reminderTime,
  spacePath,
  units,
} from "./types";

export function Modal({
  title,
  children,
  onClose,
  className = "",
}: {
  title: string;
  children: React.ReactNode;
  onClose: () => void;
  className?: string;
}) {
  const ref = useRef<HTMLDialogElement>(null);
  const titleId = useId();
  useEffect(() => {
    const d = ref.current!;
    d.showModal();
    return () => d.close();
  }, []);
  return (
    <dialog
      ref={ref}
      className={`modal ${className}`}
      aria-labelledby={titleId}
      onCancel={(e) => {
        e.preventDefault();
        // React는 cancel 이벤트를 상위 컴포넌트로 전달하므로, 사진 보기처럼
        // 겹쳐 연 창에서 Esc를 눌러도 아래 창까지 닫히지 않게 막는다.
        e.stopPropagation();
        onClose();
      }}
    >
      <div className="modal-heading">
        <h2 id={titleId}>{title}</h2>
        <button
          className="icon-button"
          type="button"
          aria-label="닫기"
          onClick={onClose}
        >
          <X size={20} />
        </button>
      </div>
      {children}
    </dialog>
  );
}

async function readPhoto(file: File): Promise<string> {
  if (!["image/jpeg", "image/png", "image/webp"].includes(file.type))
    throw new Error("JPEG·PNG·WebP 사진을 선택해 주세요.");
  if (file.size > 20 * 1024 * 1024)
    throw new Error("원본 사진은 20MB 이하로 선택해 주세요.");
  const source = URL.createObjectURL(file);
  try {
    const image = new Image();
    image.src = source;
    await image.decode();
    const scale = Math.min(1, 1600 / Math.max(image.width, image.height));
    const canvas = document.createElement("canvas");
    canvas.width = Math.max(1, Math.round(image.width * scale));
    canvas.height = Math.max(1, Math.round(image.height * scale));
    const context = canvas.getContext("2d");
    if (!context) throw new Error("사진을 처리하지 못했습니다.");
    context.fillStyle = "#ffffff";
    context.fillRect(0, 0, canvas.width, canvas.height);
    context.drawImage(image, 0, 0, canvas.width, canvas.height);
    const photo = canvas.toDataURL("image/jpeg", 0.82);
    if (photo.length > 4 * 1024 * 1024)
      throw new Error("사진을 더 작은 크기로 선택해 주세요.");
    return photo;
  } finally {
    URL.revokeObjectURL(source);
  }
}

export default function EntityEditor({
  editor,
  data,
  busy,
  onClose,
  onSave,
  onDelete,
}: {
  editor: Editor;
  data: Snapshot;
  busy: boolean;
  onClose: () => void;
  onSave: (kind: Editor["kind"], value: Space | Item | Task) => Promise<void>;
  onDelete: (kind: Editor["kind"], id: string) => void;
}) {
  const [error, setError] = useState("");
  const [photo, setPhoto] = useState<string | null>(
    editor.kind === "item" ? (editor.value?.photo ?? null) : null,
  );
  const [photoBusy, setPhotoBusy] = useState(false);
  const [viewingPhoto, setViewingPhoto] = useState(false);
  const [mode, setMode] = useState(
    editor.kind === "task" ? (editor.value?.mode ?? "once") : "once",
  );
  const [dueInput, setDueInput] = useState(() =>
    editor.kind === "task"
      ? localInput(
          editor.value?.dueAt ??
            editor.dueAt ??
            new Date(Date.now() + 3600000).toISOString(),
        )
      : "",
  );
  const [reminderMinutes, setReminderMinutes] = useState<number | null>(
    editor.kind === "task" ? (editor.value?.reminderMinutes ?? null) : null,
  );
  const previewReminder = reminderTime(dueInput, reminderMinutes);
  const [targetKind, setTargetKind] = useState(
    editor.kind === "task" && (editor.itemId || editor.value?.itemId)
      ? "item"
      : "space",
  );
  const [icon, setIcon] = useState(
    editor.kind === "space" ? editor.value?.icon || "📦" : "📦",
  );
  const label = { space: "공간", item: "아이템", task: "할 일" }[editor.kind];
  const requiredFor = editor.kind === "space" ? editor.requiredFor : undefined;
  const v = editor.value;
  const orderedSpaces = [...data.spaces].sort((a, b) =>
    spacePath(data.spaces, a.id).localeCompare(
      spacePath(data.spaces, b.id),
      "ko",
    ),
  );
  const forbidden = new Set<string>();
  if (editor.kind === "space" && v) {
    forbidden.add(v.id);
    let changed = true;
    while (changed) {
      changed = false;
      for (const s of data.spaces)
        if (s.parentId && forbidden.has(s.parentId) && !forbidden.has(s.id)) {
          forbidden.add(s.id);
          changed = true;
        }
    }
  }
  async function submit(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    setError("");
    const f = new FormData(event.currentTarget);
    const str = (key: string) => String(f.get(key) ?? "").trim();
    try {
      if (editor.kind === "space") {
        await onSave("space", {
          id: editor.value?.id ?? "",
          parentId: str("parentId") || null,
          name: str("name"),
          icon,
          notes: str("notes"),
        });
      } else if (editor.kind === "item") {
        await onSave("item", {
          id: editor.value?.id ?? "",
          spaceId: str("spaceId"),
          name: str("name"),
          quantity: Number(str("quantity")),
          unit: str("unit"),
          notes: str("notes"),
          photo,
          expiresOn: str("expiresOn") || null,
          purchasedOn: str("purchasedOn") || null,
          purchaseStore: str("purchaseStore"),
          purchasePrice: str("purchasePrice")
            ? Number(str("purchasePrice"))
            : null,
        });
      } else {
        const dueAt =
          editor.value && str("dueAt") === localInput(editor.value.dueAt)
            ? editor.value.dueAt
            : new Date(str("dueAt")).toISOString();
        const every =
          mode === "once"
            ? (editor.value?.every ?? 1)
            : Number(str("every") || "1");
        const unit =
          mode === "once"
            ? (editor.value?.unit ?? "day")
            : ((str("unit") || "day") as Task["unit"]);
        const scheduleChanged =
          !editor.value ||
          Date.parse(dueAt) !== Date.parse(editor.value.dueAt) ||
          mode !== editor.value.mode ||
          every !== editor.value.every ||
          unit !== editor.value.unit;
        await onSave("task", {
          id: editor.value?.id ?? "",
          title: str("title"),
          notes: str("notes"),
          spaceId: targetKind === "space" ? str("targetId") : null,
          itemId: targetKind === "item" ? str("targetId") : null,
          mode,
          every,
          unit,
          dueAt,
          anchorAt: scheduleChanged ? dueAt : editor.value!.anchorAt,
          timezone: scheduleChanged
            ? Intl.DateTimeFormat().resolvedOptions().timeZone
            : editor.value!.timezone,
          active: f.get("active") === "on",
          notifiedDueAt: null,
          reminderMinutes,
          notifiedReminderAt: null,
        });
      }
    } catch (e) {
      setError(String(e));
    }
  }
  return (
    <Modal
      title={`${label} ${v ? "수정" : "추가"}`}
      onClose={() => !busy && onClose()}
    >
      <form onSubmit={submit}>
        <fieldset disabled={busy}>
          {editor.kind === "space" && (
            <>
              {requiredFor && (
                <div className="editor-notice" id="space-required-reason">
                  <Info size={18} />
                  <div>
                    <strong>먼저 공간을 만들어 주세요</strong>
                    <p>
                      {requiredFor === "task"
                        ? "할 일은 공간이나 아이템에 연결해야 저장할 수 있어요. 공간을 만든 뒤 ‘할 일 추가’를 다시 눌러 주세요."
                        : "아이템은 보관할 공간을 정해야 저장할 수 있어요. 공간을 만든 뒤 ‘아이템 추가’를 다시 눌러 주세요."}
                    </p>
                  </div>
                </div>
              )}
              <div
                className="icon-picker"
                role="group"
                aria-label="공간 아이콘"
              >
                {[
                  "🏠",
                  "🏢",
                  "🛋️",
                  "🛏️",
                  "🍳",
                  "🧊",
                  "📚",
                  "🗄️",
                  "📦",
                  "🌿",
                ].map((i) => (
                  <button
                    key={i}
                    type="button"
                    className={icon === i ? "selected" : ""}
                    aria-pressed={icon === i}
                    onClick={() => setIcon(i)}
                  >
                    {i}
                  </button>
                ))}
              </div>
              <label>
                공간 이름
                <input
                  name="name"
                  defaultValue={editor.value?.name}
                  required
                  maxLength={100}
                  placeholder="예: 우리 집, 책장, 냉장칸"
                  aria-describedby={
                    requiredFor ? "space-required-reason" : undefined
                  }
                  autoFocus
                />
              </label>
              <label>
                상위 공간
                <select
                  name="parentId"
                  defaultValue={editor.value?.parentId ?? editor.parentId ?? ""}
                >
                  <option value="">최상위 공간</option>
                  {orderedSpaces
                    .filter((s) => !forbidden.has(s.id))
                    .map((s) => (
                      <option key={s.id} value={s.id}>
                        {spacePath(data.spaces, s.id)}
                      </option>
                    ))}
                </select>
              </label>
            </>
          )}
          {editor.kind === "item" && (
            <>
              <div className={`photo-picker ${photo ? "has-photo" : ""}`}>
                {photo ? (
                  <>
                    <button
                      type="button"
                      className="photo-preview"
                      aria-label="사진 크게 보기"
                      onClick={() => setViewingPhoto(true)}
                    >
                      <img src={photoSrc(photo)} alt="아이템 사진" />
                      <span className="photo-zoom" aria-hidden="true">
                        <Maximize2 size={13} />
                        크게 보기
                      </span>
                    </button>
                    <button
                      type="button"
                      className="icon-button photo-remove"
                      aria-label="사진 삭제"
                      onClick={() => setPhoto(null)}
                    >
                      <X size={16} />
                    </button>
                  </>
                ) : (
                  <ImagePlus size={28} />
                )}
                <label className="photo-label">
                  {photoBusy
                    ? "사진 처리 중…"
                    : photo
                      ? "사진 바꾸기"
                      : "사진 추가"}
                  <input
                    type="file"
                    accept="image/jpeg,image/png,image/webp"
                    disabled={photoBusy}
                    onChange={async (e) => {
                      const file = e.target.files?.[0];
                      if (!file) return;
                      setPhotoBusy(true);
                      setError("");
                      try {
                        setPhoto(await readPhoto(file));
                      } catch (e) {
                        setError(String(e));
                      } finally {
                        setPhotoBusy(false);
                      }
                    }}
                  />
                </label>
              </div>
              {viewingPhoto && photo && (
                <Modal
                  title={editor.value?.name || "아이템 사진"}
                  className="photo-viewer"
                  onClose={() => setViewingPhoto(false)}
                >
                  <img src={photoSrc(photo)} alt="아이템 사진 원본" />
                </Modal>
              )}
              <label>
                아이템 이름
                <input
                  name="name"
                  defaultValue={editor.value?.name}
                  required
                  maxLength={200}
                  placeholder="예: 우유, 공기청정기"
                  autoFocus
                />
              </label>
              <label>
                보관 공간
                <select
                  name="spaceId"
                  defaultValue={
                    editor.value?.spaceId ??
                    editor.spaceId ??
                    data.spaces[0]?.id
                  }
                  required
                >
                  {orderedSpaces.map((s) => (
                    <option key={s.id} value={s.id}>
                      {spacePath(data.spaces, s.id)}
                    </option>
                  ))}
                </select>
              </label>
              <div className="form-row">
                <label>
                  수량
                  <input
                    name="quantity"
                    type="number"
                    min="0"
                    max="1000000000"
                    step="any"
                    required
                    defaultValue={editor.value?.quantity ?? 1}
                  />
                </label>
                <label>
                  단위
                  <input
                    name="unit"
                    maxLength={30}
                    placeholder="개, 권, kg…"
                    defaultValue={editor.value?.unit ?? "개"}
                  />
                </label>
              </div>
              <div className="form-row">
                <label>
                  유통기한
                  <input
                    name="expiresOn"
                    type="date"
                    defaultValue={editor.value?.expiresOn ?? ""}
                  />
                </label>
                <label>
                  구매일
                  <input
                    name="purchasedOn"
                    type="date"
                    defaultValue={editor.value?.purchasedOn ?? ""}
                  />
                </label>
              </div>
              <div className="form-row">
                <label>
                  구매처
                  <input
                    name="purchaseStore"
                    maxLength={500}
                    defaultValue={editor.value?.purchaseStore}
                    placeholder="상점 이름 등"
                  />
                </label>
                <label>
                  구매 금액 (원)
                  <input
                    name="purchasePrice"
                    type="number"
                    min="0"
                    max="1000000000000"
                    step="1"
                    defaultValue={editor.value?.purchasePrice ?? ""}
                  />
                </label>
              </div>
            </>
          )}
          {editor.kind === "task" && (
            <>
              <label>
                할 일 이름
                <input
                  name="title"
                  defaultValue={editor.value?.title}
                  required
                  maxLength={200}
                  placeholder="예: 냉장고 청소, 필터 교체"
                  autoFocus
                />
              </label>
              <div className="form-row target-row">
                <label>
                  연결 대상
                  <select
                    value={targetKind}
                    onChange={(e) => setTargetKind(e.target.value)}
                  >
                    <option value="space">공간</option>
                    <option value="item" disabled={!data.items.length}>
                      아이템
                    </option>
                  </select>
                </label>
                <label>
                  {targetKind === "space" ? "공간 선택" : "아이템 선택"}
                  <select
                    key={targetKind}
                    name="targetId"
                    required
                    defaultValue={
                      targetKind === "space"
                        ? (editor.value?.spaceId ??
                          editor.spaceId ??
                          data.spaces[0]?.id)
                        : (editor.value?.itemId ??
                          editor.itemId ??
                          data.items[0]?.id)
                    }
                  >
                    {targetKind === "space"
                      ? orderedSpaces.map((s) => (
                          <option key={s.id} value={s.id}>
                            {spacePath(data.spaces, s.id)}
                          </option>
                        ))
                      : data.items.map((i) => (
                          <option key={i.id} value={i.id}>
                            {spacePath(data.spaces, i.spaceId)} / {i.name}
                          </option>
                        ))}
                  </select>
                </label>
              </div>
              <label>
                일정 방식
                <select
                  value={mode}
                  onChange={(e) => setMode(e.target.value as Task["mode"])}
                >
                  <option value="once">한 번만 할 일</option>
                  <option value="calendar">날짜 기준 반복</option>
                  <option value="after_completion">완료일 기준 반복</option>
                </select>
              </label>
              <label>
                다음 예정일
                <input
                  name="dueAt"
                  type="datetime-local"
                  required
                  value={dueInput}
                  onChange={(event) => setDueInput(event.target.value)}
                />
                <span className="field-hint">
                  현재 컴퓨터 시간대 ·{" "}
                  {Intl.DateTimeFormat().resolvedOptions().timeZone}
                </span>
              </label>
              <label>
                미리 알림
                <select
                  aria-label="미리 알림"
                  value={reminderMinutes ?? ""}
                  onChange={(event) =>
                    setReminderMinutes(
                      event.target.value === ""
                        ? null
                        : Number(event.target.value),
                    )
                  }
                >
                  {reminderOptions.map((option) => (
                    <option
                      key={option.minutes ?? "none"}
                      value={option.minutes ?? ""}
                    >
                      {option.label}
                    </option>
                  ))}
                </select>
              </label>
              <p className="reminder-preview" aria-live="polite">
                {previewReminder
                  ? `${dateTime(previewReminder)}에 미리 알려드려요. 예정 시각에도 한 번 더 알려드려요.`
                  : "예정 시각에 알려드려요. 미리 알림을 선택하면 그 전에 한 번 더 알려드려요."}
              </p>
              <p className="field-hint reminder-hint">
                {data.settings.notifications
                  ? "앱이 실행 중일 때 알림을 보내요. 메뉴 막대·트레이에 상주하는 동안에도 받을 수 있어요."
                  : "현재 할 일 알림이 꺼져 있어요. 저장 후 설정에서 할 일 알림을 켜 주세요."}
              </p>
              {mode !== "once" && (
                <div className="form-row">
                  <label>
                    {mode === "calendar" ? "매" : "완료 후"}
                    <input
                      name="every"
                      type="number"
                      min="1"
                      max="999"
                      step="1"
                      required
                      defaultValue={editor.value?.every ?? 1}
                    />
                  </label>
                  <label>
                    반복 단위
                    <select
                      name="unit"
                      defaultValue={editor.value?.unit ?? "week"}
                    >
                      {Object.entries(units).map(([key, label]) => (
                        <option value={key} key={key}>
                          {label}
                        </option>
                      ))}
                    </select>
                  </label>
                </div>
              )}
              <p className="field-hint">
                {mode === "calendar"
                  ? "늦게 완료해도 원래 반복 날짜를 유지해요. 밀린 회차는 묶어서 처리해요."
                  : mode === "after_completion"
                    ? "실제로 완료하거나 건너뛴 날부터 다음 일정을 계산해요."
                    : "완료하거나 건너뛰면 일정이 종료돼요."}
              </p>
              <label className="check-label">
                <input
                  name="active"
                  type="checkbox"
                  defaultChecked={editor.value?.active ?? true}
                />
                일정 활성화
              </label>
            </>
          )}
          <label>
            메모
            <textarea
              name="notes"
              rows={3}
              maxLength={10000}
              defaultValue={editor.value?.notes}
              placeholder="기억해 둘 내용을 적어 주세요."
            />
          </label>
        </fieldset>
        {error && (
          <p className="form-error" role="alert">
            {error}
          </p>
        )}
        <div className="modal-footer">
          {v && (
            <button
              type="button"
              className="button danger quiet"
              disabled={busy}
              onClick={() => onDelete(editor.kind, v.id)}
            >
              <Trash2 size={16} />
              삭제
            </button>
          )}
          <div className="spacer" />
          <button
            type="button"
            className="button secondary"
            disabled={busy}
            onClick={onClose}
          >
            취소
          </button>
          <button className="button primary" disabled={busy || photoBusy}>
            {busy ? "저장 중…" : "저장"}
          </button>
        </div>
      </form>
    </Modal>
  );
}
