mod backup;
mod calendar;
mod feedback;
mod models;
mod schedule;
mod store;
mod updates;

use std::{
    fs,
    path::PathBuf,
    sync::Mutex,
    time::{Duration, Instant},
};

use chrono::Local;
use models::*;
use serde::Serialize;
use tauri::{
    http::{
        header::{CACHE_CONTROL, CONTENT_TYPE},
        Response, StatusCode,
    },
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

fn data_dir(app: &tauri::AppHandle) -> Result<PathBuf> {
    app.path().app_data_dir().map_err(|e| e.to_string())
}

/// Photos are served to the webview from the database on request
/// (`zipsa-photo://localhost/<id>`), so data loads never carry photo bytes.
const PHOTO_SCHEME: &str = "zipsa-photo";

fn stored_photo(app: &tauri::AppHandle, id: &str) -> Option<store::Photo> {
    let state = app.try_state::<Db>()?;
    let db = lock(&state).ok()?;
    db.photo(id).ok().flatten()
}

fn photo_response(app: &tauri::AppHandle, id: &str) -> Response<Vec<u8>> {
    let response = match stored_photo(app, id) {
        // A photo id is never reused: replacing a photo creates a new id.
        Some(photo) => Response::builder()
            .header(CONTENT_TYPE, photo.mime)
            .header(CACHE_CONTROL, "max-age=31536000, immutable")
            .body(photo.data),
        None => Response::builder()
            .status(StatusCode::NOT_FOUND)
            .header(CACHE_CONTROL, "no-store")
            .body(Vec::new()),
    };
    response.unwrap_or_else(|_| Response::new(Vec::new()))
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct AppInfo {
    version: String,
    data_path: String,
    os: String,
    feedback_available: bool,
}

#[tauri::command]
fn app_info(app: tauri::AppHandle) -> Result<AppInfo> {
    let version = app.package_info().version.to_string();
    Ok(AppInfo {
        feedback_available: feedback::form_url(&version).is_ok(),
        version,
        os: feedback::platform_label().to_string(),
        data_path: app
            .path()
            .app_data_dir()
            .map_err(|e| e.to_string())?
            .display()
            .to_string(),
    })
}

#[tauri::command]
fn open_feedback_form(app: tauri::AppHandle) -> Result<()> {
    let url = feedback::form_url(&app.package_info().version.to_string())?;
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|_| "의견 접수창을 열지 못했어요. 기본 브라우저 설정을 확인해 주세요.".into())
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
fn calendar_events(db: State<'_, Db>, start: String, end: String) -> Result<Vec<CalendarEvent>> {
    let data = lock(&db)?.snapshot()?;
    calendar::events(&data, &start, &end)
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

#[tauri::command]
async fn export_backup(app: tauri::AppHandle) -> Result<Option<String>> {
    tauri::async_runtime::spawn_blocking(move || {
        let path = app
            .dialog()
            .file()
            .add_filter("집사 백업", &["zipsa"])
            .set_file_name(format!("Zipsa-{}.zipsa", Local::now().format("%Y-%m-%d")))
            .blocking_save_file();
        let Some(path) = path else {
            return Ok(None);
        };
        let path = path.into_path().map_err(|e| e.to_string())?;
        lock(&app.state::<Db>())?.backup_to(&path)?;
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
        let data_dir = data_dir(&app)?;
        let auto_dir = backup::auto_dir(&data_dir);
        let mut dialog = app.dialog().file().add_filter("집사 백업", &["zipsa"]);
        // Automatic backups live in a hidden app folder; start the picker there.
        if auto_dir.is_dir() { dialog = dialog.set_directory(&auto_dir); }
        let Some(path) = dialog.blocking_pick_file() else { return Ok(None); };
        let path = path.into_path().map_err(|e| e.to_string())?;
        let source = backup::open(&path)?;
        let summary = source.store.snapshot()?;
        store::validate(&summary)?;
        let confirmed = app.dialog().message(format!("공간 {}개, 아이템 {}개, 할 일 {}개를 복원합니다.\n현재 데이터는 이 백업의 내용으로 교체됩니다.\n교체 전 데이터는 자동으로 별도 백업합니다.", summary.spaces.len(), summary.items.len(), summary.tasks.len()))
            .title("집사 백업 복원").kind(MessageDialogKind::Warning)
            .buttons(MessageDialogButtons::OkCancelCustom("복원".into(), "취소".into())).blocking_show();
        if !confirmed { return Ok(None); }
        let dir = data_dir.join("backups");
        fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
        let recovery: PathBuf = dir.join(format!("before-restore-{}.zipsa", Local::now().format("%Y%m%d-%H%M%S")));
        let state = app.state::<Db>(); let mut db = lock(&state)?;
        db.backup_to(&recovery)?;
        let data = db.restore(&source.store)?;
        Ok(Some(RestoreResult { data, recovery_path: recovery.display().to_string() }))
    }).await.map_err(|e| e.to_string())?
}

#[tauri::command]
fn auto_backup_status(app: tauri::AppHandle) -> Result<backup::AutoBackupStatus> {
    Ok(backup::auto_status(&backup::auto_dir(&data_dir(&app)?)))
}

#[tauri::command]
fn open_backup_folder(app: tauri::AppHandle) -> Result<()> {
    let dir = backup::auto_dir(&data_dir(&app)?);
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    app.opener()
        .open_path(dir.display().to_string(), None::<&str>)
        .map_err(|_| "백업 폴더를 열지 못했어요.".into())
}

fn show_main(app: &tauri::AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.show();
        let _ = window.unminimize();
        let _ = window.set_focus();
    }
}

/// The first automatic backup check waits a minute so it never slows startup.
const AUTO_BACKUP_DELAY: Duration = Duration::from_secs(60);
const AUTO_BACKUP_CHECK: Duration = Duration::from_secs(60 * 60);

fn start_background(app: tauri::AppHandle) {
    std::thread::spawn(move || {
        let mut next_backup = Instant::now() + AUTO_BACKUP_DELAY;
        loop {
            send_reminders(&app);
            if Instant::now() >= next_backup {
                next_backup = Instant::now() + AUTO_BACKUP_CHECK;
                auto_backup(&app);
            }
            std::thread::sleep(Duration::from_secs(30));
        }
    });
}

fn auto_backup(app: &tauri::AppHandle) {
    let Ok(dir) = data_dir(app).map(|dir| backup::auto_dir(&dir)) else {
        return;
    };
    let state = app.state::<Db>();
    let Ok(mut db) = lock(&state) else {
        return;
    };
    // A failure (for example a full disk) is retried at the next check.
    if let Err(e) = backup::run_auto(&mut db, &dir, Local::now().naive_local()) {
        eprintln!("자동 백업 실패: {e}");
    }
}

fn send_reminders(app: &tauri::AppHandle) {
    let state = app.state::<Db>();
    let Ok(mut db) = lock(&state) else {
        return;
    };
    if let Ok(due) = db.reminders() {
        for kind in [store::ReminderKind::Advance, store::ReminderKind::Due] {
            let group: Vec<_> = due
                .iter()
                .filter(|reminder| reminder.kind == kind)
                .collect();
            if group.is_empty() {
                continue;
            }
            let advance = kind == store::ReminderKind::Advance;
            let body = if group.len() == 1 && advance {
                let time = schedule::parse_time(&group[0].task.due_at)
                    .map(|at| {
                        at.with_timezone(&chrono::Local)
                            .format("%m월 %d일 %H:%M")
                            .to_string()
                    })
                    .unwrap_or_default();
                format!("{} · {} 예정이에요.", group[0].task.title, time)
            } else if group.len() == 1 {
                format!("{} · 예정된 할 일을 확인해 주세요.", group[0].task.title)
            } else {
                format!(
                    "{} 외 {}개의 할 일이 {}.",
                    group[0].task.title,
                    group.len() - 1,
                    if advance {
                        "예정되어 있어요"
                    } else {
                        "기다리고 있어요"
                    }
                )
            };
            if app
                .notification()
                .builder()
                .title(if advance {
                    "집사 · 할 일 미리 알림"
                } else {
                    "집사 · 할 일 알림"
                })
                .body(body)
                .show()
                .is_ok()
            {
                for reminder in group {
                    let _ = db.mark_notified(reminder);
                }
            }
        }
    }
}

pub fn run() {
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_single_instance::init(|app, _, _| {
            show_main(app)
        }))
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .register_asynchronous_uri_scheme_protocol(PHOTO_SCHEME, |ctx, request, responder| {
            let app = ctx.app_handle().clone();
            let id = request.uri().path().trim_start_matches('/').to_owned();
            tauri::async_runtime::spawn_blocking(move || {
                responder.respond(photo_response(&app, &id));
            });
        })
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            Some(vec!["--background"]),
        ))
        .invoke_handler(tauri::generate_handler![
            app_info,
            open_feedback_form,
            check_latest_release,
            open_download_page,
            load_data,
            calendar_events,
            save_space,
            save_item,
            save_task,
            delete_entity,
            finish_task,
            reopen_task,
            save_settings,
            export_backup,
            import_backup,
            auto_backup_status,
            open_backup_folder
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
            start_background(app.handle().clone());
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
