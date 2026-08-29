//! Widget runtime decisions. These types do not cross the service protocol.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Posture {
    Exhibit,
    Instrument,
}

#[derive(Debug, Clone, PartialEq)]
pub enum WidgetReport {
    Opened {
        id: String,
        surface: String,
    },
    Done {
        id: String,
    },
    Failed {
        id: String,
        code: String,
        detail: String,
    },
    Closed {
        surface: String,
    },
}
