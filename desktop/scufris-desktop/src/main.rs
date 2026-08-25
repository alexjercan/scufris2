//! scufris-desktop: the Scufris voice pill and tray companion.
//!
//! The companion owns activation, the microphone, local transcription,
//! transcript review, and health presentation. It never owns the conversation:
//! accepted transcripts go to the Scufris daemon as ordinary user messages over
//! the control socket, and the daemon stays the only writer of session files.

mod app;
mod audio;
mod config;
mod daemon;
mod focus;
mod logging;
mod pending;
mod pill;
mod state;
mod stt;
mod tray;

use std::{
    env,
    error::Error,
    os::unix::process::CommandExt,
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

use app::{
    ACK_TIMEOUT, App, Backend, COPY_EVENT, CUES_EVENT, Executor, PRESENTATION_EVENT, Ports,
    PresentationPayload, Shown, Surface, TICK_EVENT, ThreadExecutor, TickPayload, Transcriber,
};
use audio::{CpalRecorder, Recorder};
use config::Config;
use daemon::DaemonLink;
use focus::FocusTracker;
use pending::{FilePendingStore, PendingStore};
use state::Event;
use tauri::{AppHandle, Emitter, Manager, RunEvent, menu::Menu};
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut, ShortcutState};
use tracing::{debug, error, info, warn};

fn main() -> ExitCode {
    match run() {
        Ok(code) => code,
        // Only what fails before logging is up lands here: a bad argument, or
        // logging itself. Stderr is all there is for those.
        Err(error) => {
            eprintln!("scufris-desktop: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<ExitCode, Box<dyn Error>> {
    let mut foreground = false;
    for argument in env::args().skip(1) {
        match argument.as_str() {
            "--version" => {
                println!("scufris-desktop {}", env!("CARGO_PKG_VERSION"));
                return Ok(ExitCode::SUCCESS);
            }
            "--print-config" => {
                print!("{}", Config::from_env()?.describe());
                return Ok(ExitCode::SUCCESS);
            }
            "--foreground" => foreground = true,
            unknown => return Err(format!("unknown argument {unknown}").into()),
        }
    }
    logging::init(foreground)?;
    match serve() {
        Ok(()) => Ok(ExitCode::SUCCESS),
        Err(error) => {
            error!("{error}");
            Ok(ExitCode::FAILURE)
        }
    }
}

fn serve() -> Result<(), Box<dyn Error>> {
    let config = Config::from_env()?;
    configure_webkit_renderer()?;
    start(config)
}

/// WebKitGTK otherwise renders blank windows when GBM allocation is unavailable.
fn configure_webkit_renderer() -> Result<(), Box<dyn Error>> {
    if env::var_os("WEBKIT_DISABLE_DMABUF_RENDERER").is_some() {
        return Ok(());
    }
    let executable = env::current_exe()?;
    let error = std::process::Command::new(executable)
        .args(env::args_os().skip(1))
        .env("WEBKIT_DISABLE_DMABUF_RENDERER", "1")
        .exec();
    Err(error.into())
}

/// Whether the four boundary earcons play. Session-scoped: every start ships
/// them enabled, the tray menu mutes them.
struct CueSwitch(AtomicBool);

/// The pill window, the tray, and focus restoration.
struct DesktopSurface {
    handle: AppHandle,
    menu: Menu<tauri::Wry>,
    focus: FocusTracker,
}

impl Surface for DesktopSurface {
    fn show_pill(&self) -> Result<Shown, String> {
        // Only when the pill is not already up. Capturing again while it holds
        // focus would record the pill itself as the window to go back to.
        if !pill::visible(&self.handle) {
            self.focus.capture();
        }
        pill::show(&self.handle)
    }

    fn hide_pill(&self) -> Result<(), String> {
        pill::hide(&self.handle)
    }

    fn restore_focus(&self) -> Result<(), String> {
        self.focus.restore()
    }

    fn present(&self, payload: PresentationPayload) -> Result<(), String> {
        self.handle
            .emit(PRESENTATION_EVENT, payload)
            .map_err(|error| format!("the pill would not render: {error}"))
    }

    fn tick(&self, payload: TickPayload) {
        let _ = self.handle.emit(TICK_EVENT, payload);
    }

    fn tray(&self, state: &str, detail: &str) -> Result<(), String> {
        tray::apply(&self.handle, &self.menu, state, detail)
    }

    fn copy(&self, text: String) {
        // The webview owns the clipboard.
        let _ = self.handle.emit(COPY_EVENT, text);
    }
}

/// Local transcription through the configured whisper-server endpoint.
struct HttpTranscriber {
    endpoint: String,
}

impl Transcriber for HttpTranscriber {
    fn transcribe(&self, wav: Vec<u8>) -> Result<String, String> {
        stt::transcribe(&self.endpoint, &wav).map_err(|error| error.to_string())
    }
}

impl Backend for DaemonLink {
    fn submit(&self, id: String, text: String, force: bool) -> Result<(), String> {
        DaemonLink::submit(self, id, text, force)
    }
}

fn start(config: Config) -> Result<(), Box<dyn Error>> {
    // After the WebKit re-exec, so one start logs one starting line.
    info!(
        version = env!("CARGO_PKG_VERSION"),
        socket = %config.socket.display(),
        stt = %config.stt_endpoint,
        hotkey = %config.hotkey,
        "starting"
    );
    let shortcut: Shortcut = config
        .hotkey
        .parse()
        .map_err(|_| format!("{} is not a usable accelerator", config.hotkey))?;
    let chat_available = config.chat_command.is_some();
    let restart_available = config.restart_command.is_some();

    let tauri_app = tauri::Builder::default()
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, _shortcut, event| {
                    if event.state() == ShortcutState::Pressed {
                        app.state::<Arc<App>>()
                            .inner()
                            .clone()
                            .handle(Event::Activate);
                    }
                })
                .build(),
        )
        .invoke_handler(tauri::generate_handler![
            pill_ready,
            pill_submit,
            pill_cancel,
            pill_copy,
            pill_cues,
            pill_log
        ])
        .setup(move |tauri| {
            let handle = tauri.handle().clone();
            pill::ensure(&handle)?;

            let menu = tray::build_menu(
                &handle,
                chat_available,
                restart_available,
                &app::status_line("disconnected", "The Scufris backend is unavailable."),
            )?;
            tauri.manage(CueSwitch(AtomicBool::new(true)));
            let cue_menu = menu.clone();
            tray::install(
                &handle,
                &menu,
                move |app, id| {
                    let runtime = app.state::<Arc<App>>().inner().clone();
                    match id {
                        tray::MENU_CHAT => runtime.open_chat(),
                        tray::MENU_VOICE => runtime.handle(Event::Activate),
                        tray::MENU_RESTART => runtime.restart_backend(),
                        tray::MENU_CUES => {
                            let enabled =
                                !app.state::<CueSwitch>().0.fetch_xor(true, Ordering::AcqRel);
                            if let Err(error) = tray::set_cues_label(&cue_menu, enabled) {
                                warn!("{error}");
                            }
                            let _ = app.emit(CUES_EVENT, enabled);
                            info!(enabled, "sound cues");
                        }
                        tray::MENU_QUIT => app.exit(0),
                        _ => {}
                    }
                },
                |app| app.state::<Arc<App>>().open_chat(),
            )?;

            let runtime = Arc::new(App::new(Ports {
                surface: Arc::new(DesktopSurface {
                    handle: handle.clone(),
                    menu,
                    focus: FocusTracker::new(),
                }) as Arc<dyn Surface>,
                recorder: Arc::new(CpalRecorder) as Arc<dyn Recorder>,
                pending: Arc::new(FilePendingStore::new(config.state_file.clone()))
                    as Arc<dyn PendingStore>,
                transcriber: Arc::new(HttpTranscriber {
                    endpoint: config.stt_endpoint.clone(),
                }) as Arc<dyn Transcriber>,
                executor: Arc::new(ThreadExecutor) as Arc<dyn Executor>,
                prefix: App::process_prefix(),
                chat_command: config.chat_command.clone(),
                restart_command: config.restart_command.clone(),
                ack_timeout: ACK_TIMEOUT,
            }));
            tauri.manage(Arc::clone(&runtime));

            let observer = Arc::clone(&runtime);
            let link = Arc::new(DaemonLink::start(config.socket.clone(), move |event| {
                observer.observe(event)
            }));
            runtime.set_backend(Arc::clone(&link) as Arc<dyn Backend>);
            tauri.manage(link);

            // Recovers an accepted transcript the previous process could not
            // deliver, then renders the first presentation.
            runtime.start();

            tauri.global_shortcut().register(shortcut)?;
            Ok(())
        })
        .build(tauri::generate_context!())?;

    tauri_app.run(|handle, event| match event {
        RunEvent::ExitRequested { api, code, .. } if code.is_none() => api.prevent_exit(),
        RunEvent::Exit => {
            info!("stopping");
            handle.state::<Arc<DaemonLink>>().stop();
            handle.state::<Arc<App>>().shutdown();
        }
        _ => {}
    });
    Ok(())
}

#[tauri::command]
fn pill_ready(runtime: tauri::State<'_, Arc<App>>) {
    runtime.inner().clone().publish();
}

#[tauri::command]
fn pill_submit(runtime: tauri::State<'_, Arc<App>>, text: Option<String>) {
    runtime.inner().clone().handle(Event::Enter { text });
}

#[tauri::command]
fn pill_cancel(runtime: tauri::State<'_, Arc<App>>) {
    runtime.inner().clone().handle(Event::Escape);
}

#[tauri::command]
fn pill_copy(runtime: tauri::State<'_, Arc<App>>) {
    runtime.inner().clone().handle(Event::Copy);
}

/// The current sound cue enablement, asked for once when the webview loads.
/// Later changes arrive over the cues event.
#[tauri::command]
fn pill_cues(cues: tauri::State<'_, CueSwitch>) -> bool {
    cues.0.load(Ordering::Acquire)
}

/// Webview console output, forwarded so pill behaviour is visible from
/// journalctl. Everything arrives at DEBUG; the console level rides along as a
/// field so it stays filterable.
#[tauri::command]
fn pill_log(level: String, message: String) {
    debug!(target: "webview", level = %level, "{message}");
}
