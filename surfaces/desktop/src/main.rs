//! scufris-desktop: the Scufris voice pill and tray companion.
//!
//! The companion owns activation, the microphone, local transcription, the
//! textbox the words pass through, and health presentation. It never owns the
//! conversation: accepted transcripts go to `scufris-service` as ordinary user
//! messages over its socket, and the service stays the only writer of
//! session files.

mod app;
mod attachment;
mod audio;
mod blob;
mod command;
mod config;
mod conversation;
mod display;
mod focus;
mod form;
mod hud;
mod identity;
mod keys;
mod link;
mod logging;
mod pending;
mod pill;
mod speech;
mod state;
mod stt;
mod textbox;
mod tray;
mod widgets;

use std::{
    collections::BTreeMap,
    env,
    error::Error,
    os::unix::process::CommandExt,
    process::ExitCode,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Receiver,
    },
    thread,
};

use app::{
    ACK_TIMEOUT, App, Backend, COPY_EVENT, CUES_EVENT, Executor, Hidden, Keys, PRESENTATION_EVENT,
    Ports, PresentationPayload, Shown, Surface, TICK_EVENT, ThreadExecutor, TickPayload,
    Transcriber,
};
use attachment::AttachmentClient;
use audio::{CpalRecorder, Recorder};
use config::Config;
use focus::FocusTracker;
use hud::Hud;
use link::{LinkEvent, ServiceLink};
use pending::{FilePendingStore, PendingStore};
use scufris_control::{
    command::{Outcome, Verb},
    service::{
        AttachmentDescriptor, ConversationMessage, ConversationRole, SurfaceRegistration,
        WidgetCall,
    },
};
use speech::Speaker;
use state::Event;
use tauri::{AppHandle, Emitter, Manager, RunEvent, WindowEvent, http, ipc::Channel, menu::Menu};
use tauri_plugin_dialog::DialogExt;
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

/// The two windows, the tray, and focus restoration.
struct DesktopSurface {
    handle: AppHandle,
    menu: Menu<tauri::Wry>,
    focus: FocusTracker,
    speaker: Arc<Speaker>,
}

impl DesktopSurface {
    /// Takes the widget layer down with the pill, or brings it back up.
    ///
    /// The pill and everything the runtime put beside it are one layer: a panel
    /// left over a bare desktop after the pill went down is a widget with
    /// nothing to belong to. Asked for by handle rather than held, because the
    /// runtime is built after this surface is and the layer is nothing this
    /// surface owns.
    fn layer(&self, hidden: bool) {
        if let Some(widgets) = self.handle.try_state::<Arc<widgets::Widgets>>() {
            widgets.conceal(hidden);
        }
    }
}

impl Surface for DesktopSurface {
    fn show_pill(&self) -> Result<Shown, String> {
        let shown = pill::show(&self.handle)?;
        // After the pill, and only if it came up: the shelf stands above the
        // pill, and panels over a desktop with no pill under them are panels
        // belonging to nothing.
        self.layer(false);
        Ok(shown)
    }

    fn hide_pill(&self) -> Result<Hidden, String> {
        // The shelf goes with the pill: a panel left over a bare desktop is a
        // widget belonging to nothing. State intact, widgets mounted, clocks
        // stopped - this is the layer going away, not the panels going away.
        self.layer(true);
        pill::hide(&self.handle)
    }

    fn holding(&self) -> bool {
        self.handle
            .try_state::<Arc<widgets::Widgets>>()
            .is_some_and(|widgets| widgets.holding())
    }

    fn show_textbox(&self) -> Result<Shown, String> {
        // Only when the box does not already hold the keyboard. Capturing then
        // would record the box itself as the window to give the desktop back
        // to. A pill on screen is fine to capture over: it never holds the
        // keyboard, so the active window is still the person's own.
        if !textbox::focused(&self.handle) {
            // Never a window of the companion's own, whichever one the window
            // manager is calling active.
            self.focus.capture(&crate::focus::own_windows(&self.handle));
        }
        textbox::show(&self.handle)
    }

    fn hide_textbox(&self) -> Result<(), String> {
        // A box that is already down has no keyboard to give back, and giving
        // it back anyway would take the keys off whatever the person moved to.
        // This is what makes the operation safe to ask for unconditionally.
        if !textbox::up(&self.handle) {
            return Ok(());
        }
        textbox::hide(&self.handle)?;
        self.focus.restore()
    }

    fn textbox_has_keyboard(&self) -> bool {
        textbox::focused(&self.handle)
    }

    fn nobody_has_the_keyboard(&self) -> bool {
        // The display's own answer, or the one window of ours that is a dead
        // end for every key: the pill refuses the keyboard and has no key
        // handlers, so a window manager that forces focus onto it has taken the
        // keys as surely as a window manager that focused nothing.
        display::nobody_holds_the_keyboard() == display::Verdict::Yes
            || pill::holds_the_keyboard(&self.handle)
    }

    fn present(&self, payload: PresentationPayload) -> Result<(), String> {
        // A person who is talking is not reading the screen. The pill already
        // knows the microphone is open, and this is the only place that says so
        // on every presentation rather than on one transition.
        if let Some(widgets) = self.handle.try_state::<Arc<widgets::Widgets>>() {
            widgets.recording(payload.recording);
        }
        // And a person who has started talking is not waiting for the rest of
        // the sentence. Barge-in belongs here for the same reason: it is the
        // one place that sees the microphone on every presentation.
        if payload.recording {
            self.speaker.hush();
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

/// Local transcription through the configured ai-tools-api endpoint.
struct HttpTranscriber {
    endpoint: String,
    model: String,
    language: String,
}

impl Transcriber for HttpTranscriber {
    fn transcribe(&self, wav: Vec<u8>) -> Result<String, String> {
        stt::transcribe(&self.endpoint, &self.model, &self.language, &wav)
            .map_err(|error| error.to_string())
    }
}

struct LocalPresentation {
    stop_speech: bool,
    speak: Option<String>,
    widgets: Vec<WidgetCall>,
}

fn local_presentation(
    message: &ConversationMessage,
    live: bool,
    local_surface: &str,
) -> LocalPresentation {
    if !live {
        return LocalPresentation {
            stop_speech: false,
            speak: None,
            widgets: Vec::new(),
        };
    }
    let associated =
        message.role == ConversationRole::Assistant && message.surface == local_surface;
    LocalPresentation {
        stop_speech: true,
        speak: associated.then(|| message.text.clone()),
        widgets: if associated {
            message.widgets.clone().unwrap_or_default()
        } else {
            Vec::new()
        },
    }
}

impl Backend for ServiceLink {
    fn submit(&self, id: String, text: String) -> Result<(), String> {
        ServiceLink::submit(self, id, text)
    }

    fn submit_with_attachments(
        &self,
        id: String,
        text: String,
        attachments: Vec<String>,
    ) -> Result<(), String> {
        ServiceLink::submit_with_attachments(self, id, text, attachments)
    }

    fn abort(&self, id: String) -> Result<(), String> {
        ServiceLink::abort(self, id)
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
    let content_socket = config
        .socket
        .parent()
        .ok_or("the surface socket has no runtime directory")?
        .join(scufris_control::service::CONTENT_FILE_NAME);
    let attachment_client = Arc::new(AttachmentClient::new(content_socket)?);
    let surface_id = identity::load_or_create(&config.state_file)?;
    let surface_name = identity::diagnostic_name();
    info!(
        surface = surface_id,
        name = surface_name,
        "surface {} identity loaded",
        surface_name
    );
    debug!(
        surface = surface_id,
        name = surface_name,
        state_file = %config.state_file.display(),
        command_socket = ?config.command_socket,
        "surface configuration resolved"
    );

    let tauri_app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(
            tauri_plugin_global_shortcut::Builder::new()
                .with_handler(|app, shortcut, event| {
                    // This is the display's thread, and everything this handler
                    // decides is queued rather than done: see `carry`.
                    let Some(hold) = app.try_state::<Arc<keys::Hold>>() else {
                        return;
                    };
                    let state = event.state();
                    // The hotkey that opens the pill, or one of the two keys
                    // grabbed beside it while it is up. Every accelerator
                    // arrives here, so which one it is decides what it means.
                    let pill = app.try_state::<Arc<keys::PillKeys>>();
                    let beside = match pill {
                        Some(keys) if keys.cancels(shortcut) => Some(keys::Gesture::Cancel),
                        Some(keys) if keys.stops(shortcut) => {
                            // Stop means stop. The speaker is the companion's
                            // own and never crossed the socket, so it is cut
                            // right here rather than queued; the run belongs to
                            // the service, so the runtime is what asks for that.
                            app.state::<Arc<Speaker>>().hush();
                            Some(keys::Gesture::Stop)
                        }
                        _ => None,
                    };
                    // The two keys beside the hotkey have one meaning each, so
                    // they answer the press and ignore the release. Only the
                    // hotkey has two, and only it needs to know how long it was
                    // down to tell them apart.
                    if let Some(gesture) = beside {
                        if state == ShortcutState::Pressed {
                            hold.asks(gesture);
                        }
                        return;
                    }
                    hotkey(hold.inner(), state);
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
            pill_cues,
            pill_log,
            textbox_ready,
            textbox_submit,
            textbox_cancel,
            textbox_copy,
            widget_shell_ready,
            widget_tick,
            widget_hover,
            widget_send,
            widget_ask,
            form_ready,
            form_submit,
            form_cancel,
            form_look,
            hud_ready,
            hud_submit,
            hud_attach,
            hud_detach,
            hud_open_attachment,
            hud_save_attachment,
            hud_close,
            hud_toggle
        ])
        .setup(move |tauri| {
            let handle = tauri.handle().clone();
            pill::ensure(&handle)?;
            textbox::ensure(&handle)?;
            // Made now rather than on the first question, for the reason the
            // textbox is: the page has to be loaded and listening before the
            // question that fills it arrives.
            form::ensure(&handle)?;

            // One prefix for the whole process, shared by both senders. It is
            // what makes an identifier this companion's rather than another's;
            // keeping the two counters apart is the HUD's own job.
            let prefix = App::process_prefix();
            let conversation = Hud::start(handle.clone(), prefix.clone())?;
            tauri.manage(Arc::clone(&conversation));
            tauri.manage(Arc::clone(&attachment_client));

            // Before the menu, because the menu offers what is installed: the
            // catalog is what the summon submenu is built from.
            let widgets = widgets::Widgets::start(handle.clone())?;
            // The layer starts where the pill starts, which is away. Said here
            // rather than made the runtime's default: the runtime is a layer of
            // panels and has no opinion about a pill, and a companion that
            // started with its windows up would be one that greets a person who
            // did not ask for it.
            widgets.conceal(true);
            tauri.manage(Arc::clone(&widgets));

            let menu = tray::build_menu(
                &handle,
                chat_available,
                restart_available,
                &app::status_line("disconnected", "The Scufris service is unavailable."),
                &widgets.summonable(),
            )?;
            tauri.manage(CueSwitch(AtomicBool::new(true)));
            let cue_menu = menu.clone();
            tray::install(
                &handle,
                &menu,
                move |app, id| {
                    let runtime = app.state::<Arc<App>>().inner().clone();
                    match id {
                        tray::MENU_HUD => show_conversation(app),
                        tray::MENU_CHAT => runtime.open_chat(),
                        // Off this thread. An activation waits for the pill to
                        // be on screen before the microphone opens, and this is
                        // the thread that would have to put it there.
                        tray::MENU_VOICE => {
                            thread::spawn(move || runtime.handle(Event::Activate));
                        }
                        tray::MENU_RESTART => runtime.restart_backend(),
                        tray::MENU_SPEECH => {
                            let speaker = app.state::<Arc<Speaker>>();
                            let muted = speaker.mute(!speaker.muted());
                            if let Err(error) = tray::set_speech_label(&cue_menu, muted) {
                                warn!("{error}");
                            }
                            info!(muted, "speech");
                        }
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
                        other => {
                            if let Some(widget) = tray::summoned(other) {
                                app.state::<Arc<widgets::Widgets>>()
                                    .summon(widget.to_string());
                            }
                        }
                    }
                },
                // A left click on the tray shows the conversation. It is the
                // one thing here that always works: the window ships with the
                // companion, where the terminal command is something the person
                // has to have configured.
                show_conversation,
            )?;

            // Managed as well as held by the runtime: the accelerator handler
            // has only the app to ask which key it was given.
            let pill_keys = Arc::new(keys::PillKeys::new(
                handle.clone(),
                &config.hotkey,
                keys::Wanted {
                    cancel: config.cancel_key.as_deref(),
                    stop: config.stop_key.as_deref(),
                },
            ));
            tauri.manage(Arc::clone(&pill_keys));
            // The hotkey's own memory of whether it is being tapped or held,
            // and the queue every key it reads is carried out from. Managed for
            // the same reason the keys beside it are: the handler has nothing
            // but the app.
            let (hold, queued) = keys::Hold::new();
            carry(handle.clone(), queued);
            tauri.manage(Arc::new(hold));

            // Before the runtime, because the runtime holds the surface that
            // cuts speech on barge-in; the runtime is given back to it below.
            let speaker = Speaker::new(config.speak_command.clone());

            let runtime = Arc::new(App::new(Ports {
                surface: Arc::new(DesktopSurface {
                    handle: handle.clone(),
                    menu,
                    focus: FocusTracker::new(),
                    speaker: Arc::clone(&speaker),
                }) as Arc<dyn Surface>,
                keys: Arc::clone(&pill_keys) as Arc<dyn Keys>,
                recorder: Arc::new(CpalRecorder) as Arc<dyn Recorder>,
                pending: Arc::new(FilePendingStore::new(config.state_file.clone()))
                    as Arc<dyn PendingStore>,
                transcriber: Arc::new(HttpTranscriber {
                    endpoint: config.stt_endpoint.clone(),
                    model: config.stt_model.clone(),
                    language: config.stt_language.clone(),
                }) as Arc<dyn Transcriber>,
                executor: Arc::new(ThreadExecutor) as Arc<dyn Executor>,
                prefix,
                chat_command: config.chat_command.clone(),
                restart_command: config.restart_command.clone(),
                ack_timeout: ACK_TIMEOUT,
            }));
            tauri.manage(Arc::clone(&runtime));

            // Both siblings hear it. The pill shows speech over whatever the
            // service is reporting, and the widgets runtime holds an exhibit's
            // grace while the person is listening rather than reading.
            let speaking_runtime = Arc::clone(&runtime);
            let speaking_surfaces = Arc::clone(&widgets);
            speaker.attach(move |speaking| {
                speaking_runtime.set_speaking(speaking);
                speaking_surfaces.assistant(speaking_runtime.shown_assistant());
            });
            tauri.manage(Arc::clone(&speaker));

            let observer = Arc::clone(&runtime);
            let surfaces = Arc::clone(&widgets);
            let voice = Arc::clone(&speaker);
            let said = Arc::clone(&conversation);
            let local_surface = surface_id.clone();
            let registration = SurfaceRegistration {
                id: surface_id.clone(),
                name: surface_name.clone(),
                widgets: widgets.definitions(),
            };
            let link = Arc::new(ServiceLink::start(
                config.socket.clone(),
                registration,
                move |event| match event {
                    LinkEvent::ReplayStarted => said.reconnected(),
                    LinkEvent::Ready => {
                        said.ready();
                        observer.observe(LinkEvent::Ready);
                    }
                    LinkEvent::Message { message, live } => {
                        said.said(message.clone());
                        let presentation = local_presentation(&message, live, &local_surface);
                        if presentation.stop_speech {
                            voice.hush();
                        }
                        if let Some(text) = presentation.speak {
                            voice.say(text);
                        }
                        for call in presentation.widgets {
                            surfaces.call(call);
                        }
                    }
                    LinkEvent::Accepted(id) => {
                        said.accepted(&id);
                        observer.observe(LinkEvent::Accepted(id));
                    }
                    LinkEvent::Refused(id, detail) => {
                        said.refused(&id, detail.clone());
                        observer.observe(LinkEvent::Refused(id, detail));
                    }
                    LinkEvent::Disconnected => {
                        said.dropped(link::UNAVAILABLE);
                        observer.observe(LinkEvent::Disconnected);
                    }
                    LinkEvent::HandshakeFailed => {
                        observer.observe(LinkEvent::HandshakeFailed);
                    }
                    LinkEvent::State(state, detail) => {
                        said.assistant(state);
                        observer.observe(LinkEvent::State(state, detail));
                        surfaces.assistant(observer.shown_assistant());
                    }
                },
            ));
            conversation.attach(Arc::clone(&link) as Arc<dyn Backend>);
            runtime.set_backend(Arc::clone(&link) as Arc<dyn Backend>);
            tauri.manage(link);

            // A window manager that has already taken the activation key is the
            // good case rather than a fault: its own binding runs
            // `scufris-ctl open` and arrives in the same place. The display
            // reports a key somebody else grabbed as one that is already
            // registered, which is what it looks like from here. Refusing to
            // start over it would leave the person with no companion for a key
            // that was going to work.
            if let Err(error) = tauri.global_shortcut().register(shortcut) {
                warn!(
                    hotkey = %config.hotkey,
                    "the activation accelerator belongs to somebody else: {error}"
                );
            }

            // The window manager's way in. Started last, because a verb that
            // arrives is acted on immediately and everything it acts on has to
            // be here. A socket that cannot be made is reported and nothing
            // else: the hotkey and the tray still work, and refusing to start
            // over a socket the person may never use is the worse trade.
            if let Some(socket) = config.command_socket.clone() {
                let acting = handle.clone();
                match command::listen(socket, move |verb| perform(&acting, verb)) {
                    Ok(path) => {
                        tauri.manage(CommandSocket(path));
                    }
                    Err(error) => warn!("no command socket: {error}"),
                }
            }
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
        // An i3 kill on the conversation window means put it away, not destroy
        // it. The window is built at startup and filled whether it is on screen
        // or not, and a destroyed one would have to be built and refilled the
        // next time somebody asked for it. Hidden, it is already what the next
        // toggle wants.
        RunEvent::WindowEvent {
            label,
            event: WindowEvent::CloseRequested { api, .. },
            ..
        } if label == hud::LABEL => {
            api.prevent_close();
            if let Err(error) = handle.state::<Arc<Hud>>().hide() {
                warn!("{error}");
            }
        }
        RunEvent::ExitRequested { api, code, .. } if code.is_none() => api.prevent_exit(),
        RunEvent::Exit => {
            info!("stopping");
            handle.state::<Arc<ServiceLink>>().stop();
            // Before the windows go, so a paragraph in flight does not outlive
            // the companion that asked for it.
            handle.state::<Arc<Speaker>>().hush();
            // Before the app's own shutdown, because a widget backend is its
            // own process group and so does not die with the companion. One
            // left behind is a sampler running until the machine is rebooted.
            handle.state::<Arc<widgets::Widgets>>().halt();
            // A socket file with nothing behind it makes `scufris-ctl` report
            // a refused connection rather than a companion that is not running.
            if let Some(socket) = handle.try_state::<CommandSocket>() {
                command::unbind(&socket.0);
            }
            handle.state::<Arc<App>>().shutdown();
        }
        _ => {}
    });
    Ok(())
}

/// Where the command socket is, so the exit can take it away again.
struct CommandSocket(std::path::PathBuf);

/// Carries the hotkey going down and coming up.
///
/// One key, two gestures. Tapped it asks for the workspace, which is the door
/// the companion did not have: everything else that put the pill on screen
/// opened the microphone on the way. Held it is push to talk, and the release
/// is what ends the take - so the microphone is open exactly while the key is,
/// which is the one arrangement nobody has to read a pill to be sure of.
///
/// Both gestures come out as events the runtime already had. A hold is the
/// activation that starts a take and the activation that stops it, sent at the
/// two ends of the same press, which is why the tray and `scufris-ctl open`
/// keep working the way they did.
///
/// Neither is carried out here. This is the display's thread and it reads what
/// the key did; [`carry`] is what acts on it.
fn hotkey(hold: &Arc<keys::Hold>, state: ShortcutState) {
    match state {
        ShortcutState::Pressed => {
            let turn = hold.pressed();
            let waiting = Arc::clone(hold);
            // On a thread because this one is the display's: it hands over
            // every accelerator, and a quarter of a second spent asleep here is
            // a quarter of a second in which no other key arrives.
            thread::spawn(move || {
                thread::sleep(keys::HOLD);
                waiting.matured(turn);
            });
        }
        ShortcutState::Released => hold.released(),
    }
}

/// Starts the thread that carries out what the person's keys meant.
///
/// The keys are read on the display's own thread, which is the thread the
/// display also takes grabs on: a grab asked for while that thread is inside
/// the handler waits for the handler to return. The event loop is what asks for
/// those grabs, and the runtime is full of work that waits for the event loop -
/// so a key acted on where it is read is the hotkey thread waiting for the
/// event loop while the event loop waits for the hotkey thread. Both stop, and
/// with the event loop stopped so does every window and the tray with them.
///
/// One thread rather than one per key, because the order is the meaning: the
/// activation that opens the microphone and the one that closes it are the two
/// ends of a single press.
fn carry(handle: AppHandle, queued: Receiver<keys::Gesture>) {
    thread::spawn(move || {
        for gesture in queued {
            let Some(runtime) = handle.try_state::<Arc<App>>() else {
                return;
            };
            let runtime = runtime.inner().clone();
            match gesture {
                // The two ends of one press. Both are the activation the tray
                // and `scufris-ctl open` have always sent.
                keys::Gesture::Open | keys::Gesture::Talk => runtime.handle(Event::Activate),
                keys::Gesture::Tap => runtime.workspace(),
                keys::Gesture::Cancel => runtime.handle(Event::Escape),
                keys::Gesture::Stop => runtime.handle(Event::Stop),
            }
        }
    });
}

/// Puts the conversation window up, or away if it is already up.
///
/// The tray calls this from the event loop, so it does its own reporting: there
/// is no caller in a terminal to hand a refusal to.
fn show_conversation(app: &AppHandle) {
    if let Err(error) = app.state::<Arc<Hud>>().toggle() {
        warn!("{error}");
    }
}

/// Carries out one verb the desktop sent.
///
/// Both of them are a window. Neither carries words: what carries words is
/// typed into the textbox or the HUD, which are focused windows that read their
/// own keys.
fn perform(handle: &AppHandle, verb: Verb) -> Outcome {
    match verb {
        Verb::Open => {
            handle
                .state::<Arc<App>>()
                .inner()
                .clone()
                .handle(Event::Activate);
            Outcome::Taken
        }
        // Reported rather than assumed, unlike an activation: this verb is a
        // window going up or down and the caller can see whether it did, so a
        // window that refused is worth saying out loud in their terminal.
        Verb::Hud => match handle.state::<Arc<Hud>>().toggle() {
            Ok(()) => Outcome::Taken,
            Err(detail) => Outcome::Refused { detail },
        },
        // Taken rather than reported, as an activation is. The workspace is a
        // layer and not a window: asking for one that is already up is not a
        // refusal, it is a request that was already true.
        Verb::Show => {
            handle
                .state::<Arc<App>>()
                .inner()
                .clone()
                .handle(Event::Reveal);
            Outcome::Taken
        }
        Verb::Hide => {
            handle
                .state::<Arc<App>>()
                .inner()
                .clone()
                .handle(Event::Dismiss);
            Outcome::Taken
        }
    }
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

/// The textbox page saying hello, and asking what it has missed.
///
/// A transcript recovered from a previous process is published as the companion
/// starts, which can be before this window's page is listening. Rather than let
/// the box come up empty over words nobody can read, the page asks for the
/// presentation again once it is ready for one.
#[tauri::command]
fn textbox_ready(runtime: tauri::State<'_, Arc<App>>) {
    runtime.inner().clone().publish();
}

/// Enter in the textbox, carrying whatever is in the field.
///
/// `text` is absent for a transcript the person may not edit, so the machine
/// sends the words it is holding rather than the ones a page could have
/// changed underneath it.
#[tauri::command]
fn textbox_submit(runtime: tauri::State<'_, Arc<App>>, text: Option<String>) {
    runtime.inner().clone().handle(Event::Enter { text });
}

/// Escape in the textbox.
#[tauri::command]
fn textbox_cancel(runtime: tauri::State<'_, Arc<App>>) {
    runtime.inner().clone().handle(Event::Escape);
}

/// Ctrl+C in the textbox, with nothing selected.
#[tauri::command]
fn textbox_copy(runtime: tauri::State<'_, Arc<App>>) {
    runtime.inner().clone().handle(Event::Copy);
}

/// The HUD page saying hello, and asking for everything it has missed.
///
/// The window is built at startup and the conversation fills it whether it is
/// on screen or not, so by the time a person opens it there is usually a
/// backlog. The page renders what comes back and appends from there.
#[tauri::command]
fn hud_ready(conversation: tauri::State<'_, Arc<Hud>>) -> hud::Backlog {
    conversation.backlog()
}

/// Enter in the HUD, carrying what is in the field.
///
/// Answers whether the line was taken. The page clears the field on that and on
/// nothing else: a line refused because one is already in flight has to stay
/// where the person can send it again.
#[tauri::command]
fn hud_submit(conversation: tauri::State<'_, Arc<Hud>>, text: String) -> bool {
    conversation.typed(text)
}

/// Selects and imports one regular bounded file into service-owned storage.
#[tauri::command]
async fn hud_attach(
    app: AppHandle,
    conversation: tauri::State<'_, Arc<Hud>>,
    attachments: tauri::State<'_, Arc<AttachmentClient>>,
) -> Result<conversation::Notice, String> {
    let Some(file) = app
        .dialog()
        .file()
        .set_title("Attach a file")
        .blocking_pick_file()
    else {
        return Ok(conversation.backlog().notice);
    };
    let path = file
        .into_path()
        .map_err(|_| "Choose a local file.".to_string())?;
    match attachments.import(&path) {
        Ok(descriptor) => conversation.attach_file(descriptor),
        Err(error) => {
            conversation.attachment_failed(error.clone());
            Err(error)
        }
    }
}

/// Removes one managed file from the next message.
#[tauri::command]
fn hud_detach(conversation: tauri::State<'_, Arc<Hud>>, id: String) -> conversation::Notice {
    conversation.detach(&id)
}

/// Opens one safe canonical attachment with its desktop handler.
#[tauri::command]
async fn hud_open_attachment(
    conversation: tauri::State<'_, Arc<Hud>>,
    attachments: tauri::State<'_, Arc<AttachmentClient>>,
    descriptor: AttachmentDescriptor,
) -> Result<(), String> {
    match attachments.open(&descriptor) {
        Ok(()) => Ok(()),
        Err(error) => {
            conversation.attachment_failed(error.clone());
            Err(error)
        }
    }
}

/// Saves one canonical attachment to a destination chosen by the person.
#[tauri::command]
async fn hud_save_attachment(
    app: AppHandle,
    conversation: tauri::State<'_, Arc<Hud>>,
    attachments: tauri::State<'_, Arc<AttachmentClient>>,
    descriptor: AttachmentDescriptor,
) -> Result<(), String> {
    let Some(file) = app
        .dialog()
        .file()
        .set_title("Save attachment")
        .set_file_name(&descriptor.name)
        .blocking_save_file()
    else {
        return Ok(());
    };
    let destination = file
        .into_path()
        .map_err(|_| "Choose a local destination.".to_string())?;
    match attachments.save(&descriptor, &destination) {
        Ok(()) => Ok(()),
        Err(error) => {
            conversation.attachment_failed(error.clone());
            Err(error)
        }
    }
}

/// Escape in the HUD.
#[tauri::command]
fn hud_close(conversation: tauri::State<'_, Arc<Hud>>) {
    if let Err(error) = conversation.hide() {
        warn!("{error}");
    }
}

/// A click on the pill.
///
/// The orb is the one window that refuses the keyboard, so it has no key to
/// answer with; a pointer reaches it anyway, because pointer input has nothing
/// to do with focus. This is the whole of what the pill does when it is
/// clicked, and it is the same toggle the tray and `scufris-ctl hud` ask for.
#[tauri::command]
fn hud_toggle(conversation: tauri::State<'_, Arc<Hud>>) {
    if let Err(error) = conversation.toggle() {
        warn!("{error}");
    }
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

/// One chrome tick, from the window it was clicked in.
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
        "restart" => widgets.restarted(surface),
        unknown => debug!(kind = %unknown, "a widget window reported an unknown tick"),
    }
}

/// One action a widget sent toward whatever feeds it.
///
/// The window says which surface this is, for the reason the chrome ticks do:
/// a page that named its own surface could name another window's, and this one
/// writes to a process.
#[tauri::command]
fn widget_send(
    widgets: tauri::State<'_, Arc<widgets::Widgets>>,
    window: tauri::Window,
    action: serde_json::Value,
) {
    widgets.sent(window.label().to_string(), action);
}

/// One question a widget asked before it sends anything.
///
/// The window says which surface this is, for the reason `widget_send` does. A
/// page that named its own surface could put a question on the box in another
/// panel's name, and the answer would be written into that panel's journal.
#[tauri::command]
fn widget_ask(
    widgets: tauri::State<'_, Arc<widgets::Widgets>>,
    window: tauri::Window,
    request: serde_json::Value,
) {
    widgets.asked(window.label().to_string(), request);
}

/// The form page saying hello, and asking what it has missed.
///
/// The question is pushed just before the box comes up, so a page that is still
/// loading misses it. This is how that page catches up.
#[tauri::command]
fn form_ready(widgets: tauri::State<'_, Arc<widgets::Widgets>>) -> Option<form::Ask> {
    widgets.asking()
}

/// Enter in the form box, carrying whatever is in the fields.
///
/// Only the fields that were asked for are carried on. What the answers mean
/// was decided by the widget that asked, and this process is holding that.
#[tauri::command]
fn form_submit(
    widgets: tauri::State<'_, Arc<widgets::Widgets>>,
    answers: BTreeMap<String, String>,
) {
    widgets.answered(&answers);
}

/// Escape in the form box.
#[tauri::command]
fn form_cancel(widgets: tauri::State<'_, Arc<widgets::Widgets>>) {
    widgets.dropped();
}

/// Typing in a form field that offers candidates.
///
/// The page names a field and hands over what is in it. It cannot name the
/// action or the panel: both come from the question this process is holding,
/// which is the same rule the answer follows.
#[tauri::command]
fn form_look(widgets: tauri::State<'_, Arc<widgets::Widgets>>, field: String, text: String) {
    widgets.looking(&field, &text);
}

/// The pointer arriving over one widget window, or leaving it.
///
/// A panel somebody is reading does not age out from under them. Only the page
/// can see this: the window is built unfocusable and never takes a pointer
/// grab, so the host has no other way to know the pointer is there.
#[tauri::command]
fn widget_hover(
    widgets: tauri::State<'_, Arc<widgets::Widgets>>,
    window: tauri::Window,
    over: bool,
) {
    widgets.hover(window.label().to_string(), over);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn answer(surface: &str) -> ConversationMessage {
        ConversationMessage {
            role: ConversationRole::Assistant,
            surface: surface.into(),
            text: "Spoken text.".into(),
            details: Some("## Details\n\nNever spoken.".into()),
            widgets: Some(vec![WidgetCall {
                id: "call-1".into(),
                name: "cpu".into(),
                arguments: serde_json::json!({}),
            }]),
            attachments: vec![],
        }
    }

    #[test]
    fn replay_has_no_local_presentation_effects() {
        let presentation = local_presentation(&answer("desk"), false, "desk");
        assert!(!presentation.stop_speech);
        assert_eq!(presentation.speak, None);
        assert!(presentation.widgets.is_empty());
    }

    #[test]
    fn only_the_associated_live_surface_speaks_and_executes_widgets() {
        let associated = local_presentation(&answer("desk"), true, "desk");
        assert!(associated.stop_speech);
        assert_eq!(associated.speak.as_deref(), Some("Spoken text."));
        assert_eq!(associated.widgets.len(), 1);

        let other = local_presentation(&answer("phone"), true, "desk");
        assert!(other.stop_speech);
        assert_eq!(other.speak, None);
        assert!(other.widgets.is_empty());
    }
}
