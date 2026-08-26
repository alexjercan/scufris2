//! Which widget surfaces exist, and where each one sits.
//!
//! Pure, in the style of [`crate::state`]: commands go in, actions come out,
//! and nothing here opens a window or writes to a socket. That is what lets the
//! shelf rules and the placement math be read off a test rather than off a
//! screen.
//!
//! The runtime is a small slot-based window manager, not free pixels. Exhibits
//! form a shelf directly above the pill, newest nearest the center, capped at
//! three; instruments take one of the four edge slots. Every surface knows its
//! slot, so every move is a step between two known places rather than a
//! collision to solve.

use std::collections::{BTreeMap, VecDeque};

use scufris_control::{ClientBody, Posture, SurfaceEvent};
use serde::Serialize;
use serde_json::Value;
use tauri::PhysicalPosition;

use crate::{pill, widgets::catalog::Catalog};

/// How many exhibits the shelf holds before the oldest one retires.
pub const SHELF_SLOTS: usize = 3;

/// The edge slots, in the order an instrument claims them.
///
/// The top corners first: the shelf lives above the pill at the bottom center,
/// so an instrument that lands high is an instrument the exhibits never reach.
pub const EDGE_SLOTS: [EdgeSlot; 4] = [
    EdgeSlot::TopRight,
    EdgeSlot::TopLeft,
    EdgeSlot::MidRight,
    EdgeSlot::MidLeft,
];

/// Gap between the top of the pill window and the bottom of the shelf, in
/// logical pixels. The pill's own bottom margin, halved: the shelf belongs to
/// the pill's layer, and reads as part of it rather than as a separate row.
const SHELF_GAP: f64 = 36.0;

/// Distance between the centers of two shelf columns, in logical pixels.
///
/// Fixed rather than measured, because the columns must not move when a widget
/// of a different width lands in one: the shelf is a row of places, and a place
/// that shifts with its occupant is not a place.
const SHELF_PITCH: f64 = 268.0;

/// Distance from a screen edge to an instrument parked against it, in logical
/// pixels.
const EDGE_MARGIN: f64 = 24.0;

/// A surface identifier, which doubles as the window label.
pub type SurfaceId = String;

/// One of the four instrument slots.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum EdgeSlot {
    /// The top left corner.
    TopLeft,
    /// The top right corner.
    TopRight,
    /// Halfway down the left edge.
    MidLeft,
    /// Halfway down the right edge.
    MidRight,
}

/// Where one surface sits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Slot {
    /// The shelf above the pill, by recency: rank zero is the newest and sits
    /// at the center.
    Shelf(usize),
    /// One of the four edges.
    Edge(EdgeSlot),
}

/// A window size in logical pixels.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    /// Width in logical pixels.
    pub width: f64,
    /// Height in logical pixels.
    pub height: f64,
}

/// What a surface's chrome says about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Life {
    /// The runtime owns it and it is current.
    Live,
    /// The person took it out of the runtime's hands.
    Pinned,
}

/// One open surface.
#[derive(Debug, Clone, PartialEq)]
pub struct Surface {
    /// The surface identifier, which is also the window label.
    pub id: SurfaceId,
    /// Which widget the surface is showing.
    pub widget: String,
    /// Whether Scufris opened it or the person did.
    pub posture: Posture,
    /// Where it sits.
    pub slot: Slot,
    /// Its window size.
    pub size: Size,
    /// True once the person pinned it. A pinned surface has left the runtime's
    /// hands: it is never moved, never retired to make room, and never cleared.
    pub owned: bool,
}

/// One thing the runtime is asked to do.
#[derive(Debug, Clone, PartialEq)]
pub enum Cmd {
    /// Open one widget. The daemon asked.
    Open {
        /// Correlation identifier the answer echoes.
        id: String,
        /// The shell the host reserved, whose label the surface takes.
        surface: SurfaceId,
        /// Widget to open.
        widget: String,
        /// Where the surface lives once it is open.
        posture: Posture,
        /// Widget-defined spawn payload.
        data: Value,
    },
    /// Send new data to one open surface. The daemon asked.
    Update {
        /// Correlation identifier the answer echoes.
        id: String,
        /// Surface to update.
        surface: SurfaceId,
        /// Widget-defined payload.
        data: Value,
    },
    /// Close one open surface. The daemon asked.
    Close {
        /// Correlation identifier the answer echoes.
        id: String,
        /// Surface to close.
        surface: SurfaceId,
    },
    /// Close every surface the runtime still owns. The daemon asked.
    Clear {
        /// Correlation identifier the answer echoes.
        id: String,
    },
    /// The person closed one surface with its own chrome tick.
    Dismissed {
        /// Surface the person closed.
        surface: SurfaceId,
    },
    /// The person used one surface's pin tick.
    Pin {
        /// Surface whose tick the person used.
        surface: SurfaceId,
    },
}

/// One thing the host has to carry out.
#[derive(Debug, Clone, PartialEq)]
pub enum Act {
    /// Give a warm shell its widget and put it on screen.
    Adopt {
        /// The surface being opened.
        surface: SurfaceId,
        /// Widget the shell becomes.
        widget: String,
        /// Title the chrome prints.
        name: String,
        /// Widget-defined spawn payload.
        data: Value,
        /// Where the window goes.
        slot: Slot,
        /// How big the window is.
        size: Size,
    },
    /// Put an open surface somewhere else.
    Move {
        /// The surface that moves.
        surface: SurfaceId,
        /// Where it goes.
        slot: Slot,
        /// How big the window is.
        size: Size,
    },
    /// Hand new data to an open surface.
    Update {
        /// The surface being updated.
        surface: SurfaceId,
        /// Widget-defined payload.
        data: Value,
    },
    /// Change what a surface's chrome says about it.
    Life {
        /// The surface whose chrome changes.
        surface: SurfaceId,
        /// What the chrome now says.
        life: Life,
    },
    /// Unmount a surface and hand its shell back to the pool.
    Retire {
        /// The surface that goes away.
        surface: SurfaceId,
    },
    /// Say something to the daemon.
    Report(ClientBody),
}

/// The surfaces, the shelf, and the edges.
#[derive(Debug, Default)]
pub struct Runtime {
    surfaces: BTreeMap<SurfaceId, Surface>,
    /// The shelf, newest first. Only surfaces the runtime still owns are on it.
    shelf: VecDeque<SurfaceId>,
}

impl Runtime {
    /// Returns an empty runtime.
    pub fn new() -> Self {
        Self::default()
    }

    /// Returns one open surface.
    pub fn surface(&self, id: &str) -> Option<&Surface> {
        self.surfaces.get(id)
    }

    /// Carries out one command and answers with what the host must do.
    pub fn apply(&mut self, catalog: &Catalog, cmd: Cmd) -> Vec<Act> {
        match cmd {
            Cmd::Open {
                id,
                surface,
                widget,
                posture,
                data,
            } => self.open(catalog, id, surface, widget, posture, data),
            Cmd::Update { id, surface, data } => self.update(id, surface, data),
            Cmd::Close { id, surface } => self.close(id, surface),
            Cmd::Clear { id } => self.clear(id),
            Cmd::Dismissed { surface } => self.dismissed(surface),
            Cmd::Pin { surface } => self.pin(surface),
        }
    }

    fn open(
        &mut self,
        catalog: &Catalog,
        id: String,
        surface: SurfaceId,
        widget: String,
        posture: Posture,
        data: Value,
    ) -> Vec<Act> {
        let Some(installed) = catalog.get(&widget) else {
            return vec![failed(
                id,
                "widget_not_found",
                format!("no widget named {widget}"),
            )];
        };
        let size = Size {
            width: f64::from(installed.width),
            height: f64::from(installed.height),
        };
        // An instrument that has nowhere to go is refused rather than stacked
        // on top of another one: the edges are named places, and two windows in
        // one place is the collision the slots exist to avoid.
        let slot = match posture {
            Posture::Exhibit => Slot::Shelf(0),
            Posture::Instrument => match self.free_edge() {
                Some(edge) => Slot::Edge(edge),
                None => {
                    return vec![failed(
                        id,
                        "no_free_slot",
                        "every instrument slot is taken".to_string(),
                    )];
                }
            },
        };
        self.surfaces.insert(
            surface.clone(),
            Surface {
                id: surface.clone(),
                widget: widget.clone(),
                posture,
                slot,
                size,
                owned: false,
            },
        );
        let mut acts = vec![Act::Adopt {
            surface: surface.clone(),
            widget,
            name: installed.name.clone(),
            data,
            slot,
            size,
        }];
        if posture == Posture::Exhibit {
            self.shelf.push_front(surface.clone());
            // The shelf is a row of three. A fourth exhibit does not shrink it;
            // it retires the one that has been up longest.
            acts.extend(self.crowd_out());
            acts.extend(self.reflow());
        }
        acts.push(Act::Report(ClientBody::WidgetOpened { id, surface }));
        acts
    }

    fn update(&mut self, id: String, surface: SurfaceId, data: Value) -> Vec<Act> {
        if !self.surfaces.contains_key(&surface) {
            return vec![failed(
                id,
                "surface_not_found",
                format!("{surface} is not open"),
            )];
        }
        vec![
            Act::Update { surface, data },
            Act::Report(ClientBody::WidgetDone { id }),
        ]
    }

    fn close(&mut self, id: String, surface: SurfaceId) -> Vec<Act> {
        // Closing a surface that is already gone is not a failure. The person
        // may have used the close tick a moment before the daemon asked, and
        // what was asked for - that surface is not on screen - is the case.
        let mut acts = self.retire(&surface);
        acts.push(Act::Report(ClientBody::WidgetDone { id }));
        acts
    }

    fn clear(&mut self, id: String) -> Vec<Act> {
        let owned: Vec<SurfaceId> = self
            .surfaces
            .values()
            .filter(|surface| !surface.owned)
            .map(|surface| surface.id.clone())
            .collect();
        let mut acts = Vec::new();
        for surface in owned {
            acts.extend(self.retire(&surface));
        }
        acts.push(Act::Report(ClientBody::WidgetDone { id }));
        acts
    }

    fn dismissed(&mut self, surface: SurfaceId) -> Vec<Act> {
        let mut acts = self.retire(&surface);
        // The daemon is told rather than asked: the person has already closed
        // the window, and this is what keeps the conversation's idea of what is
        // on screen from drifting away from what is.
        acts.push(Act::Report(ClientBody::WidgetEvent {
            surface,
            event: SurfaceEvent::Closed,
        }));
        acts
    }

    /// Hands one surface to the person, or takes it back.
    ///
    /// The tick reads both ways. Pinning takes the surface out of the runtime's
    /// hands: it stays where it is, nothing moves it again, and the shelf closes
    /// up behind it. Using the tick a second time gives it back, and an exhibit
    /// that comes back is the current one - it is what the person just chose to
    /// look at.
    fn pin(&mut self, surface: SurfaceId) -> Vec<Act> {
        let Some(open) = self.surfaces.get_mut(&surface) else {
            return Vec::new();
        };
        open.owned = !open.owned;
        let exhibit = open.posture == Posture::Exhibit;
        let life = if open.owned { Life::Pinned } else { Life::Live };
        let mut acts = vec![Act::Life {
            surface: surface.clone(),
            life,
        }];
        if life == Life::Pinned {
            self.shelf.retain(|id| id != &surface);
        } else if exhibit {
            self.shelf.push_front(surface);
            acts.extend(self.crowd_out());
        }
        acts.extend(self.reflow());
        acts
    }

    /// Retires exhibits until the shelf is back inside its three columns.
    fn crowd_out(&mut self) -> Vec<Act> {
        let mut acts = Vec::new();
        while self.shelf.len() > SHELF_SLOTS
            && let Some(oldest) = self.shelf.pop_back()
        {
            self.surfaces.remove(&oldest);
            acts.push(Act::Retire { surface: oldest });
        }
        acts
    }

    /// Takes one surface off the screen, whoever asked.
    fn retire(&mut self, surface: &str) -> Vec<Act> {
        if self.surfaces.remove(surface).is_none() {
            return Vec::new();
        }
        self.shelf.retain(|id| id != surface);
        let mut acts = vec![Act::Retire {
            surface: surface.to_string(),
        }];
        acts.extend(self.reflow());
        acts
    }

    /// Puts every shelf surface back in the column its recency earns it.
    fn reflow(&mut self) -> Vec<Act> {
        let mut acts = Vec::new();
        for (rank, id) in self.shelf.iter().enumerate() {
            let Some(surface) = self.surfaces.get_mut(id) else {
                continue;
            };
            let slot = Slot::Shelf(rank);
            if surface.slot == slot {
                continue;
            }
            surface.slot = slot;
            acts.push(Act::Move {
                surface: id.clone(),
                slot,
                size: surface.size,
            });
        }
        acts
    }

    /// Returns the first edge slot nothing is standing in.
    fn free_edge(&self) -> Option<EdgeSlot> {
        EDGE_SLOTS.into_iter().find(|edge| {
            !self
                .surfaces
                .values()
                .any(|surface| surface.slot == Slot::Edge(*edge))
        })
    }
}

/// Returns the answer for a command the runtime could not carry out.
fn failed(id: String, code: &str, detail: String) -> Act {
    Act::Report(ClientBody::WidgetFailed {
        id,
        code: code.to_string(),
        detail,
    })
}

/// One monitor, as the placement math sees it.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Monitor {
    /// Left edge in physical pixels.
    pub x: i32,
    /// Top edge in physical pixels.
    pub y: i32,
    /// Width in physical pixels.
    pub width: u32,
    /// Height in physical pixels.
    pub height: u32,
    /// Physical pixels per logical pixel.
    pub scale: f64,
}

/// Returns where one slot puts a window of this size on this monitor.
///
/// Pure, in the [`pill::bottom_center`] style, and clamped: a monitor smaller
/// than the widget puts the widget at the monitor's own corner rather than off
/// the screen the person is looking at.
pub fn place(slot: Slot, size: Size, monitor: &Monitor) -> PhysicalPosition<i32> {
    let width = (size.width * monitor.scale).round() as i32;
    let height = (size.height * monitor.scale).round() as i32;
    let margin = (EDGE_MARGIN * monitor.scale).round() as i32;
    let (x, y) = match slot {
        Slot::Shelf(rank) => {
            let pill = pill::bottom_center(
                monitor.x,
                monitor.y,
                monitor.width,
                monitor.height,
                monitor.scale,
            );
            let pitch = (SHELF_PITCH * monitor.scale).round() as i32;
            let gap = (SHELF_GAP * monitor.scale).round() as i32;
            let center = monitor.x + monitor.width as i32 / 2 + shelf_column(rank) * pitch;
            (center - width / 2, pill.y - gap - height)
        }
        Slot::Edge(edge) => {
            let left = monitor.x + margin;
            let right = monitor.x + monitor.width as i32 - width - margin;
            let top = monitor.y + margin;
            let middle = monitor.y + (monitor.height as i32 - height) / 2;
            match edge {
                EdgeSlot::TopLeft => (left, top),
                EdgeSlot::TopRight => (right, top),
                EdgeSlot::MidLeft => (left, middle),
                EdgeSlot::MidRight => (right, middle),
            }
        }
    };
    PhysicalPosition::new(
        clamp(x, monitor.x, monitor.x + monitor.width as i32 - width),
        clamp(y, monitor.y, monitor.y + monitor.height as i32 - height),
    )
}

/// Returns how far a shelf rank sits from the center column.
///
/// The newest exhibit holds the center and the older ones stand out from it,
/// right first: rank is distance from the middle, which is what "newest nearest
/// the center" means once the row has to be drawn.
fn shelf_column(rank: usize) -> i32 {
    match rank {
        0 => 0,
        1 => 1,
        _ => -1,
    }
}

/// Returns `value` inside the range, and the low end when the range is empty.
fn clamp(value: i32, low: i32, high: i32) -> i32 {
    value.clamp(low, high.max(low))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;

    use serde_json::json;

    use super::*;
    use crate::widgets::catalog::{Catalog, Source};

    const NOTE: &str = r#"
id = "note"
name = "Note"
description = "Show a short note"
width = 250
height = 110
"#;

    const CLOCK: &str = r#"
id = "clock"
name = "Clock"
description = "Show the time"
width = 200
height = 90
"#;

    fn catalog() -> Catalog {
        Catalog::build(&[
            Source {
                directory: "note",
                manifest: NOTE,
                script: "export function mount() {}",
            },
            Source {
                directory: "clock",
                manifest: CLOCK,
                script: "export function mount() {}",
            },
        ])
        .expect("the test catalog is well formed")
    }

    /// Opens one widget on a freshly reserved shell, the way the host does.
    ///
    /// The shell label the host reserved is the surface identifier: the pool
    /// mints it, never reuses it, and the runtime only records it.
    fn open(runtime: &mut Runtime, catalog: &Catalog, widget: &str, posture: Posture) -> Vec<Act> {
        let taken = SHELLS.with(|next| {
            let mut next = next.borrow_mut();
            *next += 1;
            *next
        });
        runtime.apply(
            catalog,
            Cmd::Open {
                id: format!("w-{taken}"),
                surface: format!("widget-{taken}"),
                widget: widget.to_string(),
                posture,
                data: json!({}),
            },
        )
    }

    thread_local! {
        /// How many shells this test has taken from its pretend pool.
        static SHELLS: RefCell<u32> = const { RefCell::new(0) };
    }

    fn opened(acts: &[Act]) -> String {
        acts.iter()
            .find_map(|act| match act {
                Act::Report(ClientBody::WidgetOpened { surface, .. }) => Some(surface.clone()),
                _ => None,
            })
            .expect("the open was answered with a surface")
    }

    #[test]
    fn an_exhibit_takes_the_center_of_the_shelf_and_pushes_the_others_out() {
        let catalog = catalog();
        let mut runtime = Runtime::new();
        let first = opened(&open(&mut runtime, &catalog, "note", Posture::Exhibit));
        assert_eq!(
            runtime.surface(&first).map(|s| s.slot),
            Some(Slot::Shelf(0))
        );
        let second = opened(&open(&mut runtime, &catalog, "note", Posture::Exhibit));
        // The newest holds the center; the one it displaced steps out by one.
        assert_eq!(
            runtime.surface(&second).map(|s| s.slot),
            Some(Slot::Shelf(0))
        );
        assert_eq!(
            runtime.surface(&first).map(|s| s.slot),
            Some(Slot::Shelf(1))
        );
    }

    #[test]
    fn a_fourth_exhibit_retires_the_one_that_has_been_up_longest() {
        let catalog = catalog();
        let mut runtime = Runtime::new();
        let first = opened(&open(&mut runtime, &catalog, "note", Posture::Exhibit));
        for _ in 0..2 {
            open(&mut runtime, &catalog, "note", Posture::Exhibit);
        }
        let acts = open(&mut runtime, &catalog, "note", Posture::Exhibit);
        assert!(acts.contains(&Act::Retire {
            surface: first.clone()
        }));
        assert!(runtime.surface(&first).is_none());
        assert_eq!(runtime.shelf.len(), SHELF_SLOTS);
    }

    #[test]
    fn instruments_fill_the_edges_and_a_fifth_is_refused_rather_than_stacked() {
        let catalog = catalog();
        let mut runtime = Runtime::new();
        let mut slots = Vec::new();
        for _ in 0..EDGE_SLOTS.len() {
            let surface = opened(&open(&mut runtime, &catalog, "clock", Posture::Instrument));
            slots.push(runtime.surface(&surface).expect("it is open").slot);
        }
        assert_eq!(
            slots,
            EDGE_SLOTS.map(Slot::Edge).to_vec(),
            "instruments claim the edges in a fixed order"
        );
        let acts = open(&mut runtime, &catalog, "clock", Posture::Instrument);
        assert!(matches!(
            acts.as_slice(),
            [Act::Report(ClientBody::WidgetFailed { code, .. })] if code == "no_free_slot"
        ));
    }

    #[test]
    fn a_widget_nobody_installed_is_refused_by_name() {
        let catalog = catalog();
        let mut runtime = Runtime::new();
        let acts = open(&mut runtime, &catalog, "weather", Posture::Exhibit);
        assert!(matches!(
            acts.as_slice(),
            [Act::Report(ClientBody::WidgetFailed { code, .. })] if code == "widget_not_found"
        ));
        assert!(runtime.surfaces.is_empty());
    }

    #[test]
    fn an_update_to_a_surface_that_is_not_open_fails_instead_of_going_nowhere() {
        let catalog = catalog();
        let mut runtime = Runtime::new();
        let acts = runtime.apply(
            &catalog,
            Cmd::Update {
                id: "w-1".into(),
                surface: "widget-9".into(),
                data: json!({ "text": "hello" }),
            },
        );
        assert!(matches!(
            acts.as_slice(),
            [Act::Report(ClientBody::WidgetFailed { code, .. })] if code == "surface_not_found"
        ));
    }

    #[test]
    fn closing_a_surface_that_is_already_gone_is_done_rather_than_failed() {
        // The person's close tick and the daemon's close race by design. What
        // was asked for - that surface is not on screen - is the case either
        // way, and a failure here would be a tool error for nothing.
        let catalog = catalog();
        let mut runtime = Runtime::new();
        let acts = runtime.apply(
            &catalog,
            Cmd::Close {
                id: "w-1".into(),
                surface: "widget-9".into(),
            },
        );
        assert_eq!(
            acts,
            vec![Act::Report(ClientBody::WidgetDone { id: "w-1".into() })]
        );
    }

    #[test]
    fn clearing_closes_the_runtimes_exhibits_and_leaves_a_pinned_widget_standing() {
        let catalog = catalog();
        let mut runtime = Runtime::new();
        let kept = opened(&open(&mut runtime, &catalog, "note", Posture::Exhibit));
        let gone = opened(&open(&mut runtime, &catalog, "note", Posture::Exhibit));
        runtime.apply(
            &catalog,
            Cmd::Pin {
                surface: kept.clone(),
            },
        );
        let acts = runtime.apply(&catalog, Cmd::Clear { id: "w-1".into() });
        assert!(acts.contains(&Act::Retire {
            surface: gone.clone()
        }));
        assert!(runtime.surface(&gone).is_none());
        assert!(runtime.surface(&kept).is_some());
    }

    #[test]
    fn pinning_takes_a_widget_off_the_shelf_and_the_rest_close_up_behind_it() {
        let catalog = catalog();
        let mut runtime = Runtime::new();
        let older = opened(&open(&mut runtime, &catalog, "note", Posture::Exhibit));
        let newer = opened(&open(&mut runtime, &catalog, "note", Posture::Exhibit));
        assert_eq!(
            runtime.surface(&older).map(|s| s.slot),
            Some(Slot::Shelf(1))
        );
        let acts = runtime.apply(
            &catalog,
            Cmd::Pin {
                surface: newer.clone(),
            },
        );
        assert!(acts.contains(&Act::Life {
            surface: newer.clone(),
            life: Life::Pinned
        }));
        // The pinned one stays where the person can see it; the shelf is now
        // one widget, which belongs at the center.
        assert_eq!(
            runtime.surface(&newer).map(|s| s.slot),
            Some(Slot::Shelf(0))
        );
        assert_eq!(
            runtime.surface(&older).map(|s| s.slot),
            Some(Slot::Shelf(0))
        );
        assert!(runtime.surface(&newer).expect("it is open").owned);
    }

    #[test]
    fn a_widget_the_person_closed_is_reported_rather_than_answered() {
        let catalog = catalog();
        let mut runtime = Runtime::new();
        let surface = opened(&open(&mut runtime, &catalog, "note", Posture::Exhibit));
        let acts = runtime.apply(
            &catalog,
            Cmd::Dismissed {
                surface: surface.clone(),
            },
        );
        assert!(acts.contains(&Act::Report(ClientBody::WidgetEvent {
            surface: surface.clone(),
            event: SurfaceEvent::Closed,
        })));
        assert!(runtime.surface(&surface).is_none());
    }

    #[test]
    fn the_pin_tick_reads_both_ways_and_a_returned_exhibit_is_the_current_one() {
        let catalog = catalog();
        let mut runtime = Runtime::new();
        let older = opened(&open(&mut runtime, &catalog, "note", Posture::Exhibit));
        let taken = opened(&open(&mut runtime, &catalog, "note", Posture::Exhibit));
        runtime.apply(
            &catalog,
            Cmd::Pin {
                surface: taken.clone(),
            },
        );
        let acts = runtime.apply(
            &catalog,
            Cmd::Pin {
                surface: taken.clone(),
            },
        );
        assert!(acts.contains(&Act::Life {
            surface: taken.clone(),
            life: Life::Live
        }));
        // Handing it back is choosing it: it takes the center, and the one that
        // held the center steps out.
        assert!(!runtime.surface(&taken).expect("it is open").owned);
        assert_eq!(
            runtime.surface(&taken).map(|s| s.slot),
            Some(Slot::Shelf(0))
        );
        assert_eq!(
            runtime.surface(&older).map(|s| s.slot),
            Some(Slot::Shelf(1))
        );
    }

    const MONITOR: Monitor = Monitor {
        x: 0,
        y: 0,
        width: 1920,
        height: 1080,
        scale: 1.0,
    };

    const CARD: Size = Size {
        width: 250.0,
        height: 110.0,
    };

    #[test]
    fn the_center_shelf_column_sits_over_the_pill_and_clear_of_it() {
        let position = place(Slot::Shelf(0), CARD, &MONITOR);
        let pill = pill::bottom_center(0, 0, 1920, 1080, 1.0);
        assert_eq!(position.x, 1920 / 2 - CARD.width as i32 / 2);
        assert_eq!(position.y + CARD.height as i32, pill.y - SHELF_GAP as i32);
    }

    #[test]
    fn the_shelf_spreads_out_from_the_center_without_two_widgets_in_one_column() {
        let columns: Vec<i32> = (0..SHELF_SLOTS)
            .map(|rank| place(Slot::Shelf(rank), CARD, &MONITOR).x)
            .collect();
        assert_eq!(columns[0], 1920 / 2 - CARD.width as i32 / 2);
        assert_eq!(columns[1] - columns[0], SHELF_PITCH as i32);
        assert_eq!(columns[0] - columns[2], SHELF_PITCH as i32);
        // The pitch is wider than the widest card, so neighbours never overlap.
        const { assert!(SHELF_PITCH > 250.0) };
    }

    #[test]
    fn the_edges_hold_their_margin_and_follow_the_monitor_offset_and_scale() {
        let monitor = Monitor {
            x: 1920,
            y: -120,
            width: 2560,
            height: 1440,
            scale: 2.0,
        };
        let margin = (EDGE_MARGIN * 2.0) as i32;
        let width = (CARD.width * 2.0) as i32;
        let height = (CARD.height * 2.0) as i32;
        assert_eq!(
            place(Slot::Edge(EdgeSlot::TopLeft), CARD, &monitor),
            PhysicalPosition::new(1920 + margin, -120 + margin)
        );
        assert_eq!(
            place(Slot::Edge(EdgeSlot::TopRight), CARD, &monitor),
            PhysicalPosition::new(1920 + 2560 - width - margin, -120 + margin)
        );
        assert_eq!(
            place(Slot::Edge(EdgeSlot::MidRight), CARD, &monitor),
            PhysicalPosition::new(1920 + 2560 - width - margin, -120 + (1440 - height) / 2)
        );
    }

    #[test]
    fn a_monitor_smaller_than_the_widget_never_places_it_off_screen() {
        let monitor = Monitor {
            x: 40,
            y: 60,
            width: 120,
            height: 80,
            scale: 1.0,
        };
        for slot in [
            Slot::Shelf(0),
            Slot::Shelf(2),
            Slot::Edge(EdgeSlot::TopRight),
        ] {
            let position = place(slot, CARD, &monitor);
            assert_eq!(
                position,
                PhysicalPosition::new(40, 60),
                "{slot:?} left the monitor"
            );
        }
    }
}
