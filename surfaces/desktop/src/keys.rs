//! The keys that are arranged where no window is.
//!
//! The pill never takes the keyboard, and the textbox is not up while the
//! microphone is open, so between `Super+D` and the words arriving there is no
//! window to type at. Two keys cover that gap: the cancel key, which stops a
//! listen the person did not mean to start, and the stop key, which stops
//! Scufris.
//!
//! They are accelerators the display grabs, built from the hotkey's own
//! modifiers: `Super+D` opens the pill, so `Super+Escape` puts it away and
//! `Super+Delete` stops what Scufris is doing. Grabbed only while the pill is
//! on screen, because an accelerator held all session is one no other program
//! can ever use.
//!
//! The deployment can name either of them instead. Deriving them is the
//! default rather than the rule: one modifier to remember is the right thing
//! to ship, and a desktop where `Super+Escape` already means something is a
//! reason to move the key rather than to lose it. [`NONE`] takes a key off the
//! companion entirely, which is the answer for a desktop that wants both of
//! these keys for itself.
//!
//! Stop is its own key rather than a second meaning for Escape. Escape puts a
//! pill away and throws away a take, and neither reaches the conversation; stop
//! ends a run that may be part way through changing something. A gesture with
//! that much behind it is not one to arrive at by pressing the dismiss key at
//! the wrong moment.
//!
//! Neither is required. A companion whose hotkey has no modifier is one the
//! person puts away with the tray and stops with the local stop control, and every
//! other key belongs to the textbox.
//!
//! Nothing any of these keys means is carried out where it is decided. The
//! display hands every accelerator to one handler, on the same thread it takes
//! grabs on, so that thread waiting for the event loop and the event loop
//! waiting for a grab is a deadlock with the person's own hotkey at the top of
//! it. What a key meant is queued here and carried out by a thread of the
//! companion's own.

use std::{
    sync::{
        Mutex,
        mpsc::{self, Receiver, Sender},
    },
    thread,
    time::Duration,
};

use tauri::AppHandle;
use tauri_plugin_global_shortcut::{GlobalShortcutExt, Shortcut};
use tracing::{debug, warn};

use crate::app::Keys;

/// The key that stops a listen.
const CANCEL: &str = "Escape";

/// The key that stops Scufris.
///
/// `Period` was the first choice and was wrong: `Super+.` is the emoji picker
/// on Windows, and the rofi and Hyprland desktops that copied it mean the same
/// thing by it. A default that collides with a convention that widespread is a
/// default that is configured away on every machine.
///
/// `Delete` belongs to nothing modified, which is what makes it grabbable, and
/// what it means is close enough: end the thing that is running. It is spelled
/// out in full because the accelerator parser knows `Delete` and does not know
/// `Del`.
const STOP: &str = "Delete";

/// How long the hotkey stays down before it is talking rather than tapping.
///
/// A quarter of a second. Long enough that no tap reaches it - a deliberate tap
/// is well under two hundred milliseconds - and short enough that a person who
/// pressed to speak has not started speaking yet: the hand leaves the key before
/// the voice arrives, which is the whole reason push to talk works.
///
/// It is the microphone's latency, and it is the price of the key meaning two
/// things. The alternative is opening the microphone on every tap and throwing
/// the scrap away, which costs nothing to hear and a great deal to trust.
pub const HOLD: Duration = Duration::from_millis(250);

/// What one of the person's keys turned out to mean.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Gesture {
    /// The hotkey has been down [`HOLD`] long. The microphone opens.
    Open,
    /// The hotkey went down and came up inside [`HOLD`]. The person asked for
    /// the workspace.
    Tap,
    /// The hotkey came up on a microphone it opened. The take ends here.
    Talk,
    /// The cancel key.
    Cancel,
    /// The stop key.
    Stop,
}

/// Where the person's keys are read, and the queue that carries them out.
///
/// Two jobs that are one job. One key has two gestures on it, so a press has to
/// be remembered until the release says which it was; and none of that may be
/// acted on where it is decided, because the deciding happens on the display's
/// own thread.
///
/// So a decided gesture is posted rather than performed, and it is posted under
/// the lock that decided it. The microphone opening and the microphone closing
/// are two gestures of one press, and the order between them is the whole
/// meaning: a stop that overtook its own start would leave the microphone open
/// with nothing on screen saying so.
#[derive(Debug)]
pub struct Hold {
    grip: Mutex<Grip>,
    /// Where a decided gesture goes to be carried out.
    posts: Sender<Gesture>,
}

#[derive(Debug, Default)]
struct Grip {
    /// Which press this is.
    ///
    /// A press is timed by a thread that wakes up later and asks whether the
    /// key is still down. Between the two the person can release and press
    /// again, and the waking thread would find a key that is down and open the
    /// microphone for a press that had nothing to do with it. Counting the
    /// presses is what tells those two apart.
    turn: u64,
    /// Whether a press is down at all.
    down: bool,
    /// Whether this press has already opened the microphone.
    talking: bool,
}

impl Hold {
    /// Builds the reader, and the queue whatever carries gestures out reads.
    pub fn new() -> (Self, Receiver<Gesture>) {
        let (posts, queued) = mpsc::channel();
        (
            Self {
                grip: Mutex::default(),
                posts,
            },
            queued,
        )
    }

    /// Records the hotkey going down, and answers which press this is.
    ///
    /// The number goes to whatever times this press and comes back with it.
    pub fn pressed(&self) -> u64 {
        let mut grip = self.grip();
        grip.turn = grip.turn.wrapping_add(1);
        grip.down = true;
        grip.talking = false;
        grip.turn
    }

    /// Opens the microphone, if the press this times is still down and silent.
    ///
    /// Once and only once per press: this asks for the microphone and the
    /// release is what closes it.
    pub fn matured(&self, turn: u64) {
        let mut grip = self.grip();
        if grip.turn != turn || !grip.down || grip.talking {
            return;
        }
        grip.talking = true;
        self.post(&grip, Gesture::Open);
    }

    /// Records the hotkey coming up, and asks for what it turned out to mean.
    ///
    /// A release with no press of ours behind it means nothing and asks for
    /// nothing, which is what a grab taken while the key was already held looks
    /// like.
    pub fn released(&self) {
        let mut grip = self.grip();
        if !grip.down {
            return;
        }
        grip.down = false;
        let gesture = if grip.talking {
            grip.talking = false;
            Gesture::Talk
        } else {
            Gesture::Tap
        };
        self.post(&grip, gesture);
    }

    /// Asks for one gesture that took no press to work out.
    ///
    /// The keys beside the hotkey have one meaning each and answer the press.
    /// They queue with the rest so that the person's keys are carried out in
    /// the order they were pressed.
    pub fn asks(&self, gesture: Gesture) {
        let grip = self.grip();
        self.post(&grip, gesture);
    }

    /// Queues one gesture. The grip is asked for to prove it is held: what
    /// orders these is the lock they were decided under.
    fn post(&self, _grip: &Grip, gesture: Gesture) {
        // The thread outlives every caller. A send that fails is a process on
        // its way out, and a key that reaches nothing is what that looks like.
        if let Err(error) = self.posts.send(gesture) {
            debug!("the key reached nothing: {error}");
        }
    }

    fn grip(&self) -> std::sync::MutexGuard<'_, Grip> {
        self.grip.lock().unwrap_or_else(|error| error.into_inner())
    }
}

/// What a deployment writes to take one key off the companion.
///
/// Not an accelerator and never parsed as one. A person who wants `Super+Q` to
/// stay theirs says so here rather than by finding a key nothing uses.
pub const NONE: &str = "none";

/// What the deployment said about the keys beside the hotkey.
///
/// Each is what was configured, and nothing is what was not: an unset key is
/// derived from the hotkey, which is what ships.
#[derive(Debug, Clone, Copy, Default)]
pub struct Wanted<'a> {
    /// The accelerator that puts the pill away.
    pub cancel: Option<&'a str>,
    /// The accelerator that stops Scufris.
    pub stop: Option<&'a str>,
}

/// The keys that answer the pill, and how they are arranged while it is up.
pub struct PillKeys {
    /// The accelerator that puts the pill away, when the hotkey leaves one.
    cancel: Option<Shortcut>,
    /// The accelerator that stops Scufris, on the same terms.
    stop: Option<Shortcut>,
    /// Where a wanted grab is asked for, when there is anything to grab.
    grabs: Option<Sender<bool>>,
}

impl PillKeys {
    /// Builds the arrangement this configuration allows.
    pub fn new(handle: AppHandle, hotkey: &str, wanted: Wanted<'_>) -> Self {
        let (cancel, stop) = arrange(hotkey, wanted);
        let wanted: Vec<Shortcut> = [cancel, stop].into_iter().flatten().collect();
        let grabs = (!wanted.is_empty()).then(|| grabber(handle, wanted));
        Self {
            cancel,
            stop,
            grabs,
        }
    }

    /// Says whether an accelerator is the cancel key.
    ///
    /// The display hands every accelerator to one handler, so this is what
    /// tells the cancel key from the hotkey that opens the pill.
    pub fn cancels(&self, shortcut: &Shortcut) -> bool {
        self.cancel.as_ref() == Some(shortcut)
    }

    /// Says whether an accelerator is the stop key.
    pub fn stops(&self, shortcut: &Shortcut) -> bool {
        self.stop.as_ref() == Some(shortcut)
    }

    /// Asks for the accelerator, or gives it back.
    fn grab(&self, wanted: bool) {
        let Some(grabs) = &self.grabs else {
            return;
        };
        // The thread outlives every caller. A send that fails is a process on
        // its way out, and the display drops every grab with the connection
        // holding it, so there is nothing left to give back.
        if let Err(error) = grabs.send(wanted) {
            debug!("the accelerator was left as it was: {error}");
        }
    }
}

/// Starts the thread that takes the accelerators and gives them back.
///
/// On a thread of its own, and never on the one that asked. The display hands
/// every accelerator to one handler on the same thread it takes grabs on, so a
/// grab asked for from inside that handler waits on a thread that is waiting
/// for the handler to return. The hotkey that opens the pill arrives on exactly
/// that thread, which makes it the ordinary road in rather than a corner.
///
/// One thread rather than one per change, because the last posture asked for is
/// the one the keys must be left arranged for, and two threads racing would
/// leave them arranged for either.
fn grabber(handle: AppHandle, keys: Vec<Shortcut>) -> Sender<bool> {
    let (asked, wanted) = mpsc::channel::<bool>();
    thread::spawn(move || {
        // Which are actually grabbed. A window manager that already holds one
        // of these refuses it, so what was asked for and what is held differ,
        // and they differ one key at a time.
        let mut held: Vec<Shortcut> = Vec::new();
        for want in wanted {
            if want {
                for key in &keys {
                    if held.contains(key) {
                        continue;
                    }
                    match handle.global_shortcut().register(*key) {
                        Ok(()) => held.push(*key),
                        // A window manager that already holds this key refuses
                        // the grab, and that is the good case: its own binding
                        // runs `scufris-ctl` and arrives in the same place.
                        Err(error) => debug!("{key} is somebody else's: {error}"),
                    }
                }
                continue;
            }
            held.retain(|key| match handle.global_shortcut().unregister(*key) {
                Ok(()) => false,
                Err(error) => {
                    warn!("{key} stayed grabbed: {error}");
                    true
                }
            });
        }
    });
    asked
}

impl Keys for PillKeys {
    fn stand(&self, on_screen: bool) {
        // Safe for as long as the pill is on screen, which is what gives the
        // key back the moment the pill is put away.
        self.grab(on_screen);
    }
}

/// The two keys this configuration leaves, with every collision refused.
///
/// A key is worth less than what it collides with. The handler matches cancel,
/// then stop, then falls through to activation, so a key that repeats an
/// earlier one does not gain a meaning: it takes the later meaning away, and
/// nothing on the desktop says so. Refusing the second one leaves a key that
/// does nothing and a line in the log, which is what the rest of this module
/// does with a key it cannot honour.
fn arrange(hotkey: &str, wanted: Wanted<'_>) -> (Option<Shortcut>, Option<Shortcut>) {
    let cancel = chosen(wanted.cancel, hotkey, CANCEL);
    let mut stop = chosen(wanted.stop, hotkey, STOP);
    if stop.is_some() && stop == cancel {
        let taken = stop.take().expect("it is the key just compared");
        warn!("{taken} already puts the pill away and does not stop Scufris");
    }
    (cancel, stop)
}

/// The accelerator one key ends up on: what was asked for, or what it derives
/// to, or nothing.
///
/// An accelerator that will not parse is warned about and dropped rather than
/// quietly derived. Falling back would leave the person with a working key on
/// the wrong accelerator, which is harder to notice than a key that does
/// nothing and says why in the log.
///
/// The hotkey is checked against for the same reason, and it reaches both
/// roads in: a deployment can name it outright, and a hotkey of `Super+Escape`
/// derives to itself. Either way the pill stops opening, which is the one key
/// nothing else on the desktop can stand in for.
fn chosen(wanted: Option<&str>, hotkey: &str, key: &str) -> Option<Shortcut> {
    let shortcut = match wanted {
        Some(NONE) => None,
        Some(accelerator) => parse(accelerator),
        None => beside(hotkey, key),
    }?;
    // Parsed here rather than through `parse`, which logs: a hotkey that will
    // not parse is not this function's news to report, and it would report it
    // twice.
    if hotkey.parse::<Shortcut>().ok() == Some(shortcut) {
        warn!("{shortcut} is what opens the pill and is left doing that");
        return None;
    }
    Some(shortcut)
}

/// One accelerator beside the activation hotkey, on the hotkey's own modifiers.
///
/// Its modifiers and nothing else: `Super+D` opens the pill, so `Super+Escape`
/// puts it away and `Super+Delete` stops it, and the person has one modifier to
/// remember rather than three. A hotkey with no modifier leaves none, because a
/// bare key the display gave the companion is one no other program on the
/// desktop would ever see again.
fn beside(hotkey: &str, key: &str) -> Option<Shortcut> {
    let (modifiers, _) = hotkey.rsplit_once('+')?;
    parse(&format!("{modifiers}+{key}"))
}

/// One accelerator, or nothing and a line in the log saying why.
fn parse(accelerator: &str) -> Option<Shortcut> {
    match accelerator.parse::<Shortcut>() {
        Ok(shortcut) => Some(shortcut),
        Err(error) => {
            warn!("{accelerator} is not an accelerator: {error}");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn accelerator(text: &str) -> Shortcut {
        text.parse().expect("it is an accelerator")
    }

    /// One modifier to remember. Whatever opens the pill is what puts it away
    /// and what stops it, down to a hotkey the person built out of two
    /// modifiers.
    #[test]
    fn the_hotkeys_own_modifiers_are_the_ones_that_answer_it() {
        assert_eq!(beside("Super+D", CANCEL), Some(accelerator("Super+Escape")));
        assert_eq!(beside("Super+D", STOP), Some(accelerator("Super+Delete")));
        assert_eq!(
            beside("Control+Alt+G", CANCEL),
            Some(accelerator("Control+Alt+Escape"))
        );
        assert_eq!(
            beside("Control+Alt+G", STOP),
            Some(accelerator("Control+Alt+Delete"))
        );
    }

    /// The two keys are two keys. Stop reaches the conversation and cancel
    /// never does, so nothing may deliver one by pressing the other.
    #[test]
    fn stopping_scufris_is_not_the_key_that_puts_the_pill_away() {
        assert_ne!(beside("Super+D", CANCEL), beside("Super+D", STOP));
    }

    /// A bare accelerator is global. Granting the companion one would take
    /// Escape off the desktop for every other program, for the whole session,
    /// and nothing here is worth that: the textbox holds its own Escape, a
    /// listen can always be put away with the tray, and a run can always be
    /// stopped with the local stop control.
    #[test]
    fn a_hotkey_with_no_modifier_leaves_the_desktops_bare_keys_alone() {
        assert_eq!(beside("F9", CANCEL), None);
        assert_eq!(beside("F9", STOP), None);
    }

    /// Deriving is the default and not the rule. What ships is one modifier to
    /// remember; what a person configures is what they get.
    #[test]
    fn a_key_the_deployment_named_is_the_key_that_is_grabbed() {
        assert_eq!(
            chosen(None, "Super+D", CANCEL),
            Some(accelerator("Super+Escape")),
            "an unset key derives"
        );
        assert_eq!(
            chosen(Some("Control+Shift+Q"), "Super+D", CANCEL),
            Some(accelerator("Control+Shift+Q"))
        );
        // A configured key owes the hotkey nothing, down to sharing no
        // modifier with it.
        assert_eq!(
            chosen(Some("Alt+F4"), "F9", STOP),
            Some(accelerator("Alt+F4")),
            "a hotkey with no modifier still allows a named key"
        );
    }

    /// A desktop that wants the key for itself says so, rather than hunting for
    /// an accelerator the companion will fail to parse.
    #[test]
    fn a_key_turned_off_is_grabbed_by_nothing() {
        assert_eq!(chosen(Some(NONE), "Super+D", CANCEL), None);
        assert_eq!(chosen(Some(NONE), "Super+D", STOP), None);
    }

    /// Never derived from. A working key on an accelerator the person did not
    /// ask for is harder to notice than a key that does nothing and says why.
    #[test]
    fn an_accelerator_that_will_not_parse_leaves_no_key_rather_than_the_default() {
        assert_eq!(chosen(Some("Hyper+Nonsense"), "Super+D", CANCEL), None);
        assert_eq!(chosen(Some(""), "Super+D", STOP), None);
    }

    /// The handler matches cancel and stop before it falls through to
    /// activation, so a key that is also the hotkey does not add a meaning to
    /// it. It removes the only way the pill opens, and nothing on the desktop
    /// would say why.
    #[test]
    fn a_key_that_is_the_hotkey_is_refused_rather_than_taking_activation() {
        assert_eq!(chosen(Some("Super+D"), "Super+D", CANCEL), None);
        assert_eq!(chosen(Some("Super+D"), "Super+D", STOP), None);
        // Spelled differently and the same key. The comparison is between two
        // parsed accelerators for exactly this.
        assert_eq!(chosen(Some("SUPER+d"), "Super+D", CANCEL), None);
        // And it reaches the derived road too: this hotkey derives to itself.
        assert_eq!(chosen(None, "Super+Escape", CANCEL), None);
        assert_eq!(
            chosen(None, "Super+Escape", STOP),
            Some(accelerator("Super+Delete")),
            "the other key is untouched by the one that collided"
        );
    }

    /// Cancel is matched first, so a shared accelerator means cancel and stop
    /// is what was lost. Refused where it can be said, rather than in a handler
    /// that has no way to.
    #[test]
    fn the_stop_key_is_refused_when_it_is_already_the_cancel_key() {
        let shared = Wanted {
            cancel: Some("Control+Q"),
            stop: Some("Control+Q"),
        };
        assert_eq!(
            arrange("Super+D", shared),
            (Some(accelerator("Control+Q")), None)
        );
        // Two keys that differ are both kept, and so is the derived pair.
        let apart = Wanted {
            cancel: Some("Control+Q"),
            stop: Some("Control+W"),
        };
        assert_eq!(
            arrange("Super+D", apart),
            (
                Some(accelerator("Control+Q")),
                Some(accelerator("Control+W"))
            )
        );
        assert_eq!(
            arrange("Super+D", Wanted::default()),
            (
                Some(accelerator("Super+Escape")),
                Some(accelerator("Super+Delete"))
            )
        );
    }

    /// Both off is not a collision. `NONE` twice is a desktop that wants both
    /// keys for itself, and the log has nothing to report about it.
    #[test]
    fn two_keys_turned_off_are_not_read_as_one_key_repeated() {
        let off = Wanted {
            cancel: Some(NONE),
            stop: Some(NONE),
        };
        assert_eq!(arrange("Super+D", off), (None, None));
    }

    /// Everything the keys asked for, in the order they asked for it.
    fn asked(queued: &Receiver<Gesture>) -> Vec<Gesture> {
        queued.try_iter().collect()
    }

    /// The press that came up before its timer woke is the workspace gesture,
    /// and nothing asks the microphone for anything.
    #[test]
    fn a_press_released_before_the_threshold_is_a_tap() {
        let (hold, queued) = Hold::new();
        hold.pressed();
        hold.released();
        assert_eq!(asked(&queued), vec![Gesture::Tap]);
    }

    /// The timer woke on a key still down, so the microphone opened, and the
    /// release is what closes it.
    #[test]
    fn a_press_that_outlives_the_threshold_is_a_take() {
        let (hold, queued) = Hold::new();
        let turn = hold.pressed();
        hold.matured(turn);
        hold.released();
        assert_eq!(asked(&queued), vec![Gesture::Open, Gesture::Talk]);
    }

    /// Once per press. A second timer on the same press would open a microphone
    /// that is already open, and the release would close only one of them.
    #[test]
    fn one_press_opens_the_microphone_once() {
        let (hold, queued) = Hold::new();
        let turn = hold.pressed();
        hold.matured(turn);
        hold.matured(turn);
        assert_eq!(asked(&queued), vec![Gesture::Open]);
    }

    /// The hazard the count is there for: a tap, then a second press inside the
    /// first press's threshold. The first timer wakes on a key that is down and
    /// must not read it as its own.
    #[test]
    fn a_timer_that_woke_late_does_not_open_the_microphone_for_the_next_press() {
        let (hold, queued) = Hold::new();
        let stale = hold.pressed();
        hold.released();
        let fresh = hold.pressed();
        hold.matured(stale);
        hold.released();
        hold.matured(fresh);
        assert_eq!(
            asked(&queued),
            vec![Gesture::Tap, Gesture::Tap],
            "two taps, and the microphone was never asked for"
        );
    }

    /// A release with no press behind it. The display can hand one over for a
    /// key that was already down when the grab was taken, and there is nothing
    /// to end.
    #[test]
    fn a_release_of_a_key_we_never_saw_go_down_asks_for_nothing() {
        let (hold, queued) = Hold::new();
        hold.released();
        hold.pressed();
        hold.released();
        hold.released();
        assert_eq!(asked(&queued), vec![Gesture::Tap]);
    }

    /// The order is the meaning. Whatever carries these out reads one queue, so
    /// the take cannot be stopped before it has started however fast the person
    /// lets go.
    #[test]
    fn the_keys_are_carried_out_in_the_order_they_were_pressed() {
        let (hold, queued) = Hold::new();
        let turn = hold.pressed();
        hold.matured(turn);
        hold.released();
        hold.asks(Gesture::Cancel);
        let second = hold.pressed();
        hold.matured(second);
        hold.asks(Gesture::Stop);
        hold.released();
        assert_eq!(
            asked(&queued),
            vec![
                Gesture::Open,
                Gesture::Talk,
                Gesture::Cancel,
                Gesture::Open,
                Gesture::Stop,
                Gesture::Talk,
            ]
        );
    }
}
