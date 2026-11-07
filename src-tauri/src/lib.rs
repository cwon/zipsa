mod models;
mod schedule;
mod store;
mod updates;

use std::{
    fs,
    io::{Read, Write},
    path::{Path, PathBuf},
    sync::Mutex,
    time::Duration,
};

use chrono::Utc;
use models::*;
use serde::Serialize;
use tauri::{
    menu::{Menu, MenuItem},
    tray::TrayIconBuilder,
    Manager, State,
};
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
use tauri_plugin_notification::NotificationExt;
use tauri_plugin_opener::OpenerExt;

type Db = Mutex<store::Store>;
type Result<T> = std::result::Result<T, String>;

fn lock(db: &Db) -> Result<std::sync::MutexGuard<'_, store::Store>> {
    db.lock()
        .map_err(|_| "저장소를 열지 못했습니다. 앱을 다시 실행해 주세요.".into())
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    version: String,
    data_path: String,
}

#[tauri::command]
fn app_info(app: tauri::AppHandle) -> Result<AppInfo> {
    Ok(AppInfo {
        version: app.package_info().version.to_string(),
        data_path: app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .display()
            .to_string(),
    })
}

#[tauri::command]
async fn check_latest_release(app: tauri::AppHandle) -> Result<updates::ReleaseCheck> {
    updates::check(&app.package_info().version.to_string()).await
}

#[tauri::command]
fn open_download_page(app: tauri::AppHandle, latest: bool) -> Result<()> {
    // Only fixed project URLs can be opened; no URL is accepted from the webview.
    app.opener()
        .open_url(
            if latest {
                updates::LATEST_RELEASE_PAGE
            } else {
                updates::RELEASES_PAGE
            },
            None::<&str>,
        )
        .map_err(|_| "브라우저를 열지 못했어요. 기본 브라우저 설정을 확인해 주세요.".to_string())
}

#[tauri::command]
fn load_data(db: State<'_, Db>) -> Result<Snapshot> {
    lock(&db)?.snapshot()
}
#[tauri::command]
fn save_space(db: State<'_, Db>, space: Space) -> Result<Snapshot> {
    lock(&db)?.save_space(space)
}
#[tauri::command]
fn save_item(db: State<'_, Db>, item: Item) -> Result<Snapshot> {
    lock(&db)?.save_item(item)
}
#[tauri::command]
fn save_task(db: State<'_, Db>, task: Task) -> Result<Snapshot> {
    lock(&db)?.save_task(task)
}
#[tauri::command]
fn delete_entity(db: State<'_, Db>, kind: String, id: String) -> Result<Snapshot> {
    lock(&db)?.delete(&kind, &id)
}
#[tauri::command]
fn finish_task(
    db: State<'_, Db>,
    id: String,
    expected_due: String,
    outcome: String,
) -> Result<Snapshot> {
    lock(&db)?.finish(&id, &expected_due, &outcome)
}
#[tauri::command]
fn reopen_task(db: State<'_, Db>, id: String, record_id: String) -> Result<Snapshot> {
    lock(&db)?.reopen(&id, &record_id)
}
#[tauri::command]
fn save_settings(db: State<'_, Db>, settings: Settings) -> Result<Snapshot> {
    lock(&db)?.set_settings(settings)
}

fn write_backup(path: &Path, data: Snapshot) -> Result<()> {
    let backup = Backup {
        format: "zipsa-backup".into(),
        version: 1,
        exported_at: Utc::now().to_rfc3339(),
        data,
    };
    let parent = path.parent().ok_or("백업 경로가 올바르지 않습니다.")?;
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    serde_json::to_writer(file.as_file_mut(), &backup).map_err(|e| e.to_string())?;
    file.flush().map_err(|e| e.to_string())?;
    if file.as_file().metadata().map_err(|e| e.to_string())?.len() > 256 * 1024 * 1024 {
        return Err(
            "백업 크기가 256MB를 넘었습니다. 사진 용량을 줄인 뒤 다시 시도해 주세요.".into(),
        );
    }
    file.as_file().sync_all().map_err(|e| e.to_string())?;
    file.persist(path).map_err(|e| e.to_string())?;
    Ok(())
}

fn read_backup(path: &Path) -> Result<Backup> {
    const MAX: u64 = 256 * 1024 * 1024;
    let file = fs::File::open(path).map_err(|e| e.to_string())?;
    if file.metadata().map_err(|e| e.to_string())?.len() > MAX {
        return Err("백업 파일은 256MB 이하여야 합니다.".into());
    }
    let mut bytes = Vec::new();
    file.take(MAX + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX {
        return Err("백업 파일이 너무 큽니다.".into());
    }
    let backup: Backup = serde_json::from_slice(&bytes)
        .map_err(|_| "집사 백업 파일을 읽지 못했습니다. 올바른 .zipsa 파일인지 확인해 주세요.")?;
    if backup.format != "zipsa-backup" || backup.version != 1 {
        return Err("지원하지 않는 백업 형식입니다.".into());
    }
    store::validate(&backup.data)?;
    Ok(backup)
}

#[tauri::command]
async fn export_backup(app: tauri::AppHandle) -> Result<Option<String>> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = app
            .dialog()
            .file()
            .add_filter("집사 백업", &["zipsa"])
            .set_file_name(format!("Zipsa-{}.zipsa", Utc::now().format("%Y-%m-%d")))
            .blocking_save_file();
        let Some(path) = path else {
            return Ok(None);
        };
        let path = path.into_path().map_err(|e| e.to_string())?;
        let snapshot = lock(&app.state::<Db>())?.snapshot()?;
        write_backup(&path, snapshot)?;
        Ok(Some(path.display().to_string()))
    })
    .await
    .map_err(|e| e.to_string())?
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct RestoreResult {
    data: Snapshot,
    recovery_path: String,
}

#[tauri::command]
async fn import_backup(app: tauri::AppHandle) -> Result<Option<RestoreResult>> {
    tauri::async_runtime::spawn_blocking(move || {
        let Some(path) = app.dialog().file().add_filter("집사 백업", &["zipsa"]).blocking_pick_file() else { return Ok(None); };
        let path = path.into_path().map_err(|e| e.to_string())?;
        let backup = read_backup(&path)?;
        let confirmed = app.dialog().message(format!("공간 {}개, 아이템 {}개, 할 일 {}개를 복원합니다.\n현재 데이터는 이 백업의 내용으로 교체됩니다.\n교체 전 데이터는 자동으로 별도 백업합니다.", backup.data.spaces.len(), backup.data.items.len(), backup.data.tasks.len()))
            .title("집사 백업 복원").kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom("복원".into(), "취소".into())).blocking_show();
        if !confirmed { return Ok(None); }
        let dir = app.path().app_data_dir().map_err(|e| e.to_string())?.join("backups");
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let recovery: PathBuf = dir.join(format!("before-restore-{}.zipsa", uuid::Uuid::new_v4()));
        let state = app.state::<Db>(); let mut db = lock(&state)?;
        write_backup(&recovery, db.snapshot()?)?;
        let data = db.restore(&backup.data)?;
        Ok(Some(RestoreResult { data, recovery_path: recovery.display().to_string() }))
    }).await.map_err(|e| e.to_string())?
}

fn show_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

fn start_reminders(app: tauri::AppHandle) {
    std::thread::spawn(move || loop {
        let state = app.state::<Db>();
        if let Ok(mut db) = lock(&state) {
            if let Ok(due) = db.reminders() {
                if !due.is_empty() {
                    let body = if due.len() == 1 {
                        format!("{} · 예정된 할 일을 확인해 주세요.", due[0].title)
                    } else {
                        format!(
                            "{} 외 {}개의 할 일이 기다리고 있어요.",
                            due[0].title,
                            due.len() - 1
                        )
                    };
                    if app
                        .notification()
                        .builder()
                        .title("집사 · 할 일 알림")
                        .body(body)
                        .show()
                        .is_ok()
                    {
                        for task in &due {
                            let _ = db.mark_notified(task);
                        }
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_secs(30));
    });
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            show_main(app)
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--background"]),
        ))
        .invoke_handler(tauri::generate_handler![
            app_info,
            check_latest_release,
            open_download_page,
            load_data,
            save_space,
            save_item,
            save_task,
            delete_entity,
            finish_task,
            reopen_task,
            save_settings,
            export_backup,
            import_backup
        ])
        .setup(|app| {
            let dir = app.path().app_data_dir()?;
            fs::create_dir_all(&dir)?;
            app.manage(Mutex::new(store::Store::open(&dir.join("zipsa.sqlite"))?));
            let open = MenuItem::with_id(app, "open", "집사 열기", true, None::<&str>)?;
            let quit = MenuItem::with_id(app, "quit", "집사 종료", true, None::<&str>)?;
            let menu = Menu::with_items(app, &[&open, &quit])?;
            let mut tray = TrayIconBuilder::new()
                .tooltip("집사 · Zipsa")
                .menu(&menu)
                .on_menu_event(|app, event| match event.id.as_ref() {
                    "open" => show_main(app),
                    "quit" => app.exit(0),
                    _ => {}
                });
            if let Some(icon) = app.default_window_icon() {
                tray = tray.icon(icon.clone());
            }
            tray.build(app)?;
            if let Some(window) = app.get_webview_window("main") {
                let handle = window.clone();
                window.on_window_event(move |event| {
                    if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                        api.prevent_close();
                        let _ = handle.hide();
                    }
                });
                if std::env::args().any(|arg| arg == "--background") {
                    let _ = window.hide();
                }
            }
            start_reminders(app.handle().clone());
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("집사를 시작하지 못했습니다");
    app.run(|app, event| {
        #[cfg(target_os = "macos")]
        if let tauri::RunEvent::Reopen { .. } = event {
            show_main(app);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = (app, event);
    });
}

#[cfg(test)]
mod backup_tests {
    use super::*;

    #[test]
    fn backup_round_trip_is_portable_and_wrong_formats_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("backup.zipsa");
        let data = Snapshot {
            spaces: vec![Space {
                id: "home".into(),
                parent_id: None,
                name: "우리 집".into(),
                icon: "🏠".into(),
                notes: "로컬 백업".into(),
            }],
            ..Default::default()
        };
        write_backup(&path, data).unwrap();
        assert_eq!(read_backup(&path).unwrap().data.spaces[0].name, "우리 집");
        fs::write(&path, r#"{"format":"other-app","version":1,"exportedAt":"2026-01-01","data":{"spaces":[],"items":[],"tasks":[],"records":[],"settings":{"notifications":false}}}"#).unwrap();
        assert!(read_backup(&path).is_err());
    }
}
