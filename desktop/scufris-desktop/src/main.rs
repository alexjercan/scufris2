//! scufris-desktop: the Scufris voice pill and tray companion.
//!
//! The companion owns activation, the microphone, local transcription,
//! transcript review, and health presentation. It never owns the conversation:
//! accepted transcripts go to the Scufris daemon as ordinary user messages over
//! the control socket, and the daemon stays the only writer of session files.

mod app;
mod audio;
mod blob;
mod config;
mod daemon;
mod display;
mod focus;
mod logging;
mod pending;
mod pill;
mod review;
mod state;
mod stt;
mod tray;
mod widgets;

use std::{
    env,
    error::Error,
    os::unix::process::CommandExt,
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

use app::{
    ACK_TIMEOUT, App, Backend, COPY_EVENT, CUES_EVENT, Executor, Hidden, PRESENTATION_EVENT, Ports,
    PresentationPayload, Shown, Surface, TICK_EVENT, ThreadExecutor, TickPayload, Transcriber,
};
use audio::{CpalRecorder, Recorder};
use config::Config;
use daemon::{DaemonEvent, DaemonLink};
use focus::FocusTracker;
use pending::{FilePendingStore, PendingStore};
use state::Event;
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent, http, ipc::Channel, menu::Menu};
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
        // Only when the pill does not already hold the keyboard. Capturing
        // then would record the pill itself as the window to go back to. A
        // pill that is up but passive is fine to capture over: the active
        // window is the person's own, and that is where focus must return.
        if !pill::focused(&self.handle) {
            // Never a window of the companion's own. The window manager names
            // the transcript box as the active window for as long as the box is
            // up, and the pill can be shown again with the box on screen: the
            // watch does exactly that when something takes the keyboard away
            // mid-review.
            self.focus.capture(&self.windows());
        }
        pill::show(&self.handle)
    }

    fn show_pill_passive(&self) -> Result<Shown, String> {
        pill::show_passive(&self.handle)
    }

    fn hide_pill(&self) -> Result<Hidden, String> {
        // The box belongs to the orb, so it goes first: a pill on its way down
        // must never leave a transcript hanging over the desktop behind it.
        if let Err(error) = review::hide(&self.handle) {
            warn!("{error}");
        }
        pill::hide(&self.handle)
    }

    fn pill_has_keyboard(&self) -> bool {
        pill::focused(&self.handle)
    }

    fn nobody_has_the_keyboard(&self) -> bool {
        // The display's own answer, or the one window of ours that would be a
        // dead end for every key: the box refuses the keyboard and has no key
        // handlers, so a window manager that forces focus onto it has taken the
        // keys as surely as a window manager that focused nothing.
        display::nobody_holds_the_keyboard() == display::Verdict::Yes
            || review::holds_the_keyboard(&self.handle)
    }

    fn restore_focus(&self) -> Result<(), String> {
        self.focus.restore()
    }

    fn present(&self, payload: PresentationPayload) -> Result<(), String> {
        // The box is raised before the presentation is emitted, so the page it
        // renders is already on screen when the words reach it. A box that
        // would not come up is a warning and not a refusal: the orb is what the
        // runtime rests on, and failing here would keep the words from reaching
        // it too.
        if let Err(error) = review::follow(&self.handle, payload.state) {
            warn!("{error}");
        }
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

impl DesktopSurface {
    /// The windows the companion put on the display, as far as it has made any.
    fn windows(&self) -> Vec<u32> {
        [pill::known_window(), review::known_window()]
            .into_iter()
            .flatten()
            .collect()
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
    // This thread runs the event loop from here on, and the event loop is what
    // carries every window request out. Anything asking a window about itself
    // has to know which thread that is, because on this one a request cannot
    // have happened yet.
    display::runs_the_event_loop();
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
        // The widget modules, served to the window that is holding them and to
        // nothing else. `webview_label` is what makes that true: the page asks
        // for one address, and what comes back depends on who asked.
        .register_uri_scheme_protocol("scufris-widget", |ctx, _request| {
            let label = ctx.webview_label().to_string();
            let module = ctx
                .app_handle()
                .try_state::<Arc<widgets::Widgets>>()
                .and_then(|widgets| widgets.module(&label));
            match module {
                Some(script) => http::Response::builder()
                    .header(http::header::CONTENT_TYPE, "text/javascript")
                    // The shell page is served from the app's own origin and
                    // the module from this scheme, so the import is a
                    // cross-origin one and the browser asks before it runs it.
                    .header(http::header::ACCESS_CONTROL_ALLOW_ORIGIN, "*")
                    .body(script.into_bytes())
                    .unwrap_or_else(|_| refused()),
                None => refused(),
            }
        })
        .invoke_handler(tauri::generate_handler![
            pill_ready,
            review_ready,
            pill_submit,
            pill_cancel,
            pill_copy,
            pill_cues,
            pill_log,
            widget_shell_ready,
            widget_tick
        ])
        .setup(move |tauri| {
            let handle = tauri.handle().clone();
            pill::ensure(&handle)?;
            review::ensure(&handle)?;

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
                        // Off this thread. An activation waits for the pill to
                        // be on screen before the microphone opens, and this is
                        // the thread that would have to put it there.
                        tray::MENU_VOICE => {
                            thread::spawn(move || runtime.handle(Event::Activate));
                        }
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

            let widgets = widgets::Widgets::start(handle.clone())?;
            tauri.manage(Arc::clone(&widgets));

            let observer = Arc::clone(&runtime);
            let surfaces = Arc::clone(&widgets);
            let link = Arc::new(DaemonLink::start(
                config.socket.clone(),
                move |event| match event {
                    // Routed away from the pill before it can reach the state
                    // machine. The runtime is the pill's sibling, and a widget
                    // command has nothing to say about a conversation.
                    DaemonEvent::Widget(command) => surfaces.command(command),
                    // The catalog goes out once per connection, as soon as
                    // there is a daemon to read it: it is what the daemon types
                    // its widget tools from.
                    DaemonEvent::Connected(session) => {
                        surfaces.announce();
                        observer.observe(DaemonEvent::Connected(session));
                    }
                    event => observer.observe(event),
                },
            ));
            widgets.attach(Arc::clone(&link));
            runtime.set_backend(Arc::clone(&link) as Arc<dyn Backend>);
            tauri.manage(link);

            tauri.global_shortcut().register(shortcut)?;
            Ok(())
        })
        .build(tauri::generate_context!())?;

    tauri_app.run(|handle, event| match event {
        RunEvent::Ready => {
            display::the_event_loop_is_running();
            // Recovers an accepted transcript the previous process could not
            // deliver, then renders the first presentation. Here rather than in
            // `setup`, and on a thread of its own, because the recovered words
            // go into a phase the person has to see: the decision waits for the
            // pill to be put on screen, and this loop is what puts it there.
            // Made from inside `setup` it asked a window that could not yet
            // exist and threw the words away on the answer.
            let runtime = handle.state::<Arc<App>>().inner().clone();
            thread::spawn(move || runtime.start());
        }
        // Something outside the companion asked a widget window to close - an
        // i3 kill, a window manager clean-up. The runtime is told, so what the
        // conversation believes is on screen stays what is on screen.
        RunEvent::WindowEvent {
            label,
            event: WindowEvent::CloseRequested { .. },
            ..
        } if widgets::is_shell(&label) => {
            handle.state::<Arc<widgets::Widgets>>().dismissed(label);
        }
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

/// The page saying hello, and saying with it whether the person asked for
/// reduced motion. Only the page can read that preference, and the window's
/// half of the entrance is the host's to run, so it travels with the first
/// word the page says.
#[tauri::command]
fn pill_ready(runtime: tauri::State<'_, Arc<App>>, reduced_motion: bool) {
    pill::set_reduced_motion(reduced_motion);
    runtime.inner().clone().publish();
}

/// The transcript page saying hello, and asking what it has missed.
///
/// A transcript recovered from a previous process is published as the companion
/// starts, which can be before this window's page is listening. Rather than let
/// the box come up empty over words nobody can read, the page asks for the
/// presentation again once it is ready for one.
#[tauri::command]
fn review_ready(runtime: tauri::State<'_, Arc<App>>) {
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

/// One shell page saying hello, and handing over the channel it listens on.
///
/// The channel is the whole host-to-page contract, and it is the page that
/// makes it: a channel is a callback in the webview, so only the webview can
/// hand one out. Until this arrives the window is built but deaf, which is why
/// the pool counts a shell as warm only from here.
#[tauri::command]
fn widget_shell_ready(
    widgets: tauri::State<'_, Arc<widgets::Widgets>>,
    window: tauri::Window,
    channel: Channel<widgets::pool::ShellMsg>,
) {
    widgets.ready(window.label().to_string(), channel);
}

/// One of the two chrome ticks, from the window it was clicked in.
///
/// The window says which surface this is; the page never does. A page that
/// named its own surface could name another window's.
#[tauri::command]
fn widget_tick(
    widgets: tauri::State<'_, Arc<widgets::Widgets>>,
    window: tauri::Window,
    kind: String,
) {
    let surface = window.label().to_string();
    match kind.as_str() {
        "close" => widgets.dismissed(surface),
        "pin" => widgets.pinned(surface),
        unknown => debug!(kind = %unknown, "a widget window reported an unknown tick"),
    }
}

/// The answer for a widget module request nothing can serve.
///
/// A page that gets this draws what it could not load, which is the one thing
/// worth seeing in a window that would otherwise be blank.
fn refused() -> http::Response<Vec<u8>> {
    http::Response::builder()
        .status(http::StatusCode::NOT_FOUND)
        .body(Vec::new())
        .expect("a bodyless response is well formed")
}
