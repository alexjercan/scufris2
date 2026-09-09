//! Every stable refusal code, in one place.
//!
//! A refusal crosses the socket as a bare string in `code`. Nothing checks it:
//! a sender that writes `attachments_unavailable` and a receiver that matches
//! `attachment_unavailable` both compile, and what fails is the behaviour that
//! depended on the match - at run time, on the one path nobody exercises by
//! hand. Naming them here is what makes a typo a compile error on both sides
//! of a match.
//!
//! It is also the list. A client author writing a surface has no other way to
//! learn what can come back, and a code written as a literal at its send site
//! is invisible to them.
//!
//! The agent's half is `agent/extensions/scufris/service/protocol.ts`, whose
//! `REFUSAL` names the same strings; `tests/service.test.ts` reads this file
//! and holds the two together.

/// A second `hello` on a connection that already said it.
pub const DUPLICATE_HELLO: &str = "duplicate_hello";

/// Another agent already holds the agent channel.
pub const AGENT_EXISTS: &str = "agent_exists";

/// Nothing is on the agent channel, so there is nobody to carry this to.
pub const AGENT_UNAVAILABLE: &str = "agent_unavailable";

/// The attachment store is not running, or would not answer.
pub const ATTACHMENTS_UNAVAILABLE: &str = "attachments_unavailable";

/// The attachment is past what the store will hold.
pub const ATTACHMENT_TOO_LARGE: &str = "attachment_too_large";

/// The attachment is not a readable regular file with a usable name.
pub const INVALID_ATTACHMENT: &str = "invalid_attachment";

/// The upload stopped before the attachment was whole.
pub const ATTACHMENT_INCOMPLETE: &str = "attachment_incomplete";

/// No attachment is stored under that identifier.
pub const ATTACHMENT_NOT_FOUND: &str = "attachment_not_found";

/// Attachment storage is full.
pub const ATTACHMENT_QUOTA: &str = "attachment_quota";

/// Attachment storage would not answer.
pub const ATTACHMENT_UNAVAILABLE: &str = "attachment_unavailable";

/// The byte range asked for is not a range this content has.
pub const INVALID_RANGE: &str = "invalid_range";

/// The widget calls in a response are not calls this can carry.
pub const INVALID_WIDGETS: &str = "invalid_widgets";

/// No widget by that name is installed.
pub const WIDGET_NOT_FOUND: &str = "widget_not_found";

/// The surface named is not open.
pub const SURFACE_NOT_FOUND: &str = "surface_not_found";

/// Every instrument slot is taken.
pub const NO_FREE_SLOT: &str = "no_free_slot";

/// The window opened for a widget never reached the screen.
pub const NOT_SHOWN: &str = "not_shown";

/// The offer pressed has been taken already, or is no longer in the replay.
pub const OFFER_UNAVAILABLE: &str = "offer_unavailable";
