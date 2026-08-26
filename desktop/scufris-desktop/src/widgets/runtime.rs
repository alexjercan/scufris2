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

use std::{
    collections::{BTreeMap, BTreeSet, VecDeque},
    time::Duration,
};

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

/// How long a dim exhibit stays up before it retires.
///
/// Counted only while its clock runs, which is what makes a minute here a
/// minute of the conversation having moved on rather than a minute of wall
/// time. Long enough to read a panel the sentence that opened it has already
/// passed; short enough that an afternoon does not leave a wall of them.
pub const GRACE: Duration = Duration::from_secs(60);

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
    /// The runtime owns it and the conversation is still about it.
    Live,
    /// The runtime owns it and the conversation has moved on. It retires when
    /// its grace runs out.
    Dim,
    /// The person took it out of the runtime's hands.
    Pinned,
}

/// What the backend behind a surface is doing, if it has one.
///
/// Separate from [`Life`] rather than folded into it, because the two say
/// different things and hold at the same time: a panel the person pinned can
/// still be showing numbers from a process that died, and a dim exhibit whose
/// sampler is fine is not the same as a bright one whose sampler is gone.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Health {
    /// Writing, and recently.
    Fresh,
    /// Still running, but it has not written for a while. The number on screen
    /// is old rather than wrong.
    Stale,
    /// Gone. The number on screen will never change again.
    Dead,
}

/// Why the runtime's clocks are stopped, if they are.
///
/// A set rather than a flag, because the reasons overlap and each one is lifted
/// by the thing that raised it. A microphone closing while Scufris is already
/// answering must not start a grace that the answer is still stopping.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Still {
    /// Scufris is speaking. The person is listening rather than reading.
    Speech,
    /// The microphone is open. The person is talking rather than reading.
    Microphone,
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
    /// What its chrome says about it.
    pub life: Life,
    /// True once Scufris opened or updated it during the turn now running.
    pub cited: bool,
    /// True while the pointer is over it. A panel somebody is reading does not
    /// age, and does not dim under them when a turn ends.
    pub hovered: bool,
    /// How long it has been dim, counting only the time its clock ran.
    pub aging: Duration,
    /// Which backend feeds it, if any. A widget with no backend shows only
    /// what the open and the updates carried.
    pub backend: Option<String>,
    /// The payload the open carried. Kept because it is half of what finds the
    /// running backend, which is what a restart needs to name.
    pub spawn: Value,
    /// How often its widget says a reading should arrive.
    pub cadence: Duration,
    /// What its backend is doing, if it has one.
    pub health: Health,
}

impl Surface {
    /// True while the runtime still ages this surface out and clears it.
    ///
    /// An exhibit is Scufris showing something, and what Scufris put up it also
    /// takes away. An instrument is a panel the person asked to keep, and so is
    /// an exhibit they pinned: neither one is the runtime's to retire.
    fn transient(&self) -> bool {
        self.posture == Posture::Exhibit && !self.owned
    }
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
    /// One turn of the conversation ended.
    TurnEnded,
    /// Time passed. Only the runtime's own clocks care.
    Sweep {
        /// How long since the last sweep.
        elapsed: Duration,
    },
    /// The pointer moved onto one surface, or off it.
    Hover {
        /// Surface the pointer moved over or off.
        surface: SurfaceId,
        /// True when it moved on, false when it moved off.
        over: bool,
    },
    /// Stop or start every clock the runtime keeps, for one reason.
    Freeze {
        /// What is stopping them, or letting them run again.
        reason: Still,
        /// True when this reason now holds.
        stopped: bool,
    },
    /// Take the runtime's own widgets off the screen with the pill, or bring
    /// them back.
    Conceal {
        /// True when the layer goes down.
        hidden: bool,
    },
    /// A backend wrote a reading for one surface.
    Feed {
        /// Surface whose backend wrote it.
        surface: SurfaceId,
        /// What the backend wrote.
        data: Value,
    },
    /// A backend's health changed for one surface.
    Health {
        /// Surface whose backend it is.
        surface: SurfaceId,
        /// What it now is.
        health: Health,
    },
    /// The person used one surface's restart tick.
    Restart {
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
        /// True when the layer is down, so the window is sized and loaded and
        /// waits off the screen instead of coming up.
        hidden: bool,
    },
    /// Take an open surface off the screen without retiring it, or put it back.
    Conceal {
        /// The surface that goes down or comes back.
        surface: SurfaceId,
        /// True when it goes down.
        hidden: bool,
        /// Where it stands when it comes back.
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
    /// Put one surface on every workspace, or bring it back to the current one.
    Stick {
        /// The surface that follows the person, or stops.
        surface: SurfaceId,
        /// True while it belongs on every workspace.
        sticky: bool,
    },
    /// Tell one surface's chrome that a tick could not be carried out.
    Refuse {
        /// The surface whose tick was refused.
        surface: SurfaceId,
        /// What the person reads.
        detail: String,
    },
    /// Start reading a backend for one surface, or start it over.
    Subscribe {
        /// The surface that reads it.
        surface: SurfaceId,
        /// Which backend.
        backend: String,
        /// The payload it is started with, which is half of what finds a
        /// process already answering the same question.
        spawn: Value,
        /// How often the widget says a reading should arrive.
        cadence: Duration,
        /// True when whatever is running has to be stopped first. The person
        /// used the restart tick, and a shared process comes back for every
        /// panel that was reading it.
        restart: bool,
    },
    /// Stop reading a backend for one surface.
    Unsubscribe {
        /// The surface that stops reading.
        surface: SurfaceId,
    },
    /// Change what a surface's chrome says about its backend.
    Health {
        /// The surface whose chrome changes.
        surface: SurfaceId,
        /// What it now says.
        health: Health,
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
    /// Why the clocks are stopped, if they are. Time the person is not looking
    /// at a panel is not time the panel has been up.
    frozen: BTreeSet<Still>,
    /// True while the layer is off the screen with the pill.
    hidden: bool,
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
            Cmd::TurnEnded => self.turn_ended(),
            Cmd::Sweep { elapsed } => self.sweep(elapsed),
            Cmd::Hover { surface, over } => self.hover(surface, over),
            Cmd::Freeze { reason, stopped } => {
                if stopped {
                    self.frozen.insert(reason);
                } else {
                    self.frozen.remove(&reason);
                }
                Vec::new()
            }
            Cmd::Conceal { hidden } => self.conceal(hidden),
            Cmd::Feed { surface, data } => self.feed(surface, data),
            Cmd::Health { surface, health } => self.health(surface, health),
            Cmd::Restart { surface } => self.restart(surface),
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
        // Kept as well as handed to the widget: it is half of what finds the
        // process behind a live panel, and the restart tick has to name it
        // again long after the open is over.
        let spawn = data.clone();
        self.surfaces.insert(
            surface.clone(),
            Surface {
                id: surface.clone(),
                widget: widget.clone(),
                posture,
                slot,
                size,
                owned: false,
                life: Life::Live,
                // Opening one is the strongest citation there is. It survives
                // the end of the turn that opened it and dims at the next.
                cited: true,
                hovered: false,
                aging: Duration::ZERO,
                backend: installed.backend.clone(),
                spawn: spawn.clone(),
                cadence: installed.cadence,
                health: Health::Fresh,
            },
        );
        let exhibit = posture == Posture::Exhibit;
        // A widget opened while the layer is down waits behind it rather than
        // flashing onto a desktop the person put the pill away from.
        let hidden = self.hidden && exhibit;
        let mut acts = vec![Act::Adopt {
            surface: surface.clone(),
            widget,
            name: installed.name.clone(),
            data,
            slot,
            size,
            hidden,
        }];
        if !hidden {
            acts.push(Act::Stick {
                surface: surface.clone(),
                sticky: exhibit,
            });
        }
        // After the adopt, so the window exists by the time the first reading
        // is beaten out of the coalescer.
        if let Some(backend) = installed.backend.clone() {
            acts.push(Act::Subscribe {
                surface: surface.clone(),
                backend,
                spawn,
                cadence: installed.cadence,
                restart: false,
            });
        }
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
        let Some(open) = self.surfaces.get_mut(&surface) else {
            return vec![failed(
                id,
                "surface_not_found",
                format!("{surface} is not open"),
            )];
        };
        open.cited = true;
        // Scufris speaking about a panel again is what brings it back, even
        // when the data it hands over is the data already on it. Where the
        // panel sits does not change: the shelf is a row of places, and one
        // that reshuffled itself under a sentence would be harder to follow
        // than one that simply brightened.
        let mut acts = Vec::new();
        if open.life == Life::Dim {
            open.life = Life::Live;
            open.aging = Duration::ZERO;
            acts.push(Act::Life {
                surface: surface.clone(),
                life: Life::Live,
            });
        }
        acts.push(Act::Update { surface, data });
        acts.push(Act::Report(ClientBody::WidgetDone { id }));
        acts
    }

    /// Hands one backend's reading to the surface reading it.
    ///
    /// A reading is not a citation. Scufris naming a panel is what says the
    /// conversation is still about it; a sampler writing its line every second
    /// says only that the machine is on. A live graph that revived itself would
    /// be the one exhibit that never ages out, and the shelf would fill with
    /// them.
    fn feed(&mut self, surface: SurfaceId, data: Value) -> Vec<Act> {
        if !self.surfaces.contains_key(&surface) {
            return Vec::new();
        }
        vec![Act::Update { surface, data }]
    }

    /// Records what a surface's backend is doing.
    fn health(&mut self, surface: SurfaceId, health: Health) -> Vec<Act> {
        let Some(open) = self.surfaces.get_mut(&surface) else {
            return Vec::new();
        };
        if open.health == health {
            return Vec::new();
        }
        open.health = health;
        vec![Act::Health { surface, health }]
    }

    /// Starts one surface's backend over, because the person asked.
    fn restart(&mut self, surface: SurfaceId) -> Vec<Act> {
        let Some(open) = self.surfaces.get_mut(&surface) else {
            return Vec::new();
        };
        let Some(backend) = open.backend.clone() else {
            return vec![Act::Refuse {
                surface,
                detail: "nothing feeds this".into(),
            }];
        };
        // Said before the process is started rather than after it writes: the
        // tick has to answer immediately, and the panel goes back to saying
        // dead on its own if the new process does not last.
        open.health = Health::Fresh;
        let spawn = open.spawn.clone();
        let cadence = open.cadence;
        vec![
            Act::Health {
                surface: surface.clone(),
                health: Health::Fresh,
            },
            Act::Subscribe {
                surface,
                backend,
                spawn,
                cadence,
                restart: true,
            },
        ]
    }

    /// Dims every exhibit the turn that just ended never mentioned.
    ///
    /// The turn boundary is what stands in for "the conversation moved on".
    /// Everything Scufris opened or updated while the turn ran is the turn's
    /// subject and stays bright; everything else is from a subject that is over.
    fn turn_ended(&mut self) -> Vec<Act> {
        let mut acts = Vec::new();
        for surface in self.surfaces.values_mut() {
            let dimming = surface.transient()
                && !surface.cited
                && !surface.hovered
                && surface.life == Life::Live;
            surface.cited = false;
            if !dimming {
                continue;
            }
            surface.life = Life::Dim;
            surface.aging = Duration::ZERO;
            acts.push(Act::Life {
                surface: surface.id.clone(),
                life: Life::Dim,
            });
        }
        acts
    }

    /// Takes the runtime's own widgets down with the pill, or brings them back.
    ///
    /// One layer, one gesture. The pill and everything the runtime put beside
    /// it go down together and come back exactly as they were: nothing is
    /// retired, no widget is unmounted, and the clocks stop for as long as the
    /// layer is off the screen. What the person kept is not on this layer any
    /// more, which is what the pin tick did to it.
    fn conceal(&mut self, hidden: bool) -> Vec<Act> {
        if self.hidden == hidden {
            return Vec::new();
        }
        self.hidden = hidden;
        let mut acts = Vec::new();
        for surface in self.surfaces.values().filter(|surface| surface.transient()) {
            acts.push(Act::Conceal {
                surface: surface.id.clone(),
                hidden,
                slot: surface.slot,
                size: surface.size,
            });
            if !hidden {
                // A window manager unmanages a window when it unmaps and takes
                // its state with it, so a panel coming back has to be told
                // again that it belongs on every workspace.
                acts.push(Act::Stick {
                    surface: surface.id.clone(),
                    sticky: true,
                });
            }
        }
        acts
    }

    /// Ages the dim exhibits, and retires the ones whose grace ran out.
    fn sweep(&mut self, elapsed: Duration) -> Vec<Act> {
        if self.hidden || !self.frozen.is_empty() {
            return Vec::new();
        }
        let mut spent = Vec::new();
        for surface in self.surfaces.values_mut() {
            if surface.life != Life::Dim || surface.hovered {
                continue;
            }
            surface.aging = surface.aging.saturating_add(elapsed);
            if surface.aging >= GRACE {
                spent.push(surface.id.clone());
            }
        }
        let mut acts = Vec::new();
        for surface in spent {
            // Silently. An exhibit needs no closing - that is what makes it an
            // exhibit - and a line in the transcript for every panel that went
            // quiet would turn the thing that needs no closing into the thing
            // that reports itself.
            acts.extend(self.retire(&surface));
        }
        acts
    }

    /// Records the pointer arriving over one surface, or leaving it.
    fn hover(&mut self, surface: SurfaceId, over: bool) -> Vec<Act> {
        let Some(open) = self.surfaces.get_mut(&surface) else {
            return Vec::new();
        };
        open.hovered = over;
        if !over || open.life != Life::Dim {
            return Vec::new();
        }
        // Somebody is reading it, which says the same thing a citation says:
        // this panel is current again.
        open.life = Life::Live;
        open.aging = Duration::ZERO;
        vec![Act::Life {
            surface,
            life: Life::Live,
        }]
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
        let ours: Vec<SurfaceId> = self
            .surfaces
            .values()
            .filter(|surface| surface.transient())
            .map(|surface| surface.id.clone())
            .collect();
        let mut acts = Vec::new();
        for surface in ours {
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
    /// Pinning promotes an exhibit into an instrument. It leaves the shelf for
    /// one of the four edge slots and stops being sticky, so it lands on the
    /// workspace the person is on rather than following them everywhere. It has
    /// to leave the shelf's columns and not merely leave the shelf: a column it
    /// kept is a column the next reflow moves a live exhibit into, and two
    /// windows in one place is the collision the slots exist to avoid.
    ///
    /// The tick reads both ways. Using it again gives the surface back, and an
    /// exhibit that comes back is the current one - it is what the person just
    /// chose to look at.
    fn pin(&mut self, surface: SurfaceId) -> Vec<Act> {
        let Some(open) = self.surfaces.get(&surface) else {
            return Vec::new();
        };
        if open.owned {
            return self.release(surface);
        }
        // Only a surface still on the shelf has to be moved. An instrument the
        // person pins is already standing in an edge slot of its own.
        let slot = if self.shelf.contains(&surface) {
            match self.free_edge() {
                Some(edge) => Slot::Edge(edge),
                None => {
                    return vec![Act::Refuse {
                        surface,
                        detail: "every slot is taken".into(),
                    }];
                }
            }
        } else {
            open.slot
        };
        let Some(open) = self.surfaces.get_mut(&surface) else {
            return Vec::new();
        };
        open.owned = true;
        open.life = Life::Pinned;
        open.aging = Duration::ZERO;
        let moving = open.slot != slot;
        open.slot = slot;
        let size = open.size;
        self.shelf.retain(|id| id != &surface);
        let mut acts = vec![Act::Life {
            surface: surface.clone(),
            life: Life::Pinned,
        }];
        if moving {
            acts.push(Act::Move {
                surface: surface.clone(),
                slot,
                size,
            });
        }
        acts.push(Act::Stick {
            surface,
            sticky: false,
        });
        acts.extend(self.reflow());
        acts
    }

    /// Gives one surface back to the runtime.
    fn release(&mut self, surface: SurfaceId) -> Vec<Act> {
        let Some(open) = self.surfaces.get_mut(&surface) else {
            return Vec::new();
        };
        open.owned = false;
        open.life = Life::Live;
        // Handing it back is choosing it: the minute starts over, and the turn
        // now running has been told about it.
        open.aging = Duration::ZERO;
        open.cited = true;
        let exhibit = open.posture == Posture::Exhibit;
        let mut acts = vec![
            Act::Life {
                surface: surface.clone(),
                life: Life::Live,
            },
            Act::Stick {
                surface: surface.clone(),
                sticky: exhibit,
            },
        ];
        if exhibit {
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
            let gone = self.surfaces.remove(&oldest);
            if gone.is_some_and(|surface| surface.backend.is_some()) {
                acts.push(Act::Unsubscribe {
                    surface: oldest.clone(),
                });
            }
            acts.push(Act::Retire { surface: oldest });
        }
        acts
    }

    /// Takes one surface off the screen, whoever asked.
    fn retire(&mut self, surface: &str) -> Vec<Act> {
        let Some(gone) = self.surfaces.remove(surface) else {
            return Vec::new();
        };
        self.shelf.retain(|id| id != surface);
        let mut acts = Vec::new();
        if gone.backend.is_some() {
            // Before the retire, so the last panel reading a sampler stops it
            // rather than leaving a process writing to a window that is gone.
            acts.push(Act::Unsubscribe {
                surface: surface.to_string(),
            });
        }
        acts.push(Act::Retire {
            surface: surface.to_string(),
        });
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

    /// A widget with something feeding it, which the other two do not have.
    const GAUGE: &str = r#"
id = "gauge"
name = "Gauge"
description = "Show a number that changes"
width = 250
height = 130
backend = "sampler"
cadence = 500
"#;

    fn catalog() -> Catalog {
        Catalog::build(
            &[
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
                Source {
                    directory: "gauge",
                    manifest: GAUGE,
                    script: "export function mount() {}",
                },
            ],
            &["sampler"],
        )
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
    fn opening_a_widget_with_something_feeding_it_starts_reading_it() {
        let catalog = catalog();
        let mut runtime = Runtime::new();
        let acts = runtime.apply(
            &catalog,
            Cmd::Open {
                id: "w-1".into(),
                surface: "widget-1".into(),
                widget: "gauge".into(),
                posture: Posture::Exhibit,
                data: json!({ "every": 2 }),
            },
        );
        assert!(acts.contains(&Act::Subscribe {
            surface: "widget-1".into(),
            backend: "sampler".into(),
            // The payload the open carried, not a summary of it: it is half of
            // what finds a process already answering the same question.
            spawn: json!({ "every": 2 }),
            cadence: Duration::from_millis(500),
            restart: false,
        }));
        // And it comes after the window, so the first reading has somewhere to
        // land.
        let adopt = acts
            .iter()
            .position(|act| matches!(act, Act::Adopt { .. }))
            .expect("the shell was adopted");
        let subscribe = acts
            .iter()
            .position(|act| matches!(act, Act::Subscribe { .. }))
            .expect("the backend was subscribed to");
        assert!(adopt < subscribe);
    }

    #[test]
    fn a_widget_with_nothing_feeding_it_starts_nothing() {
        let catalog = catalog();
        let mut runtime = Runtime::new();
        let acts = open(&mut runtime, &catalog, "note", Posture::Exhibit);
        assert!(!acts.iter().any(|act| matches!(act, Act::Subscribe { .. })));
    }

    #[test]
    fn the_panel_going_away_is_what_stops_reading_its_backend() {
        // However it goes: closed by the daemon, closed by the person, aged
        // out, or pushed off the shelf. A process left writing to a window that
        // is gone is the failure the supervisor exists to prevent, and the
        // runtime is where every one of those paths meets.
        let catalog = catalog();
        for close in [
            Cmd::Close {
                id: "w-close".into(),
                surface: "widget-1".into(),
            },
            Cmd::Dismissed {
                surface: "widget-1".into(),
            },
            Cmd::Clear {
                id: "w-clear".into(),
            },
        ] {
            let mut runtime = Runtime::new();
            runtime.apply(
                &catalog,
                Cmd::Open {
                    id: "w-1".into(),
                    surface: "widget-1".into(),
                    widget: "gauge".into(),
                    posture: Posture::Exhibit,
                    data: json!({}),
                },
            );
            let acts = runtime.apply(&catalog, close.clone());
            let stops = acts
                .iter()
                .position(|act| {
                    act == &Act::Unsubscribe {
                        surface: "widget-1".into(),
                    }
                })
                .unwrap_or_else(|| panic!("{close:?} left the backend running: {acts:?}"));
            let retires = acts
                .iter()
                .position(|act| {
                    act == &Act::Retire {
                        surface: "widget-1".into(),
                    }
                })
                .expect("the surface retired");
            assert!(stops < retires, "the window went before its backend did");
        }
    }

    #[test]
    fn an_exhibit_pushed_off_the_shelf_stops_reading_too() {
        let catalog = catalog();
        let mut runtime = Runtime::new();
        let first = opened(&open(&mut runtime, &catalog, "gauge", Posture::Exhibit));
        for _ in 0..2 {
            open(&mut runtime, &catalog, "note", Posture::Exhibit);
        }
        let acts = open(&mut runtime, &catalog, "note", Posture::Exhibit);
        assert!(acts.contains(&Act::Unsubscribe {
            surface: first.clone()
        }));
    }

    #[test]
    fn a_reading_is_not_a_citation() {
        // A sampler writing its line every second says the machine is on, not
        // that the conversation is still about the panel. An exhibit that
        // revived itself on its own data would be the one that never ages out,
        // and a shelf of live graphs would never make room for anything.
        let catalog = catalog();
        let mut runtime = Runtime::new();
        let gauge = opened(&open(&mut runtime, &catalog, "gauge", Posture::Exhibit));
        runtime.apply(&catalog, Cmd::TurnEnded);
        assert_eq!(
            runtime.apply(&catalog, Cmd::TurnEnded),
            vec![Act::Life {
                surface: gauge.clone(),
                life: Life::Dim,
            }]
        );
        let acts = runtime.apply(
            &catalog,
            Cmd::Feed {
                surface: gauge.clone(),
                data: json!({ "cpu": 12 }),
            },
        );
        assert_eq!(
            acts,
            vec![Act::Update {
                surface: gauge.clone(),
                data: json!({ "cpu": 12 }),
            }]
        );
        assert_eq!(
            runtime.surface(&gauge).map(|open| open.life),
            Some(Life::Dim)
        );
        // And its grace still runs out.
        assert!(
            runtime
                .apply(&catalog, Cmd::Sweep { elapsed: GRACE })
                .contains(&Act::Retire {
                    surface: gauge.clone()
                })
        );
    }

    #[test]
    fn a_reading_for_a_panel_that_is_gone_goes_nowhere() {
        let catalog = catalog();
        let mut runtime = Runtime::new();
        assert!(
            runtime
                .apply(
                    &catalog,
                    Cmd::Feed {
                        surface: "widget-9".into(),
                        data: json!({}),
                    }
                )
                .is_empty()
        );
    }

    #[test]
    fn a_backend_in_trouble_is_said_once_and_not_on_every_beat() {
        let catalog = catalog();
        let mut runtime = Runtime::new();
        let gauge = opened(&open(&mut runtime, &catalog, "gauge", Posture::Exhibit));
        let said = |runtime: &mut Runtime, health| {
            runtime.apply(
                &catalog,
                Cmd::Health {
                    surface: gauge.clone(),
                    health,
                },
            )
        };
        assert_eq!(
            said(&mut runtime, Health::Stale),
            vec![Act::Health {
                surface: gauge.clone(),
                health: Health::Stale,
            }]
        );
        assert!(said(&mut runtime, Health::Stale).is_empty());
        assert_eq!(
            said(&mut runtime, Health::Dead),
            vec![Act::Health {
                surface: gauge.clone(),
                health: Health::Dead,
            }]
        );
    }

    #[test]
    fn the_restart_tick_answers_before_the_process_is_back() {
        // The tick has to say something immediately. If the new process does
        // not last, the next beat puts the panel back to saying dead on its
        // own, which is a better order than a tick that appears to do nothing
        // for a second.
        let catalog = catalog();
        let mut runtime = Runtime::new();
        let gauge = opened(&open(&mut runtime, &catalog, "gauge", Posture::Exhibit));
        runtime.apply(
            &catalog,
            Cmd::Health {
                surface: gauge.clone(),
                health: Health::Dead,
            },
        );
        assert_eq!(
            runtime.apply(
                &catalog,
                Cmd::Restart {
                    surface: gauge.clone()
                }
            ),
            vec![
                Act::Health {
                    surface: gauge.clone(),
                    health: Health::Fresh,
                },
                Act::Subscribe {
                    surface: gauge.clone(),
                    backend: "sampler".into(),
                    spawn: json!({}),
                    cadence: Duration::from_millis(500),
                    // The one that stops what is running first, for every panel
                    // that was reading it.
                    restart: true,
                },
            ]
        );
    }

    #[test]
    fn a_restart_on_a_panel_with_nothing_feeding_it_says_so() {
        let catalog = catalog();
        let mut runtime = Runtime::new();
        let note = opened(&open(&mut runtime, &catalog, "note", Posture::Exhibit));
        assert!(matches!(
            runtime
                .apply(&catalog, Cmd::Restart {
                    surface: note.clone()
                })
                .as_slice(),
            [Act::Refuse { surface, .. }] if surface == &note
        ));
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
    fn pinning_promotes_an_exhibit_out_of_the_column_the_shelf_then_closes_up() {
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
        // The pinned one has to leave the shelf's columns, not merely leave the
        // shelf: the column it kept is the column the reflow behind it moves a
        // live exhibit into, and two windows in one place is the collision the
        // slots exist to avoid.
        let kept = runtime.surface(&newer).expect("it is open").slot;
        let closed_up = runtime.surface(&older).expect("it is open").slot;
        assert_eq!(kept, Slot::Edge(EDGE_SLOTS[0]));
        assert_eq!(closed_up, Slot::Shelf(0));
        assert_ne!(kept, closed_up);
        assert!(acts.contains(&Act::Move {
            surface: newer.clone(),
            slot: kept,
            size: Size {
                width: 250.0,
                height: 110.0
            },
        }));
        // And it comes down onto the workspace the person is on. Following them
        // everywhere is what it did while it was the runtime's.
        assert!(acts.contains(&Act::Stick {
            surface: newer.clone(),
            sticky: false
        }));
        assert!(runtime.surface(&newer).expect("it is open").owned);
    }

    #[test]
    fn a_pin_with_nowhere_to_put_the_panel_says_so_rather_than_doing_nothing() {
        let catalog = catalog();
        let mut runtime = Runtime::new();
        for _ in 0..EDGE_SLOTS.len() {
            open(&mut runtime, &catalog, "clock", Posture::Instrument);
        }
        let crowded = opened(&open(&mut runtime, &catalog, "note", Posture::Exhibit));
        let acts = runtime.apply(
            &catalog,
            Cmd::Pin {
                surface: crowded.clone(),
            },
        );
        assert!(matches!(
            acts.as_slice(),
            [Act::Refuse { surface, .. }] if surface == &crowded
        ));
        // Nothing moved and nothing changed hands. A tick that cannot be
        // carried out is a tick that was not carried out.
        assert!(!runtime.surface(&crowded).expect("it is open").owned);
        assert_eq!(
            runtime.surface(&crowded).map(|s| s.slot),
            Some(Slot::Shelf(0))
        );
    }

    #[test]
    fn an_exhibit_is_on_every_workspace_and_an_instrument_is_on_this_one() {
        let catalog = catalog();
        let mut runtime = Runtime::new();
        let acts = open(&mut runtime, &catalog, "note", Posture::Exhibit);
        let exhibit = opened(&acts);
        // The shelf is the layer that follows the person around, the way i3's
        // own scratchpad does. Nothing here touches i3's real scratchpad.
        assert!(acts.contains(&Act::Stick {
            surface: exhibit,
            sticky: true
        }));

        let acts = open(&mut runtime, &catalog, "clock", Posture::Instrument);
        let instrument = opened(&acts);
        assert!(acts.contains(&Act::Stick {
            surface: instrument,
            sticky: false
        }));
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

    /// Ends a turn, then lets `elapsed` pass in one sweep.
    fn wait(runtime: &mut Runtime, catalog: &Catalog, elapsed: Duration) -> Vec<Act> {
        let mut acts = runtime.apply(catalog, Cmd::TurnEnded);
        acts.extend(runtime.apply(catalog, Cmd::Sweep { elapsed }));
        acts
    }

    #[test]
    fn an_exhibit_the_turn_never_mentioned_dims_and_retires_when_its_grace_runs_out() {
        let catalog = catalog();
        let mut runtime = Runtime::new();
        let surface = opened(&open(&mut runtime, &catalog, "note", Posture::Exhibit));

        // The turn that opened it is the turn it belongs to, so it survives
        // that one bright. It is the turn after that says the subject changed.
        assert!(runtime.apply(&catalog, Cmd::TurnEnded).is_empty());
        assert_eq!(runtime.surface(&surface).map(|s| s.life), Some(Life::Live));

        let acts = runtime.apply(&catalog, Cmd::TurnEnded);
        assert_eq!(
            acts,
            vec![Act::Life {
                surface: surface.clone(),
                life: Life::Dim
            }]
        );

        assert!(
            runtime
                .apply(&catalog, Cmd::Sweep { elapsed: GRACE / 2 })
                .is_empty()
        );
        assert!(runtime.surface(&surface).is_some());
        let acts = runtime.apply(&catalog, Cmd::Sweep { elapsed: GRACE / 2 });
        // Silently: the daemon is told nothing, because an exhibit needs no
        // closing and a report for every one of them would be a transcript full
        // of panels going quiet.
        assert_eq!(
            acts,
            vec![Act::Retire {
                surface: surface.clone()
            }]
        );
        assert!(runtime.surface(&surface).is_none());
    }

    #[test]
    fn an_update_brings_a_dim_exhibit_back_without_moving_it() {
        let catalog = catalog();
        let mut runtime = Runtime::new();
        let older = opened(&open(&mut runtime, &catalog, "note", Posture::Exhibit));
        open(&mut runtime, &catalog, "note", Posture::Exhibit);
        wait(&mut runtime, &catalog, Duration::ZERO);
        wait(&mut runtime, &catalog, GRACE / 2);
        assert_eq!(runtime.surface(&older).map(|s| s.life), Some(Life::Dim));

        let acts = runtime.apply(
            &catalog,
            Cmd::Update {
                id: "w-9".into(),
                surface: older.clone(),
                data: json!({ "text": "still here" }),
            },
        );
        assert_eq!(
            acts.first(),
            Some(&Act::Life {
                surface: older.clone(),
                life: Life::Live
            }),
            "the chrome brightens before the data lands"
        );
        assert!(!acts.iter().any(|act| matches!(act, Act::Move { .. })));
        assert_eq!(
            runtime.surface(&older).map(|s| s.slot),
            Some(Slot::Shelf(1))
        );
        // The minute starts over, so the half of the grace it had already spent
        // does not take it away.
        wait(&mut runtime, &catalog, GRACE / 2);
        assert!(runtime.surface(&older).is_some());
    }

    #[test]
    fn a_panel_under_the_pointer_neither_dims_nor_ages() {
        let catalog = catalog();
        let mut runtime = Runtime::new();
        let read = opened(&open(&mut runtime, &catalog, "note", Posture::Exhibit));
        runtime.apply(
            &catalog,
            Cmd::Hover {
                surface: read.clone(),
                over: true,
            },
        );
        // Two turns and a full grace with the pointer on it: the person is
        // reading, and reading is not the conversation moving on.
        wait(&mut runtime, &catalog, GRACE);
        wait(&mut runtime, &catalog, GRACE);
        assert_eq!(runtime.surface(&read).map(|s| s.life), Some(Life::Live));

        runtime.apply(
            &catalog,
            Cmd::Hover {
                surface: read.clone(),
                over: false,
            },
        );
        wait(&mut runtime, &catalog, GRACE);
        assert!(runtime.surface(&read).is_none());
    }

    #[test]
    fn the_pointer_arriving_on_a_dim_panel_brings_it_back() {
        let catalog = catalog();
        let mut runtime = Runtime::new();
        let surface = opened(&open(&mut runtime, &catalog, "note", Posture::Exhibit));
        wait(&mut runtime, &catalog, Duration::ZERO);
        wait(&mut runtime, &catalog, Duration::ZERO);
        assert_eq!(runtime.surface(&surface).map(|s| s.life), Some(Life::Dim));

        let acts = runtime.apply(
            &catalog,
            Cmd::Hover {
                surface: surface.clone(),
                over: true,
            },
        );
        assert_eq!(
            acts,
            vec![Act::Life {
                surface,
                life: Life::Live
            }]
        );
    }

    #[test]
    fn a_frozen_clock_spends_no_grace() {
        let catalog = catalog();
        let mut runtime = Runtime::new();
        let surface = opened(&open(&mut runtime, &catalog, "note", Posture::Exhibit));
        wait(&mut runtime, &catalog, Duration::ZERO);
        runtime.apply(&catalog, Cmd::TurnEnded);

        for reason in [Still::Speech, Still::Microphone] {
            runtime.apply(
                &catalog,
                Cmd::Freeze {
                    reason,
                    stopped: true,
                },
            );
        }
        // Scufris talking, or a microphone that is open: the person is not
        // reading the screen, so this is not time the panel has been up.
        assert!(
            runtime
                .apply(&catalog, Cmd::Sweep { elapsed: GRACE * 4 })
                .is_empty()
        );
        assert!(runtime.surface(&surface).is_some());

        // One reason lifting is not every reason lifting. A microphone closing
        // while Scufris is still answering must not start a grace the answer
        // is still stopping.
        runtime.apply(
            &catalog,
            Cmd::Freeze {
                reason: Still::Microphone,
                stopped: false,
            },
        );
        assert!(
            runtime
                .apply(&catalog, Cmd::Sweep { elapsed: GRACE * 4 })
                .is_empty()
        );

        runtime.apply(
            &catalog,
            Cmd::Freeze {
                reason: Still::Speech,
                stopped: false,
            },
        );
        assert!(
            !runtime
                .apply(&catalog, Cmd::Sweep { elapsed: GRACE })
                .is_empty()
        );
    }

    #[test]
    fn the_layer_goes_down_with_the_pill_and_comes_back_the_way_it_was() {
        let catalog = catalog();
        let mut runtime = Runtime::new();
        let exhibit = opened(&open(&mut runtime, &catalog, "note", Posture::Exhibit));
        let kept = opened(&open(&mut runtime, &catalog, "note", Posture::Exhibit));
        let instrument = opened(&open(&mut runtime, &catalog, "clock", Posture::Instrument));
        runtime.apply(
            &catalog,
            Cmd::Pin {
                surface: kept.clone(),
            },
        );
        // Halfway through its grace when the pill goes down.
        wait(&mut runtime, &catalog, Duration::ZERO);
        wait(&mut runtime, &catalog, GRACE / 2);

        let acts = runtime.apply(&catalog, Cmd::Conceal { hidden: true });
        // Only what the runtime still owns. A pinned panel and an instrument
        // left this layer when the person took them.
        assert_eq!(
            acts.iter()
                .filter_map(|act| match act {
                    Act::Conceal {
                        surface,
                        hidden: true,
                        ..
                    } => Some(surface.clone()),
                    _ => None,
                })
                .collect::<Vec<_>>(),
            vec![exhibit.clone()]
        );
        assert!(runtime.surface(&kept).is_some());
        assert!(runtime.surface(&instrument).is_some());

        // Nothing ages behind the layer, however long it is down.
        assert!(
            runtime
                .apply(&catalog, Cmd::Sweep { elapsed: GRACE * 4 })
                .is_empty()
        );
        assert!(runtime.surface(&exhibit).is_some());

        let acts = runtime.apply(&catalog, Cmd::Conceal { hidden: false });
        assert_eq!(
            acts,
            vec![
                Act::Conceal {
                    surface: exhibit.clone(),
                    hidden: false,
                    slot: Slot::Shelf(0),
                    size: Size {
                        width: 250.0,
                        height: 110.0
                    },
                },
                // A window manager unmanages a window when it unmaps, so a
                // panel coming back has to be told again where it belongs.
                Act::Stick {
                    surface: exhibit.clone(),
                    sticky: true,
                },
            ]
        );
        // Exactly as it was: dim, and with the half minute it had left.
        assert_eq!(runtime.surface(&exhibit).map(|s| s.life), Some(Life::Dim));
        runtime.apply(&catalog, Cmd::Sweep { elapsed: GRACE / 2 });
        assert!(runtime.surface(&exhibit).is_none());
    }

    #[test]
    fn a_widget_opened_behind_the_layer_waits_there_instead_of_flashing_up() {
        let catalog = catalog();
        let mut runtime = Runtime::new();
        runtime.apply(&catalog, Cmd::Conceal { hidden: true });
        let acts = open(&mut runtime, &catalog, "note", Posture::Exhibit);
        assert!(matches!(
            acts.first(),
            Some(Act::Adopt { hidden: true, .. })
        ));
    }

    #[test]
    fn what_the_person_keeps_never_ages_out_from_under_them() {
        let catalog = catalog();
        let mut runtime = Runtime::new();
        let pinned = opened(&open(&mut runtime, &catalog, "note", Posture::Exhibit));
        let instrument = opened(&open(&mut runtime, &catalog, "clock", Posture::Instrument));
        runtime.apply(
            &catalog,
            Cmd::Pin {
                surface: pinned.clone(),
            },
        );

        for _ in 0..3 {
            wait(&mut runtime, &catalog, GRACE);
        }
        assert_eq!(runtime.surface(&pinned).map(|s| s.life), Some(Life::Pinned));
        // An instrument is a panel the person asked to keep. The shelf's clock
        // was never about it, and neither is the clear verb.
        assert_eq!(
            runtime.surface(&instrument).map(|s| s.life),
            Some(Life::Live)
        );
        runtime.apply(&catalog, Cmd::Clear { id: "w-9".into() });
        assert!(runtime.surface(&pinned).is_some());
        assert!(runtime.surface(&instrument).is_some());
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
