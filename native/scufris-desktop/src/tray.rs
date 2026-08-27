//! Tray icon, tray state colours, and the tray status menu.
//!
//! The icon is drawn from the state name so every state has a distinct colour
//! without shipping one image per state. Recording always draws the red privacy
//! ring, whatever the service reports.

use tauri::{
    AppHandle,
    image::Image,
    menu::{Menu, MenuBuilder, MenuItemKind},
    tray::{TrayIcon, TrayIconBuilder, TrayIconEvent},
};

/// Tray icon edge length in pixels.
pub const ICON_SIZE: u32 = 32;

/// Stable tray identifier.
pub const TRAY_ID: &str = "scufris-desktop";

/// Menu item identifiers the tray reports.
pub const MENU_HUD: &str = "hud";
pub const MENU_CHAT: &str = "chat";
pub const MENU_VOICE: &str = "voice";
pub const MENU_STATUS: &str = "status";
pub const MENU_RESTART: &str = "restart";
pub const MENU_CUES: &str = "cues";
pub const MENU_SPEECH: &str = "speech";
pub const MENU_QUIT: &str = "quit";

/// What a summon item's identifier starts with, before the widget identifier.
///
/// One namespace rather than one constant per widget: what the submenu offers
/// is read off the catalog at startup, so the identifiers cannot be a fixed
/// list. A widget identifier is already a protocol identifier, so it cannot
/// carry the separator and collide with a menu item of its own.
pub const MENU_SUMMON: &str = "summon:";

/// Returns the widget one summon item is for, or nothing if it is not one.
pub fn summoned(id: &str) -> Option<&str> {
    id.strip_prefix(MENU_SUMMON)
}

/// Returns the opaque colour that identifies one tray state.
///
/// The gruber state grammar the pill uses: yellow listening, brown
/// transcribing, niagara working, green speaking, wisteria attention, quartz
/// detached, red reserved for error and the mic ring. The two states nobody
/// is waiting on, starting and disconnected, wear the two dim greys.
pub fn state_color(state: &str) -> [u8; 3] {
    match state {
        "listening" => [0xFF, 0xDD, 0x33],
        "transcribing" => [0xCC, 0x8C, 0x3C],
        "working" => [0x96, 0xA6, 0xC8],
        "speaking" => [0x73, 0xC9, 0x36],
        "attention" => [0x9E, 0x95, 0xC7],
        "detached" => [0x6F, 0x62, 0x8C],
        "error" => [0xF4, 0x38, 0x41],
        "starting" => [0x6C, 0x77, 0x72],
        "disconnected" => [0x52, 0x49, 0x4E],
        _ => [0x95, 0xA9, 0x9F],
    }
}

/// Returns true when the state must show the recording privacy indicator.
pub fn shows_privacy_indicator(state: &str) -> bool {
    state == "listening"
}

/// Draws the tray icon for one state as RGBA bytes.
pub fn icon_rgba(state: &str) -> Vec<u8> {
    let color = state_color(state);
    let privacy = shows_privacy_indicator(state);
    let size = ICON_SIZE as i32;
    let center = (size - 1) as f32 / 2.0;
    let mut pixels = Vec::with_capacity((ICON_SIZE * ICON_SIZE * 4) as usize);
    for y in 0..size {
        for x in 0..size {
            let distance = (((x as f32 - center).powi(2)) + ((y as f32 - center).powi(2))).sqrt();
            let pixel = if privacy && distance > 12.0 && distance <= 15.0 {
                [0xF4, 0x38, 0x41, 0xFF]
            } else if distance <= 11.0 {
                [color[0], color[1], color[2], 0xFF]
            } else {
                [0, 0, 0, 0]
            };
            pixels.extend_from_slice(&pixel);
        }
    }
    pixels
}

/// Returns the tray icon image for one state.
pub fn icon(state: &str) -> Image<'static> {
    Image::new_owned(icon_rgba(state), ICON_SIZE, ICON_SIZE)
}

/// Returns the tray tooltip for one state and detail.
pub fn tooltip(state: &str, detail: &str) -> String {
    let headline = match state {
        "listening" => "Scufris is recording",
        "transcribing" => "Scufris is transcribing",
        "working" => "Scufris is working",
        "speaking" => "Scufris is speaking",
        "attention" => "Scufris needs you",
        "detached" => "Scufris is attached to a terminal",
        "error" => "Scufris reported an error",
        "starting" => "Scufris is starting",
        "disconnected" => "Scufris service unavailable",
        _ => "Scufris is idle",
    };
    if detail.is_empty() {
        headline.to_string()
    } else {
        format!("{headline}: {detail}")
    }
}

/// Returns the label of the sound cue switch for one enablement.
pub fn cues_label(enabled: bool) -> &'static str {
    if enabled {
        "Mute sound cues"
    } else {
        "Unmute sound cues"
    }
}

/// Returns the label of the speech switch for one mute.
///
/// Speech is the companion's to withhold because the speaker is the
/// companion's. The agent is not asked and is not told: it shapes the same
/// prose paragraph either way, and this decides whether it is played.
pub fn speech_label(muted: bool) -> &'static str {
    if muted {
        "Unmute Scufris"
    } else {
        "Mute Scufris"
    }
}

/// Builds the tray status menu.
///
/// `summonable` is the widgets the person can put on the desktop themselves,
/// as `(identifier, name)`. They get a submenu of their own rather than a row
/// each, because the menu's first job is still the four things that act on the
/// conversation and a fleet of widgets would bury them.
pub fn build_menu(
    app: &AppHandle,
    chat_available: bool,
    restart_available: bool,
    status: &str,
    summonable: &[(String, String)],
) -> tauri::Result<Menu<tauri::Wry>> {
    let mut widgets = tauri::menu::SubmenuBuilder::new(app, "Open a widget");
    for (id, name) in summonable {
        widgets = widgets.text(format!("{MENU_SUMMON}{id}"), name);
    }
    MenuBuilder::new(app)
        .text(MENU_STATUS, status)
        .separator()
        .text(MENU_HUD, "Show conversation")
        .text(MENU_VOICE, "Start voice input")
        // Not a fallback for the window above it. The terminal runs a whole Pi
        // session - tools, thinking, the lot - and the conversation window will
        // not be one for a long time. They are two tools, and this one needs a
        // terminal command configured before there is anything to open.
        .item(
            &tauri::menu::MenuItemBuilder::with_id(MENU_CHAT, "Open in terminal")
                .enabled(chat_available)
                .build(app)?,
        )
        .item(
            &tauri::menu::MenuItemBuilder::with_id(MENU_RESTART, "Restart backend")
                .enabled(restart_available)
                .build(app)?,
        )
        .text(MENU_SPEECH, speech_label(false))
        .text(MENU_CUES, cues_label(true))
        .separator()
        // Enabled only when there is something in it: an empty submenu that
        // opens onto nothing is worse than one that says it has nothing.
        .item(&widgets.enabled(!summonable.is_empty()).build()?)
        .separator()
        .text(MENU_QUIT, "Quit Scufris desktop")
        .build()
}

/// Applies one cue enablement to the switch label in the menu.
pub fn set_cues_label(menu: &Menu<tauri::Wry>, enabled: bool) -> Result<(), String> {
    let Some(MenuItemKind::MenuItem(item)) = menu.get(MENU_CUES) else {
        return Err("the sound cue switch is not in the menu".into());
    };
    item.set_text(cues_label(enabled))
        .map_err(|error| format!("the sound cue switch would not change: {error}"))
}

/// Applies one mute to the speech switch label in the menu.
pub fn set_speech_label(menu: &Menu<tauri::Wry>, muted: bool) -> Result<(), String> {
    let Some(MenuItemKind::MenuItem(item)) = menu.get(MENU_SPEECH) else {
        return Err("the speech switch is not in the menu".into());
    };
    item.set_text(speech_label(muted))
        .map_err(|error| format!("the speech switch would not change: {error}"))
}

/// Installs the tray icon and wires its click behaviour.
pub fn install(
    app: &AppHandle,
    menu: &Menu<tauri::Wry>,
    on_menu: impl Fn(&AppHandle, &str) + Send + Sync + 'static,
    on_left_click: impl Fn(&AppHandle) + Send + Sync + 'static,
) -> tauri::Result<TrayIcon> {
    TrayIconBuilder::with_id(TRAY_ID)
        .tooltip(tooltip("disconnected", ""))
        .icon(icon("disconnected"))
        .menu(menu)
        .show_menu_on_left_click(false)
        .on_menu_event(move |app, event| on_menu(app, event.id.as_ref()))
        .on_tray_icon_event(move |tray, event| {
            if let TrayIconEvent::Click {
                button: tauri::tray::MouseButton::Left,
                button_state: tauri::tray::MouseButtonState::Up,
                ..
            } = event
            {
                on_left_click(tray.app_handle());
            }
        })
        .build(app)
}

/// Applies one state to the installed tray icon and its status line.
///
/// The tray is the last thing that still speaks when the pill window does not,
/// so a tray that did not take the state says so rather than leaving the icon
/// on a state the companion has left behind.
pub fn apply(
    app: &AppHandle,
    menu: &Menu<tauri::Wry>,
    state: &str,
    detail: &str,
) -> Result<(), String> {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return Err("the tray icon is not installed".into());
    };
    tray.set_icon(Some(icon(state)))
        .map_err(|error| format!("the tray icon would not change: {error}"))?;
    tray.set_tooltip(Some(tooltip(state, detail)))
        .map_err(|error| format!("the tray tooltip would not change: {error}"))?;
    if let Some(MenuItemKind::MenuItem(item)) = menu.get(MENU_STATUS) {
        item.set_text(tooltip(state, detail))
            .map_err(|error| format!("the tray status line would not change: {error}"))?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    const STATES: [&str; 9] = [
        "idle",
        "starting",
        "listening",
        "transcribing",
        "working",
        "speaking",
        "attention",
        "detached",
        "error",
    ];

    #[test]
    fn every_tray_state_has_its_own_colour() {
        let colours: HashSet<[u8; 3]> = STATES
            .iter()
            .chain(["disconnected"].iter())
            .map(|state| state_color(state))
            .collect();
        assert_eq!(colours.len(), STATES.len() + 1);
    }

    #[test]
    fn the_icon_is_a_full_rgba_bitmap_that_differs_per_state() {
        let idle = icon_rgba("idle");
        assert_eq!(idle.len(), (ICON_SIZE * ICON_SIZE * 4) as usize);
        for state in STATES {
            assert_eq!(icon_rgba(state).len(), idle.len());
            if state != "idle" {
                assert_ne!(icon_rgba(state), idle, "{state} looks like idle");
            }
        }
    }

    #[test]
    fn recording_always_draws_the_privacy_indicator() {
        assert!(shows_privacy_indicator("listening"));
        assert!(!shows_privacy_indicator("working"));
        let listening = icon_rgba("listening");
        let ring_pixel = |x: u32, y: u32| {
            let offset = ((y * ICON_SIZE + x) * 4) as usize;
            [
                listening[offset],
                listening[offset + 1],
                listening[offset + 2],
                listening[offset + 3],
            ]
        };
        assert_eq!(ring_pixel(ICON_SIZE / 2, 2), [0xF4, 0x38, 0x41, 0xFF]);
        assert_eq!(
            icon_rgba("working")[((2 * ICON_SIZE + ICON_SIZE / 2) * 4) as usize + 3],
            0
        );
    }

    #[test]
    fn the_cue_switch_offers_the_opposite_of_the_current_enablement() {
        assert_eq!(cues_label(true), "Mute sound cues");
        assert_eq!(cues_label(false), "Unmute sound cues");
    }

    #[test]
    fn red_is_reserved_for_error_and_the_mic_ring() {
        assert_eq!(state_color("error"), [0xF4, 0x38, 0x41]);
        for state in STATES {
            if state != "error" {
                assert_ne!(state_color(state), state_color("error"), "{state}");
            }
        }
    }

    #[test]
    fn a_summon_item_names_its_widget_and_nothing_else_does() {
        assert_eq!(summoned(&format!("{MENU_SUMMON}timer")), Some("timer"));
        for id in [
            MENU_HUD,
            MENU_CHAT,
            MENU_VOICE,
            MENU_STATUS,
            MENU_RESTART,
            MENU_CUES,
            MENU_QUIT,
        ] {
            assert_eq!(summoned(id), None, "{id}");
        }
    }

    #[test]
    fn tooltips_name_the_state_and_carry_the_detail() {
        assert_eq!(
            tooltip("attention", "job 1 is blocked"),
            "Scufris needs you: job 1 is blocked"
        );
        assert_eq!(tooltip("idle", ""), "Scufris is idle");
        assert_eq!(
            tooltip("disconnected", "The Scufris service is unavailable."),
            "Scufris service unavailable: The Scufris service is unavailable."
        );
        assert_eq!(
            tooltip("detached", "a terminal has the session"),
            "Scufris is attached to a terminal: a terminal has the session"
        );
    }
}
