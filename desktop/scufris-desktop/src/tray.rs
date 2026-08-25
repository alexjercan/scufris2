//! Tray icon, tray state colours, and the tray status menu.
//!
//! The icon is drawn from the state name so every state has a distinct colour
//! without shipping one image per state. Recording always draws the red privacy
//! ring, whatever the daemon reports.

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
pub const MENU_CHAT: &str = "chat";
pub const MENU_VOICE: &str = "voice";
pub const MENU_STATUS: &str = "status";
pub const MENU_RESTART: &str = "restart";
pub const MENU_QUIT: &str = "quit";

/// Returns the opaque colour that identifies one tray state.
pub fn state_color(state: &str) -> [u8; 3] {
    match state {
        "listening" => [0xE0, 0x3B, 0x3B],
        "transcribing" => [0xE0, 0x8B, 0x2B],
        "working" => [0x3B, 0x8B, 0xE0],
        "speaking" => [0x35, 0xB5, 0x8A],
        "attention" => [0xE0, 0xC0, 0x2B],
        "error" => [0xC0, 0x2B, 0x2B],
        "disconnected" => [0x6B, 0x6B, 0x6B],
        _ => [0xC8, 0xC8, 0xC8],
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
                [0xFF, 0x30, 0x30, 0xFF]
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
        "error" => "Scufris reported an error",
        "disconnected" => "Scufris backend unavailable",
        _ => "Scufris is idle",
    };
    if detail.is_empty() {
        headline.to_string()
    } else {
        format!("{headline}: {detail}")
    }
}

/// Builds the tray status menu.
pub fn build_menu(
    app: &AppHandle,
    chat_available: bool,
    restart_available: bool,
    status: &str,
) -> tauri::Result<Menu<tauri::Wry>> {
    MenuBuilder::new(app)
        .text(MENU_STATUS, status)
        .separator()
        .item(
            &tauri::menu::MenuItemBuilder::with_id(MENU_CHAT, "Open chat")
                .enabled(chat_available)
                .build(app)?,
        )
        .text(MENU_VOICE, "Start voice input")
        .item(
            &tauri::menu::MenuItemBuilder::with_id(MENU_RESTART, "Restart backend")
                .enabled(restart_available)
                .build(app)?,
        )
        .separator()
        .text(MENU_QUIT, "Quit Scufris desktop")
        .build()
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

    const STATES: [&str; 7] = [
        "idle",
        "listening",
        "transcribing",
        "working",
        "speaking",
        "attention",
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
        assert_eq!(ring_pixel(ICON_SIZE / 2, 2), [0xFF, 0x30, 0x30, 0xFF]);
        assert_eq!(
            icon_rgba("working")[((2 * ICON_SIZE + ICON_SIZE / 2) * 4) as usize + 3],
            0
        );
    }

    #[test]
    fn tooltips_name_the_state_and_carry_the_detail() {
        assert_eq!(
            tooltip("attention", "job 1 is blocked"),
            "Scufris needs you: job 1 is blocked"
        );
        assert_eq!(tooltip("idle", ""), "Scufris is idle");
        assert_eq!(
            tooltip("disconnected", "The Scufris backend is unavailable."),
            "Scufris backend unavailable: The Scufris backend is unavailable."
        );
    }
}
