import { useEffect, useId, useRef, useState, type FormEvent } from "react";
import {
  ExternalLink,
  FileText,
  ImagePlus,
  Info,
  Maximize2,
  Trash2,
  X,
} from "lucide-react";
import type { Attachment, Editor, Item, Snapshot, Space, Task } from "./types";
import {
  ATTACHMENT_MAX_BYTES,
  MAX_ATTACHMENTS,
  MAX_PHOTOS,
  dateTime,
  fileSize,
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

const attachmentTypes: Record<string, string> = {
  pdf: "application/pdf",
  jpg: "image/jpeg",
  jpeg: "image/jpeg",
  png: "image/png",
  webp: "image/webp",
};

/** 첨부 파일은 원본 그대로 보관한다. 실제 형식은 저장할 때 다시 확인한다. */
async function readAttachment(file: File): Promise<Attachment> {
  // 일부 환경은 파일 형식을 비워 두므로 확장자로 보완한다.
  const extension = file.name.split(".").pop()?.toLowerCase() ?? "";
  const mime = Object.values(attachmentTypes).includes(file.type)
    ? file.type
    : attachmentTypes[extension];
  if (!mime)
    throw new Error(`${file.name}: PDF·JPEG·PNG·WebP 파일을 선택해 주세요.`);
  if (file.size > ATTACHMENT_MAX_BYTES)
    throw new Error(`${file.name}: 첨부 파일은 10MB 이하로 선택해 주세요.`);
  const url = await new Promise<string>((resolve, reject) => {
    const reader = new FileReader();
    reader.onload = () => resolve(String(reader.result));
    reader.onerror = () => reject(new Error("파일을 읽지 못했습니다."));
    reader.readAsDataURL(file);
  });
  return {
    id: "",
    name: file.name.slice(0, 200),
    mime,
    size: file.size,
    data: `data:${mime};base64,${url.slice(url.indexOf(",") + 1)}`,
  };
}

export default function EntityEditor({
  editor,
  data,
  busy,
  onClose,
  onSave,
  onDelete,
  onOpenAttachment,
}: {
  editor: Editor;
  data: Snapshot;
  busy: boolean;
  onClose: () => void;
  onSave: (kind: Editor["kind"], value: Space | Item | Task) => Promise<void>;
  onDelete: (kind: Editor["kind"], id: string) => void;
  onOpenAttachment: (id: string) => void;
}) {
  const [error, setError] = useState("");
  const [photos, setPhotos] = useState<string[]>(
    editor.kind === "item" ? (editor.value?.photos ?? []) : [],
  );
  const [photoIndex, setPhotoIndex] = useState(0);
  const photo = photos[Math.min(photoIndex, photos.length - 1)] ?? null;
  const [attachments, setAttachments] = useState<Attachment[]>(
    editor.kind === "item" ? (editor.value?.attachments ?? []) : [],
  );
  const [photoBusy, setPhotoBusy] = useState(false);
  const [fileBusy, setFileBusy] = useState(false);
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
          photos,
          attachments,
          warrantyUntil: str("warrantyUntil") || null,
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
                      onClick={() => {
                        const index = photos.indexOf(photo);
                        setPhotos(photos.filter((_, i) => i !== index));
                        setPhotoIndex(Math.max(0, index - 1));
                      }}
                    >
                      <X size={16} />
                    </button>
                  </>
                ) : (
                  <ImagePlus size={28} />
                )}
                {photos.length < MAX_PHOTOS && (
                  <label className="photo-label">
                    {photoBusy ? "사진 처리 중…" : "사진 추가"}
                    <input
                      type="file"
                      accept="image/jpeg,image/png,image/webp"
                      multiple
                      disabled={photoBusy}
                      onChange={async (e) => {
                        const files = [...(e.target.files ?? [])];
                        e.target.value = "";
                        if (!files.length) return;
                        setPhotoBusy(true);
                        setError("");
                        const room = MAX_PHOTOS - photos.length;
                        const added: string[] = [];
                        try {
                          for (const file of files.slice(0, room))
                            added.push(await readPhoto(file));
                          if (files.length > room)
                            setError(
                              `사진은 ${MAX_PHOTOS}장까지 넣을 수 있어요. 처음 ${room}장만 추가했어요.`,
                            );
                        } catch (e) {
                          setError(String(e));
                        } finally {
                          if (added.length) {
                            setPhotos([...photos, ...added]);
                            setPhotoIndex(photos.length);
                          }
                          setPhotoBusy(false);
                        }
                      }}
                    />
                  </label>
                )}
              </div>
              {photos.length > 1 && (
                <div
                  className="photo-strip"
                  role="group"
                  aria-label="아이템 사진 목록"
                >
                  {photos.map((source, index) => (
                    <button
                      // 같은 사진을 두 번 고를 수 있어 순서를 키로 쓴다.
                      key={index}
                      type="button"
                      className={source === photo ? "selected" : ""}
                      aria-pressed={source === photo}
                      aria-label={`사진 ${index + 1}${index === 0 ? " (대표)" : ""}`}
                      onClick={() => setPhotoIndex(index)}
                    >
                      <img src={photoSrc(source)} alt="" />
                      {index === 0 && <span aria-hidden="true">대표</span>}
                    </button>
                  ))}
                  {photos.indexOf(photo!) > 0 && (
                    <button
                      type="button"
                      className="text-button"
                      onClick={() => {
                        setPhotos([
                          photo!,
                          ...photos.filter((p) => p !== photo),
                        ]);
                        setPhotoIndex(0);
                      }}
                    >
                      대표 사진으로
                    </button>
                  )}
                </div>
              )}
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
              <div className="form-row">
                <label>
                  보증 만료일
                  <input
                    name="warrantyUntil"
                    type="date"
                    defaultValue={editor.value?.warrantyUntil ?? ""}
                  />
                </label>
              </div>
              <section className="attachments" aria-label="첨부 파일">
                <div className="attachments-heading">
                  <strong>
                    첨부 파일
                    {attachments.length > 0 &&
                      ` ${attachments.length}/${MAX_ATTACHMENTS}`}
                  </strong>
                  {attachments.length < MAX_ATTACHMENTS && (
                    <label className="photo-label">
                      {fileBusy ? "파일 읽는 중…" : "파일 추가"}
                      <input
                        type="file"
                        accept="application/pdf,image/jpeg,image/png,image/webp"
                        multiple
                        disabled={fileBusy}
                        onChange={async (e) => {
                          const files = [...(e.target.files ?? [])];
                          e.target.value = "";
                          if (!files.length) return;
                          setFileBusy(true);
                          setError("");
                          const room = MAX_ATTACHMENTS - attachments.length;
                          const added: Attachment[] = [];
                          try {
                            for (const file of files.slice(0, room))
                              added.push(await readAttachment(file));
                            if (files.length > room)
                              setError(
                                `첨부 파일은 ${MAX_ATTACHMENTS}개까지 넣을 수 있어요. 처음 ${room}개만 추가했어요.`,
                              );
                          } catch (e) {
                            setError(String(e));
                          } finally {
                            if (added.length)
                              setAttachments([...attachments, ...added]);
                            setFileBusy(false);
                          }
                        }}
                      />
                    </label>
                  )}
                </div>
                {attachments.length ? (
                  <ul>
                    {attachments.map((file, index) => (
                      <li key={file.id || `new-${index}`}>
                        <FileText size={16} />
                        <span className="attachment-name" title={file.name}>
                          {file.name}
                        </span>
                        <small>
                          {fileSize(file.size)}
                          {file.data && " · 저장하면 열 수 있어요"}
                        </small>
                        {!file.data && (
                          <button
                            type="button"
                            className="icon-button"
                            title="기본 앱으로 열기"
                            aria-label={`${file.name} 열기`}
                            onClick={() => onOpenAttachment(file.id)}
                          >
                            <ExternalLink size={15} />
                          </button>
                        )}
                        <button
                          type="button"
                          className="icon-button"
                          title="첨부 파일 삭제"
                          aria-label={`${file.name} 삭제`}
                          onClick={() =>
                            setAttachments(
                              attachments.filter((_, i) => i !== index),
                            )
                          }
                        >
                          <X size={15} />
                        </button>
                      </li>
                    ))}
                  </ul>
                ) : (
                  <p className="field-hint">
                    영수증·보증서·설명서를 함께 보관해요. PDF·JPEG·PNG·WebP
                    형식, 파일당 10MB까지 넣을 수 있어요.
                  </p>
                )}
              </section>
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
          <button
            className="button primary"
            disabled={busy || photoBusy || fileBusy}
          >
            {busy ? "저장 중…" : "저장"}
          </button>
        </div>
      </form>
    </Modal>
  );
}
