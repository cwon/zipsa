use std::{
    collections::{HashMap, HashSet},
    fs,
    path::Path,
};

use base64::{engine::general_purpose::STANDARD, Engine};
use chrono::{DateTime, Duration, Local, NaiveDate, Utc};
use rusqlite::{params, Connection, OptionalExtension};
use serde::{de::DeserializeOwned, Serialize};
use uuid::Uuid;

use crate::{
    models::*,
    schedule::{next_due, parse_time},
};

type Result<T> = std::result::Result<T, String>;

/// v2 moved item photos out of the item JSON into their own table, so saving
/// or listing data never reads or sends photo bytes. v3 lets an item have
/// several photos and attached documents, all stored in the `files` table.
pub const SCHEMA_VERSION: u32 = 3;
/// Marks SQLite files written by Zipsa ("ZIPS") so a restore can reject other databases.
const APPLICATION_ID: i32 = 0x5A49_5053;
pub const MAX_PHOTOS: usize = 10;
pub const MAX_ATTACHMENTS: usize = 10;

/// What a stored file is used for, which decides the types and size it may have.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FileKind {
    Photo,
    Attachment,
}

impl FileKind {
    fn types(self) -> &'static [&'static str] {
        match self {
            Self::Photo => &["image/jpeg", "image/png", "image/webp"],
            Self::Attachment => &["application/pdf", "image/jpeg", "image/png", "image/webp"],
        }
    }
    fn max_bytes(self) -> usize {
        match self {
            Self::Photo => 3 * 1024 * 1024,
            Self::Attachment => 10 * 1024 * 1024,
        }
    }
    fn error(self) -> &'static str {
        match self {
            Self::Photo => "사진은 3MB 이하의 JPEG·PNG·WebP 형식이어야 합니다.",
            Self::Attachment => "첨부 파일은 10MB 이하의 PDF·JPEG·PNG·WebP 형식이어야 합니다.",
        }
    }
}
const INVALID_BACKUP: &str =
    "집사 백업 파일을 읽지 못했습니다. 올바른 .zipsa 파일인지 확인해 주세요.";
const MISSING_FILE: &str = "백업에서 일부 사진이나 첨부 파일을 찾을 수 없습니다.";

const SCHEMA: &str = "
    CREATE TABLE IF NOT EXISTS spaces (id TEXT PRIMARY KEY, parent_id TEXT REFERENCES spaces(id) DEFERRABLE INITIALLY DEFERRED, data TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS items (id TEXT PRIMARY KEY, space_id TEXT NOT NULL REFERENCES spaces(id), data TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS tasks (id TEXT PRIMARY KEY, space_id TEXT REFERENCES spaces(id), item_id TEXT REFERENCES items(id), data TEXT NOT NULL, CHECK ((space_id IS NULL) != (item_id IS NULL)));
    CREATE TABLE IF NOT EXISTS records (id TEXT PRIMARY KEY, data TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS metadata (key TEXT PRIMARY KEY, data TEXT NOT NULL);
    CREATE TABLE IF NOT EXISTS files (id TEXT PRIMARY KEY, item_id TEXT NOT NULL REFERENCES items(id) ON DELETE CASCADE, mime TEXT NOT NULL, data BLOB NOT NULL);
    CREATE INDEX IF NOT EXISTS files_item ON files(item_id);
    CREATE INDEX IF NOT EXISTS spaces_parent ON spaces(parent_id);
    CREATE INDEX IF NOT EXISTS items_space ON items(space_id);
    CREATE INDEX IF NOT EXISTS tasks_space ON tasks(space_id);
    CREATE INDEX IF NOT EXISTS tasks_item ON tasks(item_id);";

pub struct Store {
    pub conn: Connection,
}

/// The bytes of a photo or attachment, kept in the `files` table.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StoredFile {
    pub mime: String,
    pub data: Vec<u8>,
}

impl StoredFile {
    /// Decodes a file picked in the editor (`data:<type>;base64,...`).
    pub fn from_data_url(url: &str, kind: FileKind) -> Result<Self> {
        let (mime, encoded) = url
            .strip_prefix("data:")
            .and_then(|rest| rest.split_once(";base64,"))
            .ok_or(kind.error())?;
        if encoded.len() > kind.max_bytes().div_ceil(3) * 4 {
            return Err(kind.error().into());
        }
        let file = Self {
            mime: mime.into(),
            data: STANDARD.decode(encoded).map_err(|_| kind.error())?,
        };
        file.check(kind)?;
        Ok(file)
    }

    /// The type is confirmed from the content, not from what the sender claims.
    fn check(&self, kind: FileKind) -> Result<()> {
        let d = &self.data;
        let signature = match self.mime.as_str() {
            "image/jpeg" => d.starts_with(&[0xFF, 0xD8, 0xFF]),
            "image/png" => d.starts_with(b"\x89PNG\r\n\x1a\n"),
            "image/webp" => d.len() >= 12 && d.starts_with(b"RIFF") && &d[8..12] == b"WEBP",
            "application/pdf" => d.starts_with(b"%PDF-"),
            _ => false,
        };
        if !signature || !kind.types().contains(&self.mime.as_str()) || d.len() > kind.max_bytes() {
            return Err(kind.error().into());
        }
        Ok(())
    }
}

fn err(e: impl ToString) -> String {
    e.to_string()
}

fn user_version(conn: &Connection) -> Result<u32> {
    conn.pragma_query_value(None, "user_version", |r| r.get(0))
        .map_err(err)
}

fn insert_file(conn: &Connection, id: &str, item_id: &str, file: &StoredFile) -> Result<()> {
    conn.execute(
        "INSERT INTO files (id,item_id,mime,data) VALUES (?1,?2,?3,?4)",
        params![id, item_id, file.mime, file.data],
    )
    .map(|_| ())
    .map_err(err)
}

/// Counts user edits so automatic backups can skip unchanged data.
/// Notification bookkeeping is not an edit and does not call this.
fn touch(conn: &Connection) -> Result<()> {
    conn.execute(
        "INSERT INTO metadata (key,data) VALUES ('revision','1') ON CONFLICT(key) DO UPDATE SET data = CAST(data AS INTEGER) + 1",
        [],
    )
    .map(|_| ())
    .map_err(err)
}

/// Brings item photos from an older schema into `files` and `Item::photos`.
/// v1 stored one photo as a data URL inside the item JSON. v2 stored one
/// photo per item in the `photos` table and its id in the item JSON.
fn upgrade_photos(conn: &Connection, version: u32) -> Result<()> {
    if version == 2 {
        conn.execute_batch(
            "INSERT INTO files (id,item_id,mime,data) SELECT id,item_id,mime,data FROM photos; DROP TABLE photos;",
        )
        .map_err(err)?;
    }
    let items: Vec<(String, String)> = {
        let mut stmt = conn.prepare("SELECT id, data FROM items").map_err(err)?;
        let rows = stmt
            .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))
            .map_err(err)?;
        rows.collect::<rusqlite::Result<_>>().map_err(err)?
    };
    for (id, data) in items {
        let mut item: Item = serde_json::from_str(&data).map_err(err)?;
        let Some(photo) = item.photo.take() else {
            continue;
        };
        if version == 2 {
            item.photos = vec![photo];
        } else if let Ok(file) = StoredFile::from_data_url(&photo, FileKind::Photo) {
            // A photo that would fail validation is dropped instead of blocking
            // the upgrade. The pre-upgrade backup still contains it.
            let photo_id = Uuid::new_v4().to_string();
            insert_file(conn, &photo_id, &id, &file)?;
            item.photos = vec![photo_id];
        }
        conn.execute(
            "UPDATE items SET data=?2 WHERE id=?1",
            params![id, json(&item)?],
        )
        .map_err(err)?;
    }
    Ok(())
}

/// Writes a consistent copy of the whole database, photos included.
fn backup_connection(conn: &Connection, dest: &Path) -> Result<()> {
    let fail = |e: String| format!("백업 파일을 만들지 못했습니다. {e}");
    let parent = dest.parent().ok_or("백업 경로가 올바르지 않습니다.")?;
    // VACUUM INTO needs a file that does not exist yet. Renaming the finished
    // copy means a crash never leaves a partial file under the final name.
    let staging = tempfile::Builder::new()
        .prefix(".zipsa-backup-")
        .tempdir_in(parent)
        .map_err(|e| fail(e.to_string()))?;
    let file = staging.path().join("backup.zipsa");
    let target = file
        .to_str()
        .ok_or("백업 경로에 사용할 수 없는 문자가 있습니다.")?;
    conn.execute("VACUUM INTO ?1", [target])
        .map_err(|e| fail(e.to_string()))?;
    fs::OpenOptions::new()
        .write(true)
        .open(&file)
        .and_then(|f| f.sync_all())
        .and_then(|_| fs::rename(&file, dest))
        .map_err(|e| fail(e.to_string()))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReminderKind {
    Advance,
    Due,
}

#[derive(Debug, Clone)]
pub struct Reminder {
    pub task: Task,
    pub kind: ReminderKind,
}

fn advance_time(task: &Task) -> Option<DateTime<Utc>> {
    parse_time(&task.due_at)
        .ok()?
        .checked_sub_signed(Duration::minutes(task.reminder_minutes? as i64))
}

fn text(value: &str, label: &str, required: bool, max: usize) -> Result<()> {
    if (required && value.trim().is_empty()) || value.chars().count() > max {
        return Err(format!("{label}을(를) {}자 이내로 입력해 주세요.", max));
    }
    Ok(())
}

fn date(value: &Option<String>) -> Result<()> {
    if let Some(v) = value {
        if NaiveDate::parse_from_str(v, "%Y-%m-%d").is_err() || v.len() != 10 {
            return Err("날짜 형식이 올바르지 않습니다.".into());
        }
    }
    Ok(())
}

fn unique_ids<'a>(ids: impl Iterator<Item = &'a String>) -> Result<()> {
    let mut seen = HashSet::new();
    for id in ids {
        if id.is_empty() || id.len() > 100 || !seen.insert(id) {
            return Err("데이터에 비어 있거나 중복된 식별자가 있습니다.".into());
        }
    }
    Ok(())
}

pub fn validate(data: &Snapshot) -> Result<()> {
    unique_ids(data.spaces.iter().map(|s| &s.id))?;
    unique_ids(data.items.iter().map(|s| &s.id))?;
    unique_ids(data.tasks.iter().map(|s| &s.id))?;
    unique_ids(data.records.iter().map(|s| &s.id))?;
    let spaces: HashMap<_, _> = data.spaces.iter().map(|s| (s.id.as_str(), s)).collect();
    let items: HashSet<_> = data.items.iter().map(|s| s.id.as_str()).collect();
    let mut files = HashSet::new();
    let mut file_id = |id: &str| {
        !id.is_empty() && id.len() <= 100 && !id.starts_with("data:") && files.insert(id.to_owned())
    };
    for s in &data.spaces {
        text(&s.name, "공간 이름", true, 100)?;
        text(&s.notes, "메모", false, 10_000)?;
        text(&s.icon, "아이콘", false, 20)?;
        let mut seen = HashSet::new();
        let mut current = Some(s.id.as_str());
        while let Some(id) = current {
            if !seen.insert(id) {
                return Err("자기 자신이나 하위 공간 안으로 이동할 수 없습니다.".into());
            }
            let node = spaces.get(id).ok_or("상위 공간을 찾을 수 없습니다.")?;
            current = node.parent_id.as_deref();
        }
    }
    for i in &data.items {
        if !spaces.contains_key(i.space_id.as_str()) {
            return Err("아이템의 공간을 찾을 수 없습니다.".into());
        }
        text(&i.name, "아이템 이름", true, 200)?;
        text(&i.notes, "메모", false, 10_000)?;
        text(&i.unit, "단위", false, 30)?;
        text(&i.purchase_store, "구매처", false, 500)?;
        if !i.quantity.is_finite() || i.quantity < 0.0 || i.quantity > 1_000_000_000.0 {
            return Err("수량은 0 이상 10억 이하로 입력해 주세요.".into());
        }
        if i.purchase_price
            .is_some_and(|p| !(0..=1_000_000_000_000).contains(&p))
        {
            return Err("구매 금액은 0 이상 1조 원 이하로 입력해 주세요.".into());
        }
        date(&i.expires_on)?;
        date(&i.purchased_on)?;
        date(&i.warranty_until)?;
        // Stored items refer to rows in `files`; the bytes are checked when written.
        if i.photo.is_some()
            || i.photos.len() > MAX_PHOTOS
            || !i.photos.iter().all(|id| file_id(id))
        {
            return Err(format!(
                "아이템 사진 정보가 올바르지 않습니다. 사진은 {MAX_PHOTOS}장까지 넣을 수 있어요."
            ));
        }
        if i.attachments.len() > MAX_ATTACHMENTS {
            return Err(format!(
                "첨부 파일은 아이템마다 {MAX_ATTACHMENTS}개까지 넣을 수 있어요."
            ));
        }
        for a in &i.attachments {
            text(&a.name, "첨부 파일 이름", true, 200)?;
            if a.data.is_some()
                || !FileKind::Attachment.types().contains(&a.mime.as_str())
                || !file_id(&a.id)
            {
                return Err("첨부 파일 정보가 올바르지 않습니다.".into());
            }
        }
    }
    for t in &data.tasks {
        text(&t.title, "할 일 이름", true, 200)?;
        text(&t.notes, "메모", false, 10_000)?;
        let valid_target = match (&t.space_id, &t.item_id) {
            (Some(s), None) => spaces.contains_key(s.as_str()),
            (None, Some(i)) => items.contains(i.as_str()),
            _ => false,
        };
        if !valid_target {
            return Err("할 일에 공간 또는 아이템 하나를 연결해 주세요.".into());
        }
        if !["once", "calendar", "after_completion"].contains(&t.mode.as_str())
            || !["day", "week", "month", "year"].contains(&t.unit.as_str())
            || t.every == 0
            || t.every > 999
        {
            return Err("반복 규칙이 올바르지 않습니다.".into());
        }
        parse_time(&t.due_at)?;
        parse_time(&t.anchor_at)?;
        if t.timezone.parse::<chrono_tz::Tz>().is_err() {
            return Err("지원하지 않는 시간대입니다.".into());
        }
        if let Some(d) = &t.notified_due_at {
            parse_time(d)?;
        }
        if t.reminder_minutes
            .is_some_and(|minutes| ![10, 60, 1440, 10080].contains(&minutes))
        {
            return Err("미리 알림은 10분·1시간·하루·일주일 전 중에서 선택해 주세요.".into());
        }
        if let Some(d) = &t.notified_reminder_at {
            parse_time(d)?;
        }
        if t.reminder_minutes.is_some() && advance_time(t).is_none() {
            return Err("미리 알림 시각이 지원하는 날짜 범위를 벗어났습니다.".into());
        }
    }
    for r in &data.records {
        text(&r.title, "기록 이름", true, 200)?;
        text(&r.target, "기록 위치", false, 50_000)?;
        parse_time(&r.scheduled_for)?;
        parse_time(&r.completed_at)?;
        if !["completed", "skipped"].contains(&r.outcome.as_str()) {
            return Err("수행 기록이 올바르지 않습니다.".into());
        }
    }
    Ok(())
}

fn json<T: Serialize>(v: &T) -> Result<String> {
    serde_json::to_string(v).map_err(|e| e.to_string())
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).map_err(err)?;
        let version = user_version(&conn)?;
        if (1..SCHEMA_VERSION).contains(&version) {
            // Keep the data restorable as it was before converting it.
            let dir = path
                .parent()
                .ok_or("데이터 경로가 올바르지 않습니다.")?
                .join("backups");
            fs::create_dir_all(&dir).map_err(err)?;
            let name = format!(
                "before-upgrade-v{version}-{}.zipsa",
                Local::now().format("%Y%m%d-%H%M%S")
            );
            backup_connection(&conn, &dir.join(name))?;
        }
        Self::from_connection(conn)
    }

    pub fn from_connection(conn: Connection) -> Result<Self> {
        let version = user_version(&conn)?;
        if version > SCHEMA_VERSION {
            return Err("더 최신 버전의 집사로 만든 데이터입니다. 앱을 업데이트해 주세요.".into());
        }
        conn.execute_batch(
            "PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000;",
        )
        .map_err(err)?;
        if version < SCHEMA_VERSION {
            let tx = conn.unchecked_transaction().map_err(err)?;
            tx.execute_batch(SCHEMA).map_err(err)?;
            if version > 0 {
                upgrade_photos(&tx, version)?;
            }
            tx.execute_batch(&format!(
                "PRAGMA application_id = {APPLICATION_ID}; PRAGMA user_version = {SCHEMA_VERSION};"
            ))
            .map_err(err)?;
            tx.commit().map_err(err)?;
        }
        Ok(Self { conn })
    }

    /// Opens a private copy of a `.zipsa` database backup and upgrades it to
    /// the current schema. The copy must not be the user's chosen file.
    pub fn open_backup_copy(path: &Path) -> Result<Self> {
        let conn = Connection::open(path).map_err(|_| INVALID_BACKUP)?;
        // The file comes from outside the app, so its schema may not call
        // functions with side effects.
        conn.pragma_update(None, "trusted_schema", false)
            .map_err(err)?;
        let check: String = conn
            .query_row("PRAGMA quick_check", [], |r| r.get(0))
            .map_err(|_| INVALID_BACKUP)?;
        let app_id: i32 = conn
            .pragma_query_value(None, "application_id", |r| r.get(0))
            .map_err(|_| INVALID_BACKUP)?;
        let version = user_version(&conn).map_err(|_| INVALID_BACKUP)?;
        let tables: i64 = conn
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE type = 'table' AND name IN ('spaces','items','tasks','records','metadata')",
                [],
                |r| r.get(0),
            )
            .map_err(|_| INVALID_BACKUP)?;
        // v1 files predate the application id.
        let zipsa = tables == 5 && (app_id == APPLICATION_ID || (app_id == 0 && version == 1));
        if check != "ok" || !zipsa {
            return Err(INVALID_BACKUP.into());
        }
        Self::from_connection(conn)
    }

    /// Loads a JSON backup from v0.2.0 or earlier, whose photos are data URLs.
    pub fn from_legacy(mut data: Snapshot) -> Result<Self> {
        let mut photos = HashMap::new();
        for item in &mut data.items {
            if let Some(url) = item.photo.take() {
                let id = Uuid::new_v4().to_string();
                photos.insert(
                    id.clone(),
                    StoredFile::from_data_url(&url, FileKind::Photo)?,
                );
                item.photos = vec![id];
            }
        }
        let mut store = Self::from_connection(Connection::open_in_memory().map_err(err)?)?;
        store.replace_all(&data, |id| photos.remove(id).ok_or(MISSING_FILE.into()))?;
        Ok(store)
    }

    pub fn backup_to(&self, dest: &Path) -> Result<()> {
        backup_connection(&self.conn, dest)
    }

    pub fn file(&self, id: &str) -> Result<Option<StoredFile>> {
        self.conn
            .query_row("SELECT mime, data FROM files WHERE id = ?1", [id], |r| {
                Ok(StoredFile {
                    mime: r.get(0)?,
                    data: r.get(1)?,
                })
            })
            .optional()
            .map_err(err)
    }

    pub fn revision(&self) -> Result<i64> {
        self.conn
            .query_row(
                "SELECT CAST(data AS INTEGER) FROM metadata WHERE key = 'revision'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map(Option::unwrap_or_default)
            .map_err(err)
    }

    pub fn auto_backup_revision(&self) -> Result<Option<i64>> {
        self.conn
            .query_row(
                "SELECT CAST(data AS INTEGER) FROM metadata WHERE key = 'auto_backup_revision'",
                [],
                |r| r.get(0),
            )
            .optional()
            .map_err(err)
    }

    pub fn set_auto_backup_revision(&mut self, revision: i64) -> Result<()> {
        self.conn
            .execute(
                "INSERT INTO metadata (key,data) VALUES ('auto_backup_revision',?1) ON CONFLICT(key) DO UPDATE SET data=excluded.data",
                [revision.to_string()],
            )
            .map(|_| ())
            .map_err(err)
    }

    /// True when there is nothing worth backing up.
    pub fn is_empty(&self) -> Result<bool> {
        self.conn
            .query_row(
                "SELECT NOT EXISTS (SELECT 1 FROM spaces) AND NOT EXISTS (SELECT 1 FROM records)",
                [],
                |r| r.get(0),
            )
            .map_err(err)
    }

    fn rows<T: DeserializeOwned>(&self, table: &str) -> Result<Vec<T>> {
        // The table name only comes from the fixed calls in snapshot().
        let mut stmt = self
            .conn
            .prepare(&format!("SELECT data FROM {table} ORDER BY rowid"))
            .map_err(|e| e.to_string())?;
        let rows = stmt
            .query_map([], |row| row.get::<_, String>(0))
            .map_err(|e| e.to_string())?;
        rows.map(|r| {
            serde_json::from_str(&r.map_err(|e| e.to_string())?).map_err(|e| e.to_string())
        })
        .collect()
    }

    fn settings(&self) -> Result<Settings> {
        let settings = self.conn.query_row(
            "SELECT data FROM metadata WHERE key = 'settings'",
            [],
            |row| row.get::<_, String>(0),
        );
        match settings {
            Ok(s) => serde_json::from_str(&s).map_err(|e| e.to_string()),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(Settings::default()),
            Err(e) => Err(e.to_string()),
        }
    }

    pub fn reminders(&self) -> Result<Vec<Reminder>> {
        self.reminders_at(Utc::now())
    }

    pub fn reminders_at(&self, now: DateTime<Utc>) -> Result<Vec<Reminder>> {
        if !self.settings()?.notifications {
            return Ok(vec![]);
        }
        let tasks: Vec<Task> = self.rows("tasks")?;
        Ok(tasks
            .into_iter()
            .filter_map(|task| {
                if !task.active {
                    return None;
                }
                let due = parse_time(&task.due_at).ok()?;
                // After sleep or a late launch, send the due notification only.
                // A stale advance notification must not arrive alongside it.
                let kind = if due <= now {
                    (task.notified_due_at.as_ref() != Some(&task.due_at))
                        .then_some(ReminderKind::Due)
                } else {
                    advance_time(&task)
                        .filter(|at| *at <= now)
                        .filter(|at| {
                            task.notified_reminder_at.as_deref() != Some(at.to_rfc3339().as_str())
                        })
                        .map(|_| ReminderKind::Advance)
                }?;
                Some(Reminder { task, kind })
            })
            .collect())
    }

    pub fn snapshot(&self) -> Result<Snapshot> {
        Ok(Snapshot {
            spaces: self.rows("spaces")?,
            items: self.rows("items")?,
            tasks: self.rows("tasks")?,
            records: self.rows("records")?,
            settings: self.settings()?,
        })
    }

    pub fn save_space(&mut self, mut space: Space) -> Result<Snapshot> {
        if space.id.is_empty() {
            space.id = Uuid::new_v4().to_string();
        }
        space.name = space.name.trim().into();
        let mut data = self.snapshot()?;
        data.spaces.retain(|s| s.id != space.id);
        data.spaces.push(space.clone());
        validate(&data)?;
        self.conn.execute("INSERT INTO spaces (id,parent_id,data) VALUES (?1,?2,?3) ON CONFLICT(id) DO UPDATE SET parent_id=excluded.parent_id,data=excluded.data", params![space.id, space.parent_id, json(&space)?]).map_err(|e| e.to_string())?;
        touch(&self.conn)?;
        self.snapshot()
    }

    pub fn save_item(&mut self, mut item: Item) -> Result<Snapshot> {
        if item.id.is_empty() {
            item.id = Uuid::new_v4().to_string();
        }
        item.name = item.name.trim().into();
        item.photo = None;
        let mut data = self.snapshot()?;
        let previous = data.items.iter().find(|i| i.id == item.id);
        let old_photos: HashSet<&str> = previous
            .iter()
            .flat_map(|i| i.photos.iter().map(String::as_str))
            .collect();
        // The editor sends a newly picked file as a data URL and an unchanged
        // one as its stored id.
        let mut added = vec![];
        for photo in &mut item.photos {
            if photo.starts_with("data:") {
                let file = StoredFile::from_data_url(photo, FileKind::Photo)?;
                *photo = Uuid::new_v4().to_string();
                added.push((photo.clone(), file));
            } else if !old_photos.contains(photo.as_str()) {
                return Err("사진을 찾을 수 없습니다. 사진을 다시 선택해 주세요.".into());
            }
        }
        for attachment in &mut item.attachments {
            attachment.name = attachment.name.trim().into();
            if let Some(url) = attachment.data.take() {
                let file = StoredFile::from_data_url(&url, FileKind::Attachment)?;
                attachment.id = Uuid::new_v4().to_string();
                attachment.mime = file.mime.clone();
                attachment.size = file.data.len() as u64;
                added.push((attachment.id.clone(), file));
            } else {
                let stored = previous
                    .and_then(|i| i.attachments.iter().find(|a| a.id == attachment.id))
                    .ok_or("첨부 파일을 찾을 수 없습니다. 파일을 다시 선택해 주세요.")?;
                attachment.mime = stored.mime.clone();
                attachment.size = stored.size;
            }
        }
        let file_ids = |i: &Item| -> HashSet<String> {
            i.photos
                .iter()
                .chain(i.attachments.iter().map(|a| &a.id))
                .cloned()
                .collect()
        };
        let kept = file_ids(&item);
        let removed: Vec<String> = previous
            .map(|i| file_ids(i).difference(&kept).cloned().collect())
            .unwrap_or_default();
        data.items.retain(|i| i.id != item.id);
        data.items.push(item.clone());
        validate(&data)?;
        let tx = self.conn.transaction().map_err(err)?;
        tx.execute("INSERT INTO items (id,space_id,data) VALUES (?1,?2,?3) ON CONFLICT(id) DO UPDATE SET space_id=excluded.space_id,data=excluded.data", params![item.id, item.space_id, json(&item)?]).map_err(err)?;
        for id in &removed {
            tx.execute("DELETE FROM files WHERE id = ?1", [id])
                .map_err(err)?;
        }
        for (id, file) in &added {
            insert_file(&tx, id, &item.id, file)?;
        }
        touch(&tx)?;
        tx.commit().map_err(err)?;
        self.snapshot()
    }

    pub fn save_task(&mut self, mut task: Task) -> Result<Snapshot> {
        let mut data = self.snapshot()?;
        if task.id.is_empty() {
            task.id = Uuid::new_v4().to_string();
        }
        task.title = task.title.trim().into();
        task.notified_due_at = data
            .tasks
            .iter()
            .find(|t| t.id == task.id && t.due_at == task.due_at && t.active == task.active)
            .and_then(|t| t.notified_due_at.clone());
        task.notified_reminder_at = data
            .tasks
            .iter()
            .find(|t| {
                t.id == task.id
                    && t.due_at == task.due_at
                    && t.active == task.active
                    && t.reminder_minutes == task.reminder_minutes
            })
            .and_then(|t| t.notified_reminder_at.clone());
        data.tasks.retain(|t| t.id != task.id);
        data.tasks.push(task.clone());
        validate(&data)?;
        self.conn.execute("INSERT INTO tasks (id,space_id,item_id,data) VALUES (?1,?2,?3,?4) ON CONFLICT(id) DO UPDATE SET space_id=excluded.space_id,item_id=excluded.item_id,data=excluded.data", params![task.id, task.space_id, task.item_id, json(&task)?]).map_err(|e| e.to_string())?;
        touch(&self.conn)?;
        self.snapshot()
    }

    pub fn delete(&mut self, kind: &str, id: &str) -> Result<Snapshot> {
        let data = self.snapshot()?;
        let table = match kind {
            "space" => {
                if data
                    .spaces
                    .iter()
                    .any(|s| s.parent_id.as_deref() == Some(id))
                    || data.items.iter().any(|i| i.space_id == id)
                    || data.tasks.iter().any(|t| t.space_id.as_deref() == Some(id))
                {
                    return Err("하위 공간·아이템·할 일을 먼저 옮기거나 삭제해 주세요.".into());
                }
                "spaces"
            }
            "item" => {
                if data.tasks.iter().any(|t| t.item_id.as_deref() == Some(id)) {
                    return Err("연결된 할 일을 먼저 옮기거나 삭제해 주세요.".into());
                }
                "items"
            }
            "task" => "tasks",
            _ => return Err("삭제할 대상을 찾을 수 없습니다.".into()),
        };
        // Deleting an item also deletes its photos and attachments (ON DELETE CASCADE).
        self.conn
            .execute(&format!("DELETE FROM {table} WHERE id = ?1"), [id])
            .map_err(|e| e.to_string())?;
        touch(&self.conn)?;
        self.snapshot()
    }

    pub fn finish(&mut self, id: &str, expected_due: &str, outcome: &str) -> Result<Snapshot> {
        let data = self.snapshot()?;
        let mut task = data
            .tasks
            .iter()
            .find(|t| t.id == id)
            .cloned()
            .ok_or("할 일을 찾을 수 없습니다.")?;
        if !task.active || parse_time(&task.due_at)? != parse_time(expected_due)? {
            return Err("이미 처리된 일정입니다. 목록을 새로 확인해 주세요.".into());
        }
        if !["completed", "skipped"].contains(&outcome) {
            return Err("처리 방법이 올바르지 않습니다.".into());
        }
        let now = Utc::now();
        let target = if let Some(s) = &task.space_id {
            space_path(&data, s)
        } else {
            data.items
                .iter()
                .find(|i| Some(&i.id) == task.item_id.as_ref())
                .map(|i| format!("{} / {}", space_path(&data, &i.space_id), i.name))
                .unwrap_or_default()
        };
        let mut record = Record {
            id: Uuid::new_v4().to_string(),
            task_id: task.id.clone(),
            title: task.title.clone(),
            target,
            scheduled_for: task.due_at.clone(),
            completed_at: now.to_rfc3339(),
            outcome: outcome.into(),
            task_after: None,
        };
        match next_due(&task, now)? {
            Some(next) => task.due_at = next,
            None => task.active = false,
        }
        task.notified_due_at = None;
        task.notified_reminder_at = None;
        record.task_after = Some(task.clone());
        let tx = self.conn.transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "INSERT INTO records (id,data) VALUES (?1,?2)",
            params![record.id, json(&record)?],
        )
        .map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE tasks SET data=?2 WHERE id=?1",
            params![task.id, json(&task)?],
        )
        .map_err(|e| e.to_string())?;
        touch(&tx)?;
        tx.commit().map_err(|e| e.to_string())?;
        self.snapshot()
    }

    /// Moves the current occurrence to a later time. The recurrence rule and
    /// its anchor stay as they are, so later occurrences keep their dates.
    pub fn snooze(&mut self, id: &str, expected_due: &str, due_at: &str) -> Result<Snapshot> {
        let mut data = self.snapshot()?;
        let task = data
            .tasks
            .iter_mut()
            .find(|t| t.id == id)
            .ok_or("할 일을 찾을 수 없습니다.")?;
        let current = parse_time(&task.due_at)?;
        if !task.active || current != parse_time(expected_due)? {
            return Err("이미 처리된 일정입니다. 목록을 새로 확인해 주세요.".into());
        }
        let next = parse_time(due_at)?;
        if next <= current {
            return Err("지금 예정일보다 늦은 시각으로 미뤄 주세요.".into());
        }
        task.due_at = next.to_rfc3339();
        task.notified_due_at = None;
        task.notified_reminder_at = None;
        let task = task.clone();
        validate(&data)?;
        self.conn
            .execute(
                "UPDATE tasks SET data=?2 WHERE id=?1",
                params![task.id, json(&task)?],
            )
            .map_err(err)?;
        touch(&self.conn)?;
        self.snapshot()
    }

    pub fn reopen(&mut self, id: &str, record_id: &str) -> Result<Snapshot> {
        let data = self.snapshot()?;
        let mut task = data
            .tasks
            .iter()
            .find(|t| t.id == id)
            .cloned()
            .ok_or("할 일이 삭제되어 미완료로 되돌릴 수 없습니다.")?;
        let record = data
            .records
            .iter()
            .rev()
            .find(|r| r.task_id == id)
            .ok_or("취소할 수행 기록을 찾을 수 없습니다.")?;
        if record.id != record_id {
            return Err("가장 최근 기록부터 취소해 주세요.".into());
        }
        // Existing backups have no task_after. Reconstruct their resulting
        // occurrence using the stored completion time and current recurrence rule.
        let expected = match &record.task_after {
            Some(after) => after.clone(),
            None => {
                let mut after = task.clone();
                after.due_at = record.scheduled_for.clone();
                after.active = true;
                match next_due(&after, parse_time(&record.completed_at)?)? {
                    Some(next) => after.due_at = next,
                    None => after.active = false,
                }
                after
            }
        };
        if task.active != expected.active
            || parse_time(&task.due_at)? != parse_time(&expected.due_at)?
            || parse_time(&task.anchor_at)? != parse_time(&expected.anchor_at)?
            || task.mode != expected.mode
            || task.every != expected.every
            || task.unit != expected.unit
            || task.timezone != expected.timezone
        {
            return Err("처리 후 일정이 수정되어 취소할 수 없습니다. 할 일 수정에서 예정일과 활성 상태를 변경해 주세요.".into());
        }
        task.due_at = record.scheduled_for.clone();
        task.active = true;
        task.notified_due_at = None;
        task.notified_reminder_at = None;
        let tx = self.conn.transaction().map_err(|e| e.to_string())?;
        tx.execute(
            "UPDATE tasks SET data=?2 WHERE id=?1",
            params![task.id, json(&task)?],
        )
        .map_err(|e| e.to_string())?;
        tx.execute("DELETE FROM records WHERE id=?1", [record_id])
            .map_err(|e| e.to_string())?;
        touch(&tx)?;
        tx.commit().map_err(|e| e.to_string())?;
        self.snapshot()
    }

    pub fn set_settings(&mut self, settings: Settings) -> Result<Snapshot> {
        self.conn.execute("INSERT INTO metadata (key,data) VALUES ('settings',?1) ON CONFLICT(key) DO UPDATE SET data=excluded.data", [json(&settings)?]).map_err(|e| e.to_string())?;
        touch(&self.conn)?;
        self.snapshot()
    }

    pub fn mark_notified(&mut self, reminder: &Reminder) -> Result<()> {
        let mut t = reminder.task.clone();
        match reminder.kind {
            ReminderKind::Due => t.notified_due_at = Some(t.due_at.clone()),
            ReminderKind::Advance => {
                t.notified_reminder_at = advance_time(&t).map(|at| at.to_rfc3339());
            }
        }
        self.conn
            .execute(
                "UPDATE tasks SET data=?2 WHERE id=?1",
                params![t.id, json(&t)?],
            )
            .map_err(|e| e.to_string())?;
        Ok(())
    }

    /// Replaces all data with the contents of an opened backup.
    pub fn restore(&mut self, source: &Store) -> Result<Snapshot> {
        let data = source.snapshot()?;
        self.replace_all(&data, |id| Ok(source.file(id)?.ok_or(MISSING_FILE)?))?;
        self.snapshot()
    }

    /// Replaces everything in one transaction. `file` returns the bytes for
    /// each photo and attachment id the snapshot refers to.
    fn replace_all(
        &mut self,
        data: &Snapshot,
        mut file: impl FnMut(&str) -> Result<StoredFile>,
    ) -> Result<()> {
        validate(data)?;
        let tx = self.conn.transaction().map_err(|e| e.to_string())?;
        tx.execute_batch("PRAGMA defer_foreign_keys=ON; DELETE FROM files; DELETE FROM tasks; DELETE FROM items; DELETE FROM spaces; DELETE FROM records;").map_err(|e| e.to_string())?;
        for s in &data.spaces {
            tx.execute(
                "INSERT INTO spaces (id,parent_id,data) VALUES (?1,?2,?3)",
                params![s.id, s.parent_id, json(s)?],
            )
            .map_err(|e| e.to_string())?;
        }
        for i in &data.items {
            tx.execute(
                "INSERT INTO items (id,space_id,data) VALUES (?1,?2,?3)",
                params![i.id, i.space_id, json(i)?],
            )
            .map_err(|e| e.to_string())?;
            for id in &i.photos {
                let photo = file(id)?;
                photo.check(FileKind::Photo)?;
                insert_file(&tx, id, &i.id, &photo)?;
            }
            for a in &i.attachments {
                let stored = file(&a.id)?;
                stored.check(FileKind::Attachment)?;
                if stored.mime != a.mime {
                    return Err("첨부 파일 정보가 올바르지 않습니다.".into());
                }
                insert_file(&tx, &a.id, &i.id, &stored)?;
            }
        }
        for t in &data.tasks {
            tx.execute(
                "INSERT INTO tasks (id,space_id,item_id,data) VALUES (?1,?2,?3,?4)",
                params![t.id, t.space_id, t.item_id, json(t)?],
            )
            .map_err(|e| e.to_string())?;
        }
        for r in &data.records {
            tx.execute(
                "INSERT INTO records (id,data) VALUES (?1,?2)",
                params![r.id, json(r)?],
            )
            .map_err(|e| e.to_string())?;
        }
        tx.execute("INSERT INTO metadata (key,data) VALUES ('settings',?1) ON CONFLICT(key) DO UPDATE SET data=excluded.data", [json(&data.settings)?]).map_err(|e| e.to_string())?;
        touch(&tx)?;
        tx.commit().map_err(|e| e.to_string())?;
        Ok(())
    }
}

pub fn space_path(data: &Snapshot, id: &str) -> String {
    let mut names = vec![];
    let mut current = Some(id);
    let mut seen = HashSet::new();
    while let Some(id) = current {
        if !seen.insert(id) {
            break;
        }
        if let Some(space) = data.spaces.iter().find(|s| s.id == id) {
            names.push(space.name.clone());
            current = space.parent_id.as_deref();
        } else {
            break;
        }
    }
    names.reverse();
    names.join(" / ")
}

#[cfg(test)]
mod tests {
    use super::*;
    fn store() -> Store {
        Store::from_connection(Connection::open_in_memory().unwrap()).unwrap()
    }
    fn space(id: &str, parent: Option<&str>) -> Space {
        Space {
            id: id.into(),
            parent_id: parent.map(Into::into),
            name: id.into(),
            icon: "🏠".into(),
            notes: "".into(),
        }
    }

    #[test]
    fn cycles_and_nonempty_space_deletion_are_rejected_without_changes() {
        let mut db = store();
        db.save_space(space("home", None)).unwrap();
        db.save_space(space("shelf", Some("home"))).unwrap();
        assert!(db.save_space(space("home", Some("shelf"))).is_err());
        assert!(db.delete("space", "home").is_err());
        assert_eq!(db.snapshot().unwrap().spaces[0].parent_id, None);
    }

    /// A 1x1 JPEG header is enough for the signature check.
    const JPEG: &str = "data:image/jpeg;base64,/9j/2Q==";
    const PNG: &str = "data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAQAAAC1HAwCAAAAC0lEQVR42mP8/x8AAwMCAO+jp1sAAAAASUVORK5CYII=";

    const PDF: &str = "data:application/pdf;base64,JVBERi0xLjQK";

    fn item(id: &str, photo: Option<&str>) -> Item {
        Item {
            id: id.into(),
            space_id: "home".into(),
            name: id.into(),
            quantity: 1.0,
            unit: "개".into(),
            notes: "".into(),
            photos: photo.into_iter().map(Into::into).collect(),
            photo: None,
            attachments: vec![],
            expires_on: None,
            purchased_on: None,
            purchase_store: "".into(),
            purchase_price: None,
            warranty_until: None,
        }
    }

    /// An item as schema v1 and v2 stored it, with a single `photo` field.
    fn old_item(id: &str, photo: Option<&str>) -> Item {
        Item {
            photo: photo.map(Into::into),
            ..item(id, None)
        }
    }

    fn attachment(name: &str, data: &str) -> Attachment {
        Attachment {
            id: "".into(),
            name: name.into(),
            mime: "".into(),
            size: 0,
            data: Some(data.into()),
        }
    }

    fn file_rows(db: &Store) -> i64 {
        db.conn
            .query_row("SELECT count(*) FROM files", [], |r| r.get(0))
            .unwrap()
    }

    /// Writes rows without validation, as a damaged or hand-made backup might.
    fn raw_source(sql: &str) -> Store {
        let source = store();
        source
            .conn
            .execute_batch(&format!("PRAGMA foreign_keys = OFF; {sql}"))
            .unwrap();
        source
    }

    #[test]
    fn invalid_restore_preserves_existing_database() {
        let mut db = store();
        db.save_space(space("home", None)).unwrap();
        let orphan = raw_source(&format!(
            "INSERT INTO spaces VALUES ('shelf','missing','{}');",
            json(&space("shelf", Some("missing"))).unwrap()
        ));
        assert!(db.restore(&orphan).is_err());
        // The photo row is missing, so the restore fails mid-transaction.
        let missing_photo = raw_source(&format!(
            "INSERT INTO spaces VALUES ('home',NULL,'{}'); INSERT INTO items VALUES ('cup','home','{}');",
            json(&space("home", None)).unwrap(),
            json(&item("cup", Some("ghost"))).unwrap()
        ));
        assert_eq!(
            db.restore(&missing_photo).unwrap_err(),
            MISSING_FILE.to_string()
        );
        let data = db.snapshot().unwrap();
        assert_eq!(data.spaces.len(), 1);
        assert!(data.items.is_empty());
    }

    #[test]
    fn restore_accepts_children_before_parents_and_round_trips() {
        let mut db = store();
        let data = Snapshot {
            spaces: vec![space("shelf", Some("home")), space("home", None)],
            ..Default::default()
        };
        let restored = db.restore(&Store::from_legacy(data).unwrap()).unwrap();
        assert_eq!(restored.spaces.len(), 2);
        assert_eq!(space_path(&restored, "shelf"), "home / shelf");
    }

    #[test]
    fn photos_are_stored_apart_from_item_data_and_follow_their_item() {
        let mut db = store();
        db.save_space(space("home", None)).unwrap();
        let data = db.save_item(item("cup", Some(JPEG))).unwrap();
        let first = data.items[0].photos[0].clone();
        assert!(!first.starts_with("data:"), "snapshots carry only the id");
        assert_eq!(
            db.file(&first).unwrap().unwrap(),
            StoredFile {
                mime: "image/jpeg".into(),
                data: vec![0xFF, 0xD8, 0xFF, 0xD9]
            }
        );
        // Saving other fields keeps the photo.
        let mut edited = data.items[0].clone();
        edited.notes = "메모".into();
        let data = db.save_item(edited.clone()).unwrap();
        assert_eq!(data.items[0].photos, std::slice::from_ref(&first));
        // A new photo gets a new id and the old row is removed.
        edited.photos = vec![PNG.into()];
        let data = db.save_item(edited.clone()).unwrap();
        let second = data.items[0].photos[0].clone();
        assert_ne!(second, first);
        assert!(db.file(&first).unwrap().is_none());
        assert_eq!(db.file(&second).unwrap().unwrap().mime, "image/png");
        assert_eq!(file_rows(&db), 1);
        // Another item cannot point at this photo.
        assert!(db.save_item(item("plate", Some(&second))).is_err());
        // Bytes that do not match the declared type are rejected.
        assert!(db
            .save_item(item("bowl", Some("data:image/png;base64,/9j/2Q==")))
            .is_err());
        // A document is not a photo.
        assert!(db.save_item(item("bowl", Some(PDF))).is_err());
        edited.photos = vec![];
        db.save_item(edited.clone()).unwrap();
        assert_eq!(file_rows(&db), 0);
        edited.photos = vec![JPEG.into()];
        db.save_item(edited).unwrap();
        db.delete("item", "cup").unwrap();
        assert_eq!(file_rows(&db), 0);
    }

    #[test]
    fn items_keep_several_photos_in_order_and_attachments_with_their_names() {
        let mut db = store();
        db.save_space(space("home", None)).unwrap();
        let mut tv = item("tv", None);
        tv.photos = vec![JPEG.into(), PNG.into()];
        tv.attachments = vec![
            attachment(" 보증서.pdf ", PDF),
            attachment("영수증.jpg", JPEG),
        ];
        tv.warranty_until = Some("2028-11-11".into());
        let saved = db.save_item(tv).unwrap().items.remove(0);
        assert_eq!(saved.warranty_until.as_deref(), Some("2028-11-11"));
        assert_eq!(file_rows(&db), 4);
        assert_eq!(
            db.file(&saved.photos[1]).unwrap().unwrap().mime,
            "image/png"
        );
        let warranty = &saved.attachments[0];
        assert_eq!(
            (
                warranty.name.as_str(),
                warranty.mime.as_str(),
                warranty.size
            ),
            ("보증서.pdf", "application/pdf", 9)
        );
        assert!(warranty.data.is_none(), "snapshots carry no file bytes");
        assert_eq!(db.file(&warranty.id).unwrap().unwrap().data, b"%PDF-1.4\n");

        // Reordering makes another photo the cover without rewriting files.
        // Stored details of a kept attachment cannot be changed by the editor.
        let mut edited = saved.clone();
        edited.photos.reverse();
        edited.photos.push(JPEG.into());
        edited.attachments.remove(1);
        edited.attachments[0].mime = "image/png".into();
        edited.attachments[0].size = 1;
        let next = db.save_item(edited).unwrap().items.remove(0);
        assert_eq!(
            next.photos[..2],
            [saved.photos[1].clone(), saved.photos[0].clone()]
        );
        assert_eq!(next.attachments, std::slice::from_ref(warranty));
        assert!(db.file(&saved.attachments[1].id).unwrap().is_none());
        assert_eq!(file_rows(&db), 4);

        // Limits, unknown ids and unsupported types leave the item unchanged.
        let mut invalid = next.clone();
        invalid.photos = vec![JPEG.into(); MAX_PHOTOS + 1];
        assert!(db.save_item(invalid).is_err());
        let mut invalid = next.clone();
        invalid.attachments[0].id = "ghost".into();
        assert!(db.save_item(invalid).is_err());
        let mut invalid = next.clone();
        invalid.attachments = vec![attachment(
            "run.exe",
            "data:application/x-msdownload;base64,TVo=",
        )];
        assert!(db.save_item(invalid).is_err());
        let mut invalid = next.clone();
        invalid.attachments = vec![attachment(
            "fake.pdf",
            "data:application/pdf;base64,/9j/2Q==",
        )];
        assert!(db.save_item(invalid).is_err());
        let mut invalid = next.clone();
        invalid.warranty_until = Some("내년".into());
        assert!(db.save_item(invalid).is_err());
        assert_eq!(db.snapshot().unwrap().items[0], next);
        assert_eq!(file_rows(&db), 4);

        // Everything survives a restore into another database.
        let mut other = store();
        let restored = other.restore(&db).unwrap();
        assert_eq!(restored.items[0], next);
        assert_eq!(file_rows(&other), 4);
        db.delete("item", "tv").unwrap();
        assert_eq!(file_rows(&db), 0);
    }

    #[test]
    fn upgrading_v2_keeps_each_photo_as_the_first_of_its_item() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("zipsa.sqlite");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("
            CREATE TABLE spaces (id TEXT PRIMARY KEY, parent_id TEXT REFERENCES spaces(id) DEFERRABLE INITIALLY DEFERRED, data TEXT NOT NULL);
            CREATE TABLE items (id TEXT PRIMARY KEY, space_id TEXT NOT NULL REFERENCES spaces(id), data TEXT NOT NULL);
            CREATE TABLE tasks (id TEXT PRIMARY KEY, space_id TEXT REFERENCES spaces(id), item_id TEXT REFERENCES items(id), data TEXT NOT NULL, CHECK ((space_id IS NULL) != (item_id IS NULL)));
            CREATE TABLE records (id TEXT PRIMARY KEY, data TEXT NOT NULL);
            CREATE TABLE metadata (key TEXT PRIMARY KEY, data TEXT NOT NULL);
            CREATE TABLE photos (id TEXT PRIMARY KEY, item_id TEXT NOT NULL UNIQUE REFERENCES items(id) ON DELETE CASCADE, mime TEXT NOT NULL, data BLOB NOT NULL);
            PRAGMA application_id = 0x5A495053;
            PRAGMA user_version = 2;").unwrap();
        conn.execute(
            "INSERT INTO spaces VALUES ('home',NULL,?1)",
            [json(&space("home", None)).unwrap()],
        )
        .unwrap();
        for (id, photo) in [("cup", Some("photo-1")), ("bowl", None)] {
            conn.execute(
                "INSERT INTO items VALUES (?1,'home',?2)",
                params![id, json(&old_item(id, photo)).unwrap()],
            )
            .unwrap();
        }
        conn.execute(
            "INSERT INTO photos VALUES ('photo-1','cup','image/jpeg',?1)",
            [vec![0xFFu8, 0xD8, 0xFF, 0xD9]],
        )
        .unwrap();
        drop(conn);

        let mut db = Store::open(&path).unwrap();
        assert_eq!(user_version(&db.conn).unwrap(), SCHEMA_VERSION);
        let data = db.snapshot().unwrap();
        assert_eq!(data.items[0].photos, ["photo-1"]);
        assert!(data.items[0].photo.is_none());
        assert!(data.items[1].photos.is_empty());
        assert_eq!(
            db.file("photo-1").unwrap().unwrap().data,
            [0xFF, 0xD8, 0xFF, 0xD9]
        );
        let tables: i64 = db
            .conn
            .query_row(
                "SELECT count(*) FROM sqlite_schema WHERE name = 'photos'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tables, 0);
        // The upgraded item accepts a second photo.
        let mut cup = data.items[0].clone();
        cup.photos.push(PNG.into());
        assert_eq!(db.save_item(cup).unwrap().items.len(), 2);
        assert_eq!(file_rows(&db), 2);
        let saved: Vec<_> = fs::read_dir(dir.path().join("backups"))
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(saved.len(), 1);
        assert!(saved[0].starts_with("before-upgrade-v2-"), "{}", saved[0]);
    }

    #[test]
    fn upgrading_v1_moves_inline_photos_and_keeps_a_backup() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("zipsa.sqlite");
        let conn = Connection::open(&path).unwrap();
        conn.execute_batch("
            CREATE TABLE spaces (id TEXT PRIMARY KEY, parent_id TEXT REFERENCES spaces(id) DEFERRABLE INITIALLY DEFERRED, data TEXT NOT NULL);
            CREATE TABLE items (id TEXT PRIMARY KEY, space_id TEXT NOT NULL REFERENCES spaces(id), data TEXT NOT NULL);
            CREATE TABLE tasks (id TEXT PRIMARY KEY, space_id TEXT REFERENCES spaces(id), item_id TEXT REFERENCES items(id), data TEXT NOT NULL, CHECK ((space_id IS NULL) != (item_id IS NULL)));
            CREATE TABLE records (id TEXT PRIMARY KEY, data TEXT NOT NULL);
            CREATE TABLE metadata (key TEXT PRIMARY KEY, data TEXT NOT NULL);
            PRAGMA user_version = 1;").unwrap();
        conn.execute(
            "INSERT INTO spaces VALUES ('home',NULL,?1)",
            [json(&space("home", None)).unwrap()],
        )
        .unwrap();
        for (id, photo) in [
            ("cup", Some(JPEG)),
            ("plate", Some("data:image/jpeg;base64,!!")),
            ("bowl", None),
        ] {
            conn.execute(
                "INSERT INTO items VALUES (?1,'home',?2)",
                params![id, json(&old_item(id, photo)).unwrap()],
            )
            .unwrap();
        }
        drop(conn);

        let db = Store::open(&path).unwrap();
        assert_eq!(user_version(&db.conn).unwrap(), SCHEMA_VERSION);
        let data = db.snapshot().unwrap();
        let photo = |id: &str| {
            data.items
                .iter()
                .find(|i| i.id == id)
                .unwrap()
                .photos
                .first()
                .cloned()
        };
        let cup = photo("cup").unwrap();
        assert_eq!(
            db.file(&cup).unwrap().unwrap().data,
            [0xFF, 0xD8, 0xFF, 0xD9]
        );
        assert_eq!(photo("plate"), None, "an undecodable photo is dropped");
        assert_eq!(photo("bowl"), None);

        let saved: Vec<_> = fs::read_dir(dir.path().join("backups"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        assert_eq!(saved.len(), 1);
        let name = saved[0].file_name().unwrap().to_string_lossy().into_owned();
        assert!(name.starts_with("before-upgrade-v1-"), "{name}");
        // The pre-upgrade copy is itself restorable, inline photos included.
        let copy = dir.path().join("copy.sqlite");
        fs::copy(&saved[0], &copy).unwrap();
        let source = Store::open_backup_copy(&copy).unwrap();
        let mut restored = store();
        let data = restored.restore(&source).unwrap();
        let cup = data.items.iter().find(|i| i.id == "cup").unwrap();
        assert!(restored.file(&cup.photos[0]).unwrap().is_some());
        // Opening again does not repeat the upgrade backup.
        drop(db);
        Store::open(&path).unwrap();
        assert_eq!(fs::read_dir(dir.path().join("backups")).unwrap().count(), 1);
    }

    #[test]
    fn only_user_edits_change_the_revision() {
        let mut db = store();
        assert_eq!(db.revision().unwrap(), 0);
        db.save_space(space("home", None)).unwrap();
        let mut t = crate::schedule::tests::task();
        t.anchor_at = "2020-01-01T00:00:00Z".into();
        t.due_at = t.anchor_at.clone();
        db.save_task(t).unwrap();
        db.set_settings(Settings {
            notifications: true,
        })
        .unwrap();
        let revision = db.revision().unwrap();
        assert_eq!(revision, 3);
        db.mark_notified(&db.reminders().unwrap()[0]).unwrap();
        db.set_auto_backup_revision(revision).unwrap();
        assert_eq!(db.revision().unwrap(), revision);
        assert_eq!(db.auto_backup_revision().unwrap(), Some(revision));
    }

    #[test]
    fn finishing_same_occurrence_twice_is_rejected_and_history_survives_deletion() {
        let mut db = store();
        db.save_space(space("home", None)).unwrap();
        let mut t = crate::schedule::tests::task();
        t.mode = "once".into();
        db.save_task(t.clone()).unwrap();
        db.finish(&t.id, &t.due_at, "completed").unwrap();
        assert!(db.finish(&t.id, &t.due_at, "completed").is_err());
        let data = db.delete("task", &t.id).unwrap();
        assert_eq!(data.records.len(), 1);
        assert!(data.tasks.is_empty());
    }

    #[test]
    fn photos_purchase_data_and_item_tasks_survive_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("zipsa.sqlite");
        let mut db = Store::open(&path).unwrap();
        db.save_space(space("home", None)).unwrap();
        let item = Item {
            id: "milk".into(),
            space_id: "home".into(),
            name: "우유".into(),
            quantity: 2.0,
            unit: "개".into(),
            notes: "".into(),
            photos: vec!["data:image/jpeg;base64,/9j/2Q==".into()],
            photo: None,
            attachments: vec![],
            expires_on: Some("2026-11-15".into()),
            purchased_on: Some("2026-11-07".into()),
            purchase_store: "마트".into(),
            purchase_price: Some(4800),
            warranty_until: None,
        };
        db.save_item(item.clone()).unwrap();
        let mut t = crate::schedule::tests::task();
        t.space_id = None;
        t.item_id = Some(item.id.clone());
        db.save_task(t).unwrap();
        assert!(db.delete("item", &item.id).is_err());
        let photo_id = db.snapshot().unwrap().items[0].photos[0].clone();
        drop(db);
        let reopened = Store::open(&path).unwrap();
        let restored = reopened.snapshot().unwrap();
        assert_eq!(restored.items[0].photos, std::slice::from_ref(&photo_id));
        assert_eq!(
            reopened.file(&photo_id).unwrap().unwrap().data,
            [0xFF, 0xD8, 0xFF, 0xD9]
        );
        assert_eq!(restored.items[0].purchase_price, Some(4800));
        assert_eq!(restored.tasks[0].item_id.as_deref(), Some("milk"));
    }

    #[test]
    fn reminders_are_opt_in_and_deduplicated_after_content_edits() {
        let mut db = store();
        db.save_space(space("home", None)).unwrap();
        let mut t = crate::schedule::tests::task();
        t.anchor_at = "2020-01-01T00:00:00Z".into();
        t.due_at = t.anchor_at.clone();
        db.save_task(t.clone()).unwrap();
        assert!(db.reminders().unwrap().is_empty());
        db.set_settings(Settings {
            notifications: true,
        })
        .unwrap();
        assert_eq!(db.reminders().unwrap().len(), 1);
        db.mark_notified(&db.reminders().unwrap()[0]).unwrap();
        assert!(db.reminders().unwrap().is_empty());
        t.notes = "내용만 수정".into();
        db.save_task(t).unwrap();
        assert!(db.reminders().unwrap().is_empty());
    }

    #[test]
    fn older_app_does_not_reset_newer_schema_version() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("future.sqlite");
        let conn = Connection::open(&path).unwrap();
        conn.pragma_update(None, "user_version", SCHEMA_VERSION + 1)
            .unwrap();
        drop(conn);
        assert!(Store::open(&path).is_err());
        let conn = Connection::open(&path).unwrap();
        assert_eq!(user_version(&conn).unwrap(), SCHEMA_VERSION + 1);
    }

    #[test]
    fn completed_and_skipped_occurrences_can_be_reopened_and_completed_again() {
        for (mode, outcome) in [
            ("once", "completed"),
            ("calendar", "completed"),
            ("after_completion", "completed"),
            ("once", "skipped"),
        ] {
            let mut db = store();
            db.save_space(space("home", None)).unwrap();
            let mut t = crate::schedule::tests::task();
            t.mode = mode.into();
            db.save_task(t.clone()).unwrap();
            let done = db.finish(&t.id, &t.due_at, outcome).unwrap();
            let record_id = &done.records[0].id;
            let restored = db.reopen(&t.id, record_id).unwrap();
            assert!(restored.tasks[0].active);
            assert_eq!(restored.tasks[0].due_at, t.due_at);
            assert_eq!(restored.tasks[0].anchor_at, t.anchor_at);
            assert!(restored.tasks[0].notified_due_at.is_none());
            assert!(restored.records.is_empty());
            assert!(db.reopen(&t.id, record_id).is_err());
            assert_eq!(
                db.finish(&t.id, &t.due_at, "completed")
                    .unwrap()
                    .records
                    .len(),
                1
            );
        }
    }

    #[test]
    fn snoozing_moves_only_the_current_occurrence_and_rearms_notifications() {
        let mut db = store();
        db.save_space(space("home", None)).unwrap();
        db.set_settings(Settings {
            notifications: true,
        })
        .unwrap();
        // Monthly on the 31st, Seoul time.
        let t = crate::schedule::tests::task();
        db.save_task(t.clone()).unwrap();
        let due = parse_time(&t.due_at).unwrap();
        db.mark_notified(&db.reminders_at(due).unwrap()[0]).unwrap();
        let revision = db.revision().unwrap();

        assert!(db.snooze(&t.id, &t.due_at, &t.due_at).is_err());
        assert!(db.snooze(&t.id, &t.due_at, "2026-01-30T00:00:00Z").is_err());
        assert!(db
            .snooze(&t.id, "2026-01-01T00:00:00Z", "2026-02-03T00:00:00Z")
            .is_err());
        assert!(db.snooze(&t.id, &t.due_at, "내일").is_err());
        assert_eq!(db.revision().unwrap(), revision);

        // The same instant may be written with another offset.
        let data = db
            .snooze(
                &t.id,
                "2026-01-31T09:00:00+09:00",
                "2026-02-03T09:00:00+09:00",
            )
            .unwrap();
        let snoozed = data.tasks[0].clone();
        assert_eq!(
            parse_time(&snoozed.due_at).unwrap(),
            parse_time("2026-02-03T00:00:00Z").unwrap()
        );
        assert_eq!(snoozed.anchor_at, t.anchor_at);
        assert!(snoozed.notified_due_at.is_none());
        assert_eq!(db.revision().unwrap(), revision + 1);
        assert!(db.reminders_at(due).unwrap().is_empty());
        assert_eq!(
            db.reminders_at(parse_time(&snoozed.due_at).unwrap())
                .unwrap()
                .len(),
            1
        );
        // The next occurrence returns to the original monthly dates, as if
        // the task had never been snoozed.
        let done = db.finish(&t.id, &snoozed.due_at, "completed").unwrap();
        assert_eq!(
            done.tasks[0].due_at,
            next_due(&t, Utc::now()).unwrap().unwrap()
        );
        assert_eq!(done.records[0].scheduled_for, snoozed.due_at);
        // A finished one-time task cannot be snoozed.
        let mut once = t.clone();
        once.id = "once".into();
        once.mode = "once".into();
        db.save_task(once.clone()).unwrap();
        db.finish("once", &once.due_at, "skipped").unwrap();
        assert!(db
            .snooze("once", &once.due_at, "2026-02-03T00:00:00Z")
            .is_err());
    }

    #[test]
    fn recurring_completions_are_undone_in_reverse_order_without_losing_content_edits() {
        let mut db = store();
        db.save_space(space("home", None)).unwrap();
        let t = crate::schedule::tests::task();
        db.save_task(t.clone()).unwrap();
        let first = db.finish(&t.id, &t.due_at, "completed").unwrap();
        let mut edited = first.tasks[0].clone();
        edited.title = "새 이름".into();
        edited.notes = "새 메모".into();
        db.save_task(edited.clone()).unwrap();
        let second = db.finish(&t.id, &edited.due_at, "completed").unwrap();
        assert!(db.reopen(&t.id, &first.records[0].id).is_err());
        assert_eq!(db.snapshot().unwrap().records.len(), 2);
        let restored = db.reopen(&t.id, &second.records[1].id).unwrap();
        assert_eq!(restored.tasks[0].due_at, edited.due_at);
        let restored = db.reopen(&t.id, &first.records[0].id).unwrap();
        assert_eq!(restored.tasks[0].due_at, t.due_at);
        assert_eq!(restored.tasks[0].title, "새 이름");
        assert_eq!(restored.tasks[0].notes, "새 메모");
        assert!(restored.records.is_empty());
    }

    #[test]
    fn undo_does_not_overwrite_a_schedule_edited_after_completion() {
        let mut db = store();
        db.save_space(space("home", None)).unwrap();
        let t = crate::schedule::tests::task();
        db.save_task(t.clone()).unwrap();
        let done = db.finish(&t.id, &t.due_at, "completed").unwrap();
        let mut edited = done.tasks[0].clone();
        edited.every = 3;
        db.save_task(edited.clone()).unwrap();
        assert!(db.reopen(&t.id, &done.records[0].id).is_err());
        let current = db.snapshot().unwrap();
        assert_eq!(current.tasks[0].every, 3);
        assert_eq!(current.tasks[0].due_at, edited.due_at);
        assert_eq!(current.records.len(), 1);
    }

    #[test]
    fn completion_records_from_older_versions_can_be_undone() {
        for mode in ["once", "calendar", "after_completion"] {
            let mut db = store();
            db.save_space(space("home", None)).unwrap();
            let mut t = crate::schedule::tests::task();
            t.mode = mode.into();
            db.save_task(t.clone()).unwrap();
            let done = db.finish(&t.id, &t.due_at, "completed").unwrap();
            let mut legacy = done.records[0].clone();
            legacy.task_after = None;
            db.conn
                .execute(
                    "UPDATE records SET data=?2 WHERE id=?1",
                    params![legacy.id, json(&legacy).unwrap()],
                )
                .unwrap();
            let data = db.reopen(&t.id, &legacy.id).unwrap();
            assert!(data.tasks[0].active);
            assert_eq!(data.tasks[0].due_at, t.due_at);
            assert!(data.records.is_empty());
        }
    }

    #[test]
    fn undo_rolls_back_task_state_if_history_update_fails() {
        let mut db = store();
        db.save_space(space("home", None)).unwrap();
        let mut t = crate::schedule::tests::task();
        t.mode = "once".into();
        db.save_task(t.clone()).unwrap();
        let done = db.finish(&t.id, &t.due_at, "completed").unwrap();
        db.conn.execute_batch("CREATE TRIGGER prevent_record_delete BEFORE DELETE ON records BEGIN SELECT RAISE(ABORT, 'test failure'); END;").unwrap();
        assert!(db.reopen(&t.id, &done.records[0].id).is_err());
        let data = db.snapshot().unwrap();
        assert!(!data.tasks[0].active);
        assert_eq!(data.records.len(), 1);
    }

    #[test]
    fn advance_and_due_notifications_are_independent_and_do_not_reappear_after_restart() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("reminders.sqlite");
        let mut db = Store::open(&path).unwrap();
        db.save_space(space("home", None)).unwrap();
        let mut task = crate::schedule::tests::task();
        task.mode = "once".into();
        task.due_at = "2030-04-01T09:00:00Z".into();
        task.anchor_at = task.due_at.clone();
        task.reminder_minutes = Some(60);
        db.save_task(task.clone()).unwrap();
        db.set_settings(Settings {
            notifications: true,
        })
        .unwrap();
        assert!(db
            .reminders_at(parse_time("2030-04-01T07:59:59Z").unwrap())
            .unwrap()
            .is_empty());
        let at = parse_time("2030-04-01T08:00:00Z").unwrap();
        let notices = db.reminders_at(at).unwrap();
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].kind, ReminderKind::Advance);
        db.mark_notified(&notices[0]).unwrap();
        task.title = "이름만 바꾼 청소".into();
        let saved = db.save_task(task.clone()).unwrap();
        assert!(saved.tasks[0].notified_reminder_at.is_some());
        assert!(db.reminders_at(at).unwrap().is_empty());
        drop(db);
        let mut db = Store::open(&path).unwrap();
        assert!(db.reminders_at(at).unwrap().is_empty());
        let notices = db.reminders_at(parse_time(&task.due_at).unwrap()).unwrap();
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].kind, ReminderKind::Due);
        db.mark_notified(&notices[0]).unwrap();
        assert!(db
            .reminders_at(parse_time(&task.due_at).unwrap())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn missed_advance_is_replaced_by_due_and_edits_rearm_the_right_notification() {
        let mut db = store();
        db.save_space(space("home", None)).unwrap();
        db.set_settings(Settings {
            notifications: true,
        })
        .unwrap();
        let mut task = crate::schedule::tests::task();
        task.due_at = "2030-04-01T09:00:00Z".into();
        task.reminder_minutes = Some(1440);
        db.save_task(task.clone()).unwrap();
        let due = parse_time(&task.due_at).unwrap();
        let notices = db.reminders_at(due + Duration::hours(1)).unwrap();
        assert_eq!(notices.len(), 1);
        assert_eq!(notices[0].kind, ReminderKind::Due);
        db.mark_notified(&notices[0]).unwrap();
        task.reminder_minutes = Some(10);
        let saved = db.save_task(task.clone()).unwrap();
        assert!(saved.tasks[0].notified_due_at.is_some());
        assert!(db.reminders_at(due).unwrap().is_empty());
        task.due_at = "2030-04-02T09:00:00Z".into();
        let saved = db.save_task(task).unwrap();
        assert!(saved.tasks[0].notified_due_at.is_none());
        assert!(saved.tasks[0].notified_reminder_at.is_none());
        assert_eq!(
            db.reminders_at(parse_time("2030-04-02T08:50:00Z").unwrap())
                .unwrap()[0]
                .kind,
            ReminderKind::Advance
        );
    }

    #[test]
    fn advance_settings_survive_backup_recurrence_and_undo() {
        let mut db = store();
        db.save_space(space("home", None)).unwrap();
        let mut task = crate::schedule::tests::task();
        task.reminder_minutes = Some(10080);
        db.save_task(task.clone()).unwrap();
        db.set_settings(Settings {
            notifications: true,
        })
        .unwrap();
        let at = parse_time(&task.due_at).unwrap() - Duration::days(7);
        let notice = db.reminders_at(at).unwrap().remove(0);
        db.mark_notified(&notice).unwrap();
        assert!(db.snapshot().unwrap().tasks[0]
            .notified_reminder_at
            .is_some());
        let done = db.finish(&task.id, &task.due_at, "completed").unwrap();
        assert_eq!(done.tasks[0].reminder_minutes, Some(10080));
        assert!(done.tasks[0].notified_reminder_at.is_none());
        let bytes = serde_json::to_string(&done).unwrap();
        let restored: Snapshot = serde_json::from_str(&bytes).unwrap();
        let mut second = store();
        second
            .restore(&Store::from_legacy(restored).unwrap())
            .unwrap();
        let reopened = second.reopen(&task.id, &done.records[0].id).unwrap();
        assert_eq!(reopened.tasks[0].reminder_minutes, Some(10080));
        assert!(reopened.tasks[0].notified_reminder_at.is_none());
        assert_eq!(reopened.tasks[0].due_at, task.due_at);
    }

    #[test]
    fn legacy_task_json_defaults_to_no_advance_reminder_and_invalid_values_are_rejected() {
        let mut value = serde_json::to_value(crate::schedule::tests::task()).unwrap();
        value.as_object_mut().unwrap().remove("reminderMinutes");
        value.as_object_mut().unwrap().remove("notifiedReminderAt");
        let legacy: Task = serde_json::from_value(value).unwrap();
        assert!(legacy.reminder_minutes.is_none());
        assert!(legacy.notified_reminder_at.is_none());
        let mut db = store();
        db.save_space(space("home", None)).unwrap();
        db.save_task(legacy.clone()).unwrap();
        let mut invalid = legacy;
        invalid.reminder_minutes = Some(11);
        assert!(db.save_task(invalid).is_err());
        assert_eq!(db.snapshot().unwrap().tasks[0].reminder_minutes, None);
    }
}
