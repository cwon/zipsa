use std::{
    collections::{HashMap, HashSet},
    path::Path,
};

use chrono::{DateTime, Duration, NaiveDate, Utc};
use rusqlite::{params, Connection};
use serde::{de::DeserializeOwned, Serialize};
use uuid::Uuid;

use crate::{
    models::*,
    schedule::{next_due, parse_time},
};

type Result<T> = std::result::Result<T, String>;

pub struct Store {
    pub conn: Connection,
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
        if let Some(photo) = &i.photo {
            if photo.len() > 4 * 1024 * 1024
                || ![
                    "data:image/jpeg;base64,",
                    "data:image/png;base64,",
                    "data:image/webp;base64,",
                ]
                .iter()
                .any(|prefix| photo.starts_with(prefix))
            {
                return Err("사진은 4MB 이하의 JPEG·PNG·WebP 형식이어야 합니다.".into());
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
        Self::from_connection(Connection::open(path).map_err(|e| e.to_string())?)
    }

    pub fn from_connection(conn: Connection) -> Result<Self> {
        let version: u32 = conn
            .pragma_query_value(None, "user_version", |r| r.get(0))
            .map_err(|e| e.to_string())?;
        if version > 1 {
            return Err("더 최신 버전의 집사로 만든 데이터입니다. 앱을 업데이트해 주세요.".into());
        }
        conn.execute_batch("PRAGMA foreign_keys = ON; PRAGMA journal_mode = WAL; PRAGMA busy_timeout = 5000;
            CREATE TABLE IF NOT EXISTS spaces (id TEXT PRIMARY KEY, parent_id TEXT REFERENCES spaces(id) DEFERRABLE INITIALLY DEFERRED, data TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS items (id TEXT PRIMARY KEY, space_id TEXT NOT NULL REFERENCES spaces(id), data TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS tasks (id TEXT PRIMARY KEY, space_id TEXT REFERENCES spaces(id), item_id TEXT REFERENCES items(id), data TEXT NOT NULL, CHECK ((space_id IS NULL) != (item_id IS NULL)));
            CREATE TABLE IF NOT EXISTS records (id TEXT PRIMARY KEY, data TEXT NOT NULL);
            CREATE TABLE IF NOT EXISTS metadata (key TEXT PRIMARY KEY, data TEXT NOT NULL);
            CREATE INDEX IF NOT EXISTS spaces_parent ON spaces(parent_id);
            CREATE INDEX IF NOT EXISTS items_space ON items(space_id);
            CREATE INDEX IF NOT EXISTS tasks_space ON tasks(space_id);
            CREATE INDEX IF NOT EXISTS tasks_item ON tasks(item_id);
            PRAGMA user_version = 1;") .map_err(|e| e.to_string())?;
        Ok(Self { conn })
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
        self.snapshot()
    }

    pub fn save_item(&mut self, mut item: Item) -> Result<Snapshot> {
        if item.id.is_empty() {
            item.id = Uuid::new_v4().to_string();
        }
        item.name = item.name.trim().into();
        let mut data = self.snapshot()?;
        data.items.retain(|i| i.id != item.id);
        data.items.push(item.clone());
        validate(&data)?;
        self.conn.execute("INSERT INTO items (id,space_id,data) VALUES (?1,?2,?3) ON CONFLICT(id) DO UPDATE SET space_id=excluded.space_id,data=excluded.data", params![item.id, item.space_id, json(&item)?]).map_err(|e| e.to_string())?;
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
        self.conn
            .execute(&format!("DELETE FROM {table} WHERE id = ?1"), [id])
            .map_err(|e| e.to_string())?;
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
        tx.commit().map_err(|e| e.to_string())?;
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
        tx.commit().map_err(|e| e.to_string())?;
        self.snapshot()
    }

    pub fn set_settings(&mut self, settings: Settings) -> Result<Snapshot> {
        self.conn.execute("INSERT INTO metadata (key,data) VALUES ('settings',?1) ON CONFLICT(key) DO UPDATE SET data=excluded.data", [json(&settings)?]).map_err(|e| e.to_string())?;
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

    pub fn restore(&mut self, data: &Snapshot) -> Result<Snapshot> {
        validate(data)?;
        let tx = self.conn.transaction().map_err(|e| e.to_string())?;
        tx.execute_batch("PRAGMA defer_foreign_keys=ON; DELETE FROM tasks; DELETE FROM items; DELETE FROM spaces; DELETE FROM records;").map_err(|e| e.to_string())?;
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
        tx.commit().map_err(|e| e.to_string())?;
        self.snapshot()
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

    #[test]
    fn invalid_restore_preserves_existing_database() {
        let mut db = store();
        db.save_space(space("home", None)).unwrap();
        let bad = Snapshot {
            spaces: vec![space("shelf", Some("missing"))],
            ..Default::default()
        };
        assert!(db.restore(&bad).is_err());
        assert_eq!(db.snapshot().unwrap().spaces[0].id, "home");
    }

    #[test]
    fn restore_accepts_children_before_parents_and_round_trips() {
        let mut db = store();
        let data = Snapshot {
            spaces: vec![space("shelf", Some("home")), space("home", None)],
            ..Default::default()
        };
        let restored = db.restore(&data).unwrap();
        assert_eq!(restored.spaces.len(), 2);
        assert_eq!(space_path(&restored, "shelf"), "home / shelf");
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
            photo: Some("data:image/jpeg;base64,/9j/2Q==".into()),
            expires_on: Some("2026-11-15".into()),
            purchased_on: Some("2026-11-07".into()),
            purchase_store: "마트".into(),
            purchase_price: Some(4800),
        };
        db.save_item(item.clone()).unwrap();
        let mut t = crate::schedule::tests::task();
        t.space_id = None;
        t.item_id = Some(item.id.clone());
        db.save_task(t).unwrap();
        assert!(db.delete("item", &item.id).is_err());
        drop(db);
        let restored = Store::open(&path).unwrap().snapshot().unwrap();
        assert_eq!(restored.items[0].photo, item.photo);
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
        conn.pragma_update(None, "user_version", 2).unwrap();
        drop(conn);
        assert!(Store::open(&path).is_err());
        let conn = Connection::open(&path).unwrap();
        assert_eq!(
            conn.pragma_query_value(None, "user_version", |r| r.get::<_, u32>(0))
                .unwrap(),
            2
        );
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
        second.restore(&restored).unwrap();
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
