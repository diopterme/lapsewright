// No console window next to the app on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod convert;
mod finish;
mod folder;
mod lang;
mod prefs;
mod presets;
mod session;

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use jiff::Zoned;
use lapsewright_core::{binaries, metadata, source};
use lapsewright_core::preview::{self, PreviewError};
use lapsewright_core::runner::{Canceller, RunError};
use lapsewright_core::settings::{MotionBlur, Settings};
use lapsewright_core::store::Store;
use prefs::{Power, Prefs};
use session::Session;
use lang::t;
use tauri::ipc::{Channel, Response};
use tauri::menu::Menu;
#[cfg(target_os = "macos")]
use tauri::menu::{MenuItem, PredefinedMenuItem};
use tauri::window::{ProgressBarState, ProgressBarStatus};
use tauri::{AppHandle, Emitter, Manager, RunEvent, State, WebviewUrl, WebviewWindowBuilder, Window, WindowEvent, Wry};

const QUIT_MENU: &str = "quit";
const SETTINGS_MENU: &str = "settings";
const MAIN_WINDOW: &str = "main";
const SETTINGS_WINDOW: &str = "settings";
/// The settings window changed something; the main window shows author and copyright (M3).
const PREFS_CHANGED: &str = "prefs-changed";
const RECENT_CLEARED: &str = "recent-cleared";
const LINKS: [&str; 2] = ["https://github.com/diopterme/", "https://ffmpeg.org/"];
/// V5: asks the window whether to stop the conversion and quit.
const QUIT_REQUESTED: &str = "quit-requested";

struct App {
    ffmpeg: Result<PathBuf, String>,
    /// The app data folder; `None` when the system reports no home folder.
    data: Option<PathBuf>,
    /// What the panel shows; previews use it.
    settings: Mutex<Settings>,
    images: Mutex<Arc<Vec<PathBuf>>>,
    /// Latest blurred and original preview: a newer request stops the older one of its kind.
    running: Mutex<[Option<Canceller>; 2]>,
    /// The conversion running now; the window keeps the queue and starts one at a time.
    conversion: Mutex<Option<Running>>,
    prefs: Mutex<Prefs>,
}

#[derive(Default)]
struct Running {
    /// `None` until ffmpeg has started.
    canceller: Option<Canceller>,
    file: Option<PathBuf>,
    /// Asked before ffmpeg started: it is stopped as soon as it does.
    is_stopping: bool,
    is_quitting: bool,
}

impl Running {
    fn stop(&mut self) {
        self.is_stopping = true;
        if let Some(canceller) = &self.canceller {
            // Already finished is fine: the result is on its way.
            let _ = canceller.cancel();
        }
    }
}

#[tauri::command(async)]
fn open_folder(path: PathBuf, app: State<App>) -> Result<folder::Folder, String> {
    let (folder, images) = folder::open(&path)?;
    *app.images.lock().unwrap() = Arc::new(images);
    let Some(root) = &app.data else { return Ok(folder) };
    // Only the recent list and the crop are lost; the folder itself opened fine.
    match folder::remember(root, &folder.path, folder.names.len()) {
        Ok(crop) => Ok(folder::Folder { crop, ..folder }),
        Err(e) => {
            eprintln!("could not save recent folders: {e}");
            Ok(folder)
        }
    }
}

/// After a drag of the crop frame; [시작] sends the position itself, so a lost save only
/// costs the next opening.
#[tauri::command(async)]
fn set_crop(path: PathBuf, crop: u16, app: State<App>) {
    let Some(root) = &app.data else { return };
    if let Err(e) = folder::set_crop(root, &path, crop) {
        eprintln!("could not save the crop: {e}");
    }
}

#[tauri::command(async)]
fn recent_folders(app: State<App>) -> Vec<folder::Recent> {
    app.data.as_deref().map_or_else(Vec::new, folder::recent)
}

/// Y10: JPEG of a recent folder's first photo, made once and then read from the data folder.
#[tauri::command(async)]
fn recent_thumb(path: PathBuf, app: State<App>) -> Result<Response, String> {
    let root = app.data.as_deref().ok_or(t("앱 데이터 폴더가 없음", "No app data folder"))?;
    let cached = folder::thumb_file(root, &path);
    if let Ok(bytes) = fs::read(&cached) {
        return Ok(Response::new(bytes));
    }
    let ffmpeg = app.ffmpeg.clone()?;
    let images = source::scan(&path).map_err(|e| format!("{}: {e}", t("폴더를 못 읽음", "Can’t read the folder")))?;
    let first = images.into_iter().next().ok_or(t("사진이 없음", "No photos"))?;
    let settings = Settings { blur: MotionBlur::Off, ..Settings::default() };
    let bytes = preview::start(&ffmpeg, &[first], 0, &settings).and_then(preview::Preview::wait).map_err(reason)?;
    // Not kept: it is made again next time.
    if let Err(e) = cached.parent().map_or(Ok(()), fs::create_dir_all).and_then(|()| fs::write(&cached, &bytes)) {
        eprintln!("could not keep the recent folder picture: {e}");
    }
    Ok(Response::new(bytes))
}

/// The panel as it was when the app last closed.
#[tauri::command]
fn session(app: State<App>) -> Session {
    let (session, settings) = app.data.as_deref().map_or_else(|| Session::default().normalize().expect("default settings are valid"), session::load);
    *app.settings.lock().unwrap() = settings;
    session
}

/// Every panel change. Not async, so changes apply in the order they were made.
/// Returns the session with values the profile can't encode replaced (C3).
#[tauri::command]
fn set_session(session: Session, app: State<App>) -> Result<Session, String> {
    let (session, settings) = session.normalize()?;
    *app.settings.lock().unwrap() = settings;
    if let Some(root) = &app.data {
        // Only the next launch loses it.
        if let Err(e) = session::save(root, &session) {
            eprintln!("could not save the last session: {e}");
        }
    }
    Ok(session)
}

#[tauri::command(async)]
fn output_problem(dir: PathBuf) -> Option<String> {
    session::output_problem(Path::new(&dir))
}

/// JPEG bytes. `original`: without motion blur, for the hold-to-compare button.
#[tauri::command(async)]
fn preview(index: usize, original: bool, app: State<App>) -> Result<Response, String> {
    let ffmpeg = app.ffmpeg.clone()?;
    let images = app.images.lock().unwrap().clone();
    let current = *app.settings.lock().unwrap();
    let settings = if original { Settings { blur: MotionBlur::Off, ..current } } else { current };
    let job = preview::start(&ffmpeg, &images, index, &settings).map_err(reason)?;
    let older = app.running.lock().unwrap()[usize::from(original)].replace(job.canceller());
    if let Some(older) = older {
        // Already finished is fine: std does not signal a child it has reaped.
        let _ = older.cancel();
    }
    job.wait().map(Response::new).map_err(reason)
}

#[tauri::command(async)]
fn photo_taken(index: usize, app: State<App>) -> Option<String> {
    let images = app.images.lock().unwrap().clone();
    folder::taken(images.get(index)?)
}

/// Rejects with `convert::Failure`; `on_event` gets the file name, then progress.
#[tauri::command(async)]
fn convert(job: convert::Job, on_event: Channel<convert::Event>, window: Window, handle: AppHandle, app: State<App>) -> Result<convert::Done, convert::Failure> {
    let ffmpeg = app.ffmpeg.clone().map_err(convert::Failure::new)?;
    {
        let mut conversion = app.conversion.lock().unwrap();
        if conversion.is_some() {
            return Err(convert::Failure::new(t("다른 변환이 돌고 있음", "Another conversion is running")));
        }
        *conversion = Some(Running::default());
    }
    let _awake = convert::Awake::new();
    let prefs = app.prefs.lock().unwrap().clone();
    let mut frames = 0;
    let result = convert::run(
        &ffmpeg,
        &job,
        &prefs,
        |canceller| {
            if let Some(running) = app.conversion.lock().unwrap().as_mut() {
                running.canceller = Some(canceller);
                if running.is_stopping {
                    running.stop();
                }
            }
        },
        |event| {
            match &event {
                convert::Event::Started { file, frames: total, .. } => {
                    frames = *total;
                    if let Some(running) = app.conversion.lock().unwrap().as_mut() {
                        running.file = Some(file.clone());
                    }
                }
                // V3: on macOS the bar is on the Dock icon.
                convert::Event::Progress { frame, .. } => {
                    let percent = (*frame * 100).checked_div(frames as u64).map(|p| p.min(100));
                    let _ = window.set_progress_bar(ProgressBarState { status: Some(ProgressBarStatus::Normal), progress: percent });
                }
            }
            // A closed window only stops listening.
            let _ = on_event.send(event);
        },
    );
    let _ = window.set_progress_bar(ProgressBarState { status: Some(ProgressBarStatus::None), progress: None });
    let finished = app.conversion.lock().unwrap().take();
    if finished.is_some_and(|r| r.is_quitting) {
        handle.exit(0);
        return result;
    }
    // F1: cancelling was the user's own doing.
    match &result {
        Ok(done) => finish::notify(t("영상 완성", "Video Done"), &file_name(&done.file), true),
        Err(failure) if !failure.cancelled => finish::notify(t("변환 실패", "Conversion Failed"), &format!("{}: {}", file_name(&job.folder), failure.message), false),
        Err(_) => {}
    }
    result
}

fn file_name(path: &Path) -> String {
    path.file_name().map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}

/// [취소]: the running conversion ends with a cancelled `Failure`.
#[tauri::command]
fn cancel_convert(app: State<App>) {
    if let Some(running) = app.conversion.lock().unwrap().as_mut() {
        running.stop();
    }
}

/// V5 answered "멈추고 끄기": quits once the partial video is deleted.
#[tauri::command]
fn quit(handle: AppHandle, app: State<App>) {
    let mut conversion = app.conversion.lock().unwrap();
    match conversion.as_mut() {
        Some(running) => {
            running.is_quitting = true;
            running.stop();
        }
        None => {
            drop(conversion);
            handle.exit(0);
        }
    }
}

fn is_converting(handle: &AppHandle) -> bool {
    handle.state::<App>().conversion.lock().unwrap().is_some()
}

#[tauri::command]
fn ffmpeg_problem(app: State<App>) -> Option<String> {
    app.ffmpeg.as_ref().err().map(|_| t("ffmpeg 를 찾을 수 없음 → 앱을 다시 설치", "ffmpeg is missing → reinstall the app").to_string())
}

#[tauri::command(async)]
fn target(dir: PathBuf, name: String, extension: String) -> convert::Target {
    convert::target(&dir, &name, &extension)
}

#[tauri::command(async)]
fn reveal(path: PathBuf) -> Result<(), String> {
    convert::show(&path, true)
}

#[tauri::command(async)]
fn open_video(path: PathBuf) -> Result<(), String> {
    convert::show(&path, false)
}

/// `"ko"` or `"en"`, as the menus and messages use.
#[tauri::command]
fn lang() -> &'static str {
    if lang::is_ko() { "ko" } else { "en" }
}

/// Settings › About: the source, sponsor and ffmpeg pages in the browser.
#[tauri::command(async)]
fn open_link(url: String) -> Result<(), String> {
    if !LINKS.iter().any(|link| url.starts_with(link)) {
        return Err(format!("not an allowed link: {url}"));
    }
    #[cfg(target_os = "windows")]
    let mut open = std::process::Command::new("explorer");
    #[cfg(not(target_os = "windows"))]
    let mut open = std::process::Command::new("/usr/bin/open");
    // explorer exits 1 even when the browser opens.
    open.arg(&url).spawn().map(drop).map_err(|e| e.to_string())
}

#[tauri::command]
fn prefs(app: State<App>) -> Prefs {
    app.prefs.lock().unwrap().clone()
}

/// W1: applies at once; the next conversion uses it.
#[tauri::command]
fn set_prefs(prefs: Prefs, handle: AppHandle, app: State<App>) -> Result<(), String> {
    *app.prefs.lock().unwrap() = prefs.clone();
    let _ = handle.emit(PREFS_CHANGED, &prefs);
    let root = app.data.as_deref().ok_or(t("앱 데이터 폴더가 없음 → 다음에 켜면 잊음", "No app data folder → forgotten next launch"))?;
    prefs::save(root, &prefs).map_err(|e| format!("{}: {e}", t("설정을 저장 못 함", "Couldn’t save settings")))
}

/// M1: the copyright as the video gets it; the year is the photo's, this year without one.
#[tauri::command]
fn copyright_text(holder: String) -> String {
    metadata::copyright(Zoned::now().year(), holder.trim().to_string())
}

/// W4
#[tauri::command]
fn clear_recent(handle: AppHandle, app: State<App>) -> Result<(), String> {
    if let Some(root) = &app.data {
        folder::forget(root).map_err(|e| format!("{}: {e}", t("못 지움", "Couldn’t clear")))?;
    }
    let _ = handle.emit(RECENT_CLEARED, ());
    Ok(())
}

/// Async: a window made inside a sync command deadlocks on Windows.
#[tauri::command(async)]
fn open_settings(handle: AppHandle) -> Result<(), String> {
    show_settings(&handle).map_err(|e| e.to_string())
}

#[tauri::command(async)]
fn power(power: Power) -> Result<(), String> {
    finish::power(power)
}

/// W1: one settings window; ⌘, again brings it to the front.
fn show_settings(handle: &AppHandle) -> tauri::Result<()> {
    if let Some(window) = handle.get_webview_window(SETTINGS_WINDOW) {
        return window.set_focus();
    }
    WebviewWindowBuilder::new(handle, SETTINGS_WINDOW, WebviewUrl::App("index.html".into()))
        .title(t("설정", "Settings"))
        .inner_size(480.0, 460.0)
        .resizable(false)
        .minimizable(false)
        .center()
        .build()?;
    Ok(())
}

/// The default menu, but ⌘Q asks first while converting (V5): the built-in Quit item ends
/// the app at once (tao has no `applicationShouldTerminate`).
fn menu(handle: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::default(handle)?;
    #[cfg(target_os = "macos")]
    {
        let items = menu.items()?;
        if let Some(app_menu) = items.first().and_then(|item| item.as_submenu()) {
            let quit = app_menu.items()?.len().checked_sub(1);
            if let Some(quit) = quit {
                app_menu.remove_at(quit)?;
            }
            let label = format!("Quit {}", handle.package_info().name);
            app_menu.append(&MenuItem::with_id(handle, QUIT_MENU, label, true, Some("CmdOrCtrl+Q"))?)?;
            // After About and its separator, where mac apps put it.
            let settings = MenuItem::with_id(handle, SETTINGS_MENU, t("설정…", "Settings…"), true, Some("CmdOrCtrl+,"))?;
            app_menu.insert_items(&[&settings, &PredefinedMenuItem::separator(handle)?], 2)?;
        }
    }
    Ok(menu)
}

/// Shown under "못 읽음": ffmpeg's first complaint, which names the cause.
fn reason(e: PreviewError) -> String {
    match e {
        PreviewError::Run(RunError::Failed { stderr, code }) => match stderr.first() {
            Some(line) => format!("ffmpeg: {}", strip_tags(line)),
            None => format!("{} {code:?}", t("ffmpeg 종료 코드", "ffmpeg exit code")),
        },
        other => other.to_string(),
    }
}

/// `[vist#0:0/jpegxl @ 0x78…] [dec:libjxl @ 0x78…] Decoding error` → `Decoding error`
fn strip_tags(line: &str) -> &str {
    let mut rest = line;
    while let Some((_, after)) = rest.strip_prefix('[').and_then(|r| r.split_once("] ")) {
        rest = after;
    }
    rest
}

fn main() {
    // Development has no bundled ffmpeg: LAPSEWRIGHT_FFMPEG=/opt/homebrew/bin/ffmpeg
    let ffmpeg_override = env::var_os("LAPSEWRIGHT_FFMPEG").map(PathBuf::from);
    let data = Store::default_root();
    let app = App {
        ffmpeg: binaries::locate(ffmpeg_override.as_deref()).map_err(|e| e.to_string()),
        data: data.clone(),
        settings: Mutex::new(Settings::default()),
        images: Mutex::new(Arc::new(Vec::new())),
        running: Mutex::new([None, None]),
        conversion: Mutex::new(None),
        prefs: Mutex::new(data.as_deref().map(prefs::load).unwrap_or_default()),
    };
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_window_state::Builder::new().build())
        .manage(app)
        .menu(menu)
        .on_menu_event(|handle, event| {
            if event.id() == SETTINGS_MENU {
                if let Err(e) = show_settings(handle) {
                    eprintln!("could not open the settings window: {e}");
                }
                return;
            }
            if event.id() != QUIT_MENU {
                return;
            }
            if is_converting(handle) {
                let _ = handle.emit(QUIT_REQUESTED, ());
            } else {
                handle.exit(0);
            }
        })
        .on_window_event(|window, event| {
            // The settings window just closes.
            if window.label() != MAIN_WINDOW {
                return;
            }
            if let WindowEvent::CloseRequested { api, .. } = event {
                if is_converting(window.app_handle()) {
                    api.prevent_close();
                    let _ = window.emit(QUIT_REQUESTED, ());
                } else {
                    // An open settings window would keep the app running without its main window.
                    window.app_handle().exit(0);
                }
            }
        })
        .invoke_handler(tauri::generate_handler![
            open_folder,
            set_crop,
            recent_folders,
            recent_thumb,
            session,
            set_session,
            output_problem,
            preview,
            photo_taken,
            convert,
            cancel_convert,
            quit,
            ffmpeg_problem,
            target,
            reveal,
            open_video,
            lang,
            open_link,
            presets::profiles,
            presets::presets,
            presets::save_preset,
            presets::delete_preset,
            presets::restore_built_ins,
            prefs,
            set_prefs,
            copyright_text,
            clear_recent,
            open_settings,
            power,
        ])
        .build(tauri::generate_context!())
        .expect("failed to start the window")
        .run(|handle, event| {
            // A 60 MP JXL keeps ffmpeg busy for ~10 s; don't leave it running after quitting.
            if let RunEvent::Exit = event {
                let app = handle.state::<App>();
                for job in app.running.lock().unwrap().iter().flatten() {
                    let _ = job.cancel();
                }
                // Quitting from the Dock skips the question; the half-made video still goes.
                if let Some(running) = app.conversion.lock().unwrap().as_mut() {
                    running.stop();
                    if let Some(file) = &running.file {
                        let _ = fs::remove_file(file);
                    }
                }
            }
        });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_ffmpeg_tags() {
        let line = "[vist#0:0/jpegxl @ 0x78] [dec:libjxl @ 0x78] Decoding error: Invalid data found";
        assert_eq!(strip_tags(line), "Decoding error: Invalid data found");
        assert_eq!(strip_tags("Impossible to open 'a [1] b.jxl'"), "Impossible to open 'a [1] b.jxl'");
    }
}
