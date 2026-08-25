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
mod pending;
mod pill;
mod state;
mod stt;
mod tray;

use std::{env, error::Error, os::unix::process::CommandExt, process::ExitCode, sync::Arc};

use app::{
    ACK_TIMEOUT, App, Backend, COPY_EVENT, Executor, PRESENTATION_EVENT, Ports,
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

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("scufris-desktop: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn Error>> {
    let arguments: Vec<String> = env::args().skip(1).collect();
    match arguments.first().map(String::as_str) {
        Some("--version") => {
            println!("scufris-desktop {}", env!("CARGO_PKG_VERSION"));
            return Ok(());
        }
        Some("--print-config") => {
            print!("{}", Config::from_env()?.describe());
            return Ok(());
        }
        Some(unknown) => return Err(format!("unknown argument {unknown}").into()),
        None => {}
    }
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
            pill_copy
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
            tray::install(
                &handle,
                &menu,
                |app, id| {
                    let runtime = app.state::<Arc<App>>().inner().clone();
                    match id {
                        tray::MENU_CHAT => runtime.open_chat(),
                        tray::MENU_VOICE => runtime.handle(Event::Activate),
                        tray::MENU_RESTART => runtime.restart_backend(),
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
