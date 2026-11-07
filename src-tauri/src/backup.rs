//! `.zipsa` backup files and the daily automatic backup.
//!
//! Since schema v2 a backup is a compact SQLite copy of the whole database,
//! photos included, so its size is not limited by memory. Backups from v0.2.0
//! and earlier are JSON files with inline photos and can still be restored.

use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
};

use chrono::{Local, NaiveDateTime, TimeZone};
use serde::Serialize;

use crate::{models::Backup, store::Store};

type Result<T> = std::result::Result<T, String>;

const INVALID: &str = "집사 백업 파일을 읽지 못했습니다. 올바른 .zipsa 파일인지 확인해 주세요.";
const SQLITE_HEADER: &[u8; 16] = b"SQLite format 3\0";
/// Older JSON backups are read into memory at once, so they stay capped.
const LEGACY_MAX: u64 = 256 * 1024 * 1024;

pub const AUTO_KEEP: usize = 7;
const AUTO_PREFIX: &str = "Zipsa-auto-";
const AUTO_STAMP: &str = "%Y%m%d-%H%M%S";

/// A backup opened for restoring.
pub struct BackupSource {
    pub store: Store,
    // Declared after `store` so the database closes before its copy is deleted.
    _copy: Option<tempfile::TempDir>,
}

pub fn open(path: &Path) -> Result<BackupSource> {
    let mut header = Vec::with_capacity(SQLITE_HEADER.len());
    fs::File::open(path)
        .and_then(|file| {
            file.take(SQLITE_HEADER.len() as u64)
                .read_to_end(&mut header)
        })
        .map_err(|_| "백업 파일을 열지 못했습니다.")?;
    if header == SQLITE_HEADER {
        open_database(path)
    } else {
        open_legacy(path)
    }
}

fn open_database(path: &Path) -> Result<BackupSource> {
    // Work on a private copy: the chosen file is never modified, and an older
    // backup can be upgraded to the current schema before it is read.
    let dir = tempfile::tempdir().map_err(|e| e.to_string())?;
    let copy = dir.path().join("backup.sqlite");
    fs::copy(path, &copy).map_err(|_| "백업 파일을 읽지 못했습니다.")?;
    Ok(BackupSource {
        store: Store::open_backup_copy(&copy)?,
        _copy: Some(dir),
    })
}

fn open_legacy(path: &Path) -> Result<BackupSource> {
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > LEGACY_MAX {
        return Err("이전 형식의 백업 파일은 256MB 이하여야 합니다.".into());
    }
    let mut bytes = Vec::new();
    file.take(LEGACY_MAX + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > LEGACY_MAX {
        return Err("백업 파일이 너무 큽니다.".into());
    }
    let backup: Backup = serde_json::from_slice(&bytes).map_err(|_| INVALID)?;
    if backup.format != "zipsa-backup" || backup.version != 1 {
        return Err("지원하지 않는 백업 형식입니다.".into());
    }
    Ok(BackupSource {
        store: Store::from_legacy(backup.data)?,
        _copy: None,
    })
}

pub fn auto_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("backups").join("auto")
}

/// Automatic backups, oldest first, with the local time from their file name.
fn auto_backups(dir: &Path) -> Vec<(NaiveDateTime, PathBuf)> {
    let Ok(entries) = fs::read_dir(dir) else {
        return vec![];
    };
    let mut list: Vec<_> = entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();
            let stamp = path
                .file_name()?
                .to_str()?
                .strip_prefix(AUTO_PREFIX)?
                .strip_suffix(".zipsa")?;
            let at = NaiveDateTime::parse_from_str(stamp, AUTO_STAMP).ok()?;
            Some((at, path))
        })
        .collect();
    list.sort();
    list
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AutoBackupStatus {
    pub last_at: Option<String>,
    pub count: usize,
    pub keep: usize,
}

pub fn auto_status(dir: &Path) -> AutoBackupStatus {
    let list = auto_backups(dir);
    AutoBackupStatus {
        last_at: list
            .last()
            .and_then(|(at, _)| Local.from_local_datetime(at).earliest())
            .map(|at| at.to_rfc3339()),
        count: list.len(),
        keep: AUTO_KEEP,
    }
}

/// Backs up the data at most once a day, and only when it changed since the
/// last automatic backup. Keeps the newest `AUTO_KEEP` files.
pub fn run_auto(store: &mut Store, dir: &Path, now: NaiveDateTime) -> Result<Option<PathBuf>> {
    let revision = store.revision()?;
    if let Some((last, _)) = auto_backups(dir).last() {
        if now.signed_duration_since(*last) < chrono::Duration::hours(24)
            || store.auto_backup_revision()? == Some(revision)
        {
            return Ok(None);
        }
    }
    if store.is_empty()? {
        return Ok(None);
    }
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    let path = dir.join(format!("{AUTO_PREFIX}{}.zipsa", now.format(AUTO_STAMP)));
    store.backup_to(&path)?;
    store.set_auto_backup_revision(revision)?;
    for (_, old) in auto_backups(dir).iter().rev().skip(AUTO_KEEP) {
        let _ = fs::remove_file(old);
    }
    Ok(Some(path))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::models::*;
    use rusqlite::Connection;

    const JPEG: &str = "data:image/jpeg;base64,/9j/2Q==";

    fn home() -> Space {
        Space {
            id: "home".into(),
            parent_id: None,
            name: "우리 집".into(),
            icon: "🏠".into(),
            notes: "로컬 백업".into(),
        }
    }

    fn cup(photo: Option<&str>) -> Item {
        Item {
            id: "cup".into(),
            space_id: "home".into(),
            name: "컵".into(),
            quantity: 1.0,
            unit: "개".into(),
            notes: "".into(),
            photo: photo.map(Into::into),
            expires_on: None,
            purchased_on: None,
            purchase_store: "".into(),
            purchase_price: None,
        }
    }

    fn filled() -> Store {
        let mut db = Store::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        db.save_space(home()).unwrap();
        db.save_item(cup(Some(JPEG))).unwrap();
        db.set_settings(Settings {
            notifications: true,
        })
        .unwrap();
        db
    }

    fn at(stamp: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(stamp, "%Y-%m-%d %H:%M").unwrap()
    }

    #[test]
    fn database_backup_round_trips_photos_and_settings() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("backup.zipsa");
        let db = filled();
        db.backup_to(&path).unwrap();
        // Overwriting an existing file (a save dialog may allow it) works too.
        db.backup_to(&path).unwrap();
        assert_eq!(
            fs::read_dir(dir.path()).unwrap().count(),
            1,
            "no staging left"
        );

        let source = open(&path).unwrap();
        let mut target = Store::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        let data = target.restore(&source.store).unwrap();
        assert_eq!(data.spaces[0].name, "우리 집");
        assert!(data.settings.notifications);
        let photo = target
            .photo(data.items[0].photo.as_deref().unwrap())
            .unwrap();
        assert_eq!(photo.unwrap().data, [0xFF, 0xD8, 0xFF, 0xD9]);
        // The chosen file itself is never changed by opening it.
        drop(source);
        let check = Connection::open(&path).unwrap();
        let mode: String = check
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode, "delete");
    }

    #[test]
    fn legacy_json_backups_with_inline_photos_still_restore() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("old.zipsa");
        let legacy = Backup {
            format: "zipsa-backup".into(),
            version: 1,
            exported_at: "2026-11-01T00:00:00Z".into(),
            data: Snapshot {
                spaces: vec![home()],
                items: vec![cup(Some(JPEG))],
                ..Default::default()
            },
        };
        fs::write(&path, serde_json::to_vec(&legacy).unwrap()).unwrap();
        let source = open(&path).unwrap();
        let mut target = Store::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        let data = target.restore(&source.store).unwrap();
        let id = data.items[0].photo.as_deref().unwrap();
        assert!(!id.starts_with("data:"));
        assert!(target.photo(id).unwrap().is_some());
    }

    #[test]
    fn other_files_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("other.zipsa");
        fs::write(&path, r#"{"format":"other-app","version":1,"exportedAt":"2026-01-01","data":{"spaces":[],"items":[],"tasks":[],"records":[],"settings":{"notifications":false}}}"#).unwrap();
        assert!(open(&path).is_err());
        fs::write(&path, "not a backup").unwrap();
        assert!(open(&path).is_err());
        // Another app's SQLite file must not restore as an empty data set.
        fs::remove_file(&path).unwrap();
        Connection::open(&path)
            .unwrap()
            .execute_batch("CREATE TABLE notes (body TEXT); INSERT INTO notes VALUES ('x');")
            .unwrap();
        assert!(open(&path).is_err());
        // A truncated database is rejected.
        let good = dir.path().join("good.zipsa");
        filled().backup_to(&good).unwrap();
        let bytes = fs::read(&good).unwrap();
        fs::write(&path, &bytes[..bytes.len() / 2]).unwrap();
        assert!(open(&path).is_err());
    }

    #[test]
    fn automatic_backups_run_daily_only_after_changes_and_keep_seven() {
        let dir = tempfile::tempdir().unwrap();
        let auto = auto_dir(dir.path());
        let mut empty = Store::from_connection(Connection::open_in_memory().unwrap()).unwrap();
        assert!(run_auto(&mut empty, &auto, at("2026-11-01 09:00"))
            .unwrap()
            .is_none());

        let mut db = filled();
        let first = run_auto(&mut db, &auto, at("2026-11-01 09:00")).unwrap();
        assert!(first.unwrap().ends_with("Zipsa-auto-20261101-090000.zipsa"));
        // Unchanged data is not backed up again, even days later.
        assert!(run_auto(&mut db, &auto, at("2026-11-05 09:00"))
            .unwrap()
            .is_none());
        // A change waits until a day has passed since the last backup.
        db.save_space(home()).unwrap();
        assert!(run_auto(&mut db, &auto, at("2026-11-02 08:59"))
            .unwrap()
            .is_none());
        assert!(run_auto(&mut db, &auto, at("2026-11-02 09:00"))
            .unwrap()
            .is_some());

        for day in 3..=12 {
            db.save_space(home()).unwrap();
            let now = at(&format!("2026-11-{day:02} 09:00"));
            assert!(run_auto(&mut db, &auto, now).unwrap().is_some());
        }
        let status = auto_status(&auto);
        assert_eq!(status.count, AUTO_KEEP);
        assert!(status.last_at.unwrap().starts_with("2026-11-12T09:00:00"));
        let names: Vec<_> = auto_backups(&auto)
            .into_iter()
            .map(|(at, _)| at.format("%d").to_string())
            .collect();
        assert_eq!(names, ["06", "07", "08", "09", "10", "11", "12"]);
        // Files that are not automatic backups are left alone.
        fs::write(auto.join("메모.txt"), "keep").unwrap();
        db.save_space(home()).unwrap();
        run_auto(&mut db, &auto, at("2026-11-13 09:00")).unwrap();
        assert!(auto.join("메모.txt").exists());
        // Deleting every backup makes the next check write one again.
        for (_, path) in auto_backups(&auto) {
            fs::remove_file(path).unwrap();
        }
        assert!(run_auto(&mut db, &auto, at("2026-11-13 10:00"))
            .unwrap()
            .is_some());
    }
}
