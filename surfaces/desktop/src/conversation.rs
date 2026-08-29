//! What the HUD shows, and what typing into it does.
//!
//! Every decision the HUD makes lives here, with no window and no socket in
//! sight, the way [`crate::state::Companion`] holds the pill's. [`crate::hud`]
//! runs what this returns.
//!
//! This is a sibling of the pill's state machine rather than a phase of it.
//! The pill machine is about one take - the microphone opens, the words are
//! read, they go or they do not - and it ends. The conversation outlives every
//! take, and a line typed here must not raise the textbox, end a recording, or
//! move the pill off whatever it is showing.
//!
//! Two things follow from that, and they are the whole of the design:
//!
//! - **Two senders, one verb.** The textbox sends a transcription and the HUD
//!   sends a typed line; both are a `submit` on the same socket.
//! - **No durable copy.** The pill persists an accepted transcript because
//!   spoken words cannot be typed again. A line typed here is still in the
//!   field until the service takes it, so there is nothing to rescue.

use std::collections::VecDeque;

use scufris_control::service::ConversationMessage;
use serde::Serialize;

/// How many lines the companion keeps.
///
/// The same as the service's own ring, which is what it replays on connect:
/// keeping more here would be keeping lines nothing can refill after a
/// reconnection, and keeping fewer would throw away lines it just sent.
pub const LINES: usize = 200;

/// What the page is told after one thing happens to it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Notice {
    /// True while a typed line is waiting for the service to take it.
    pub sending: bool,
    /// What went wrong, empty when nothing did.
    pub trouble: String,
}

/// What the HUD is showing and what it is waiting for.
#[derive(Debug, Clone, PartialEq)]
pub struct Conversation {
    lines: VecDeque<ConversationMessage>,
    /// The identifier of the line the service has not answered for yet.
    sending: Option<String>,
    trouble: String,
    prefix: String,
    submissions: u64,
}

impl Conversation {
    /// Builds one, with the identifier prefix this process owns.
    ///
    /// The prefix is the pill's, and the counter is not: an identifier is how
    /// an answer is matched to the line that asked, and two senders sharing a
    /// prefix and a counter would eventually name two different lines the same
    /// thing and take each other's answers. The `h` is what keeps them
    /// apart.
    pub fn new(prefix: impl Into<String>) -> Self {
        Self {
            lines: VecDeque::new(),
            sending: None,
            trouble: String::new(),
            prefix: prefix.into(),
            submissions: 0,
        }
    }

    /// Puts one line of the conversation on the end.
    ///
    /// Everything said reaches here, whoever said it and whatever surface sent
    /// it: the service pushes its ring on connect and every line after it, so
    /// a line sent by another registered surface appears in the HUD the same
    /// way one typed into this HUD does.
    pub fn said(&mut self, entry: ConversationMessage) {
        if self.lines.len() == LINES {
            self.lines.pop_front();
        }
        self.lines.push_back(entry);
    }

    /// Everything said so far, oldest first.
    pub fn lines(&self) -> Vec<ConversationMessage> {
        self.lines.iter().cloned().collect()
    }

    /// Forgets every line, because the ones about to arrive are all of them.
    ///
    /// The service replays its whole ring to a frontend that connects, so a
    /// reconnection delivers everything this already holds a second time. What
    /// is kept here is a copy of that ring and never a longer history, which is
    /// what makes throwing it away the right answer rather than a loss: after
    /// the replay the two hold exactly the same lines.
    pub fn restart(&mut self) {
        self.lines.clear();
        self.trouble = "Loading conversation.".into();
    }

    /// Replay is complete and live presentation may begin.
    pub fn ready(&mut self) {
        if self.sending.is_none() {
            self.trouble.clear();
        }
    }

    /// The connection went away, so nothing in flight is coming back.
    ///
    /// Answers whether there was anything to give up on. Without this a line
    /// submitted as the service went down would wait for an answer that no
    /// reconnection brings, and the field would refuse every line after it for
    /// the rest of the session.
    pub fn dropped(&mut self, trouble: impl Into<String>) -> bool {
        if self.sending.take().is_none() {
            return false;
        }
        self.trouble = trouble.into();
        true
    }

    /// Takes one typed line, and answers with the identifier to send it under.
    ///
    /// Nothing is refused for being long: the socket measures the same bytes
    /// and its refusal is the one worth reporting, because it is the one that
    /// is true. Blank is refused, because a blank line is a stray Enter rather
    /// than a question.
    pub fn typed(&mut self, text: &str) -> Option<String> {
        if text.trim().is_empty() {
            return None;
        }
        // One line at a time. A second Enter while the first is unanswered
        // would put two questions in the conversation from one intention, and
        // the field still holds the words either way.
        if self.sending.is_some() {
            return None;
        }
        self.submissions += 1;
        let id = format!("{}-h{}", self.prefix, self.submissions);
        self.sending = Some(id.clone());
        self.trouble.clear();
        Some(id)
    }

    /// The service took the line under this identifier.
    ///
    /// Nothing is appended here. The line comes back as a transcript entry,
    /// which is what puts it on screen - so what the HUD shows is what the
    /// conversation holds rather than what this process hoped it sent.
    pub fn accepted(&mut self, id: &str) -> bool {
        if self.sending.as_deref() != Some(id) {
            return false;
        }
        self.sending = None;
        self.trouble.clear();
        true
    }

    /// The service would not take the line, or it never left this process.
    pub fn refused(&mut self, id: &str, trouble: impl Into<String>) -> bool {
        if self.sending.as_deref() != Some(id) {
            return false;
        }
        self.sending = None;
        self.trouble = trouble.into();
        true
    }

    /// What the page is told about what it is waiting for.
    pub fn notice(&self) -> Notice {
        Notice {
            sending: self.sending.is_some(),
            trouble: self.trouble.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use scufris_control::service::ConversationRole;

    use super::*;

    fn said(text: &str) -> ConversationMessage {
        ConversationMessage {
            role: ConversationRole::Assistant,
            surface: "desk".into(),
            text: text.into(),
            details: None,
            widgets: None,
        }
    }

    #[test]
    fn every_line_of_the_conversation_is_kept_in_the_order_it_was_said() {
        let mut conversation = Conversation::new("p");
        conversation.said(ConversationMessage {
            role: ConversationRole::User,
            surface: "desk".into(),
            text: "what time is it".into(),
            details: None,
            widgets: None,
        });
        conversation.said(said("half past four"));
        let lines = conversation.lines();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0].role, ConversationRole::User);
        assert_eq!(lines[1].text, "half past four");
    }

    #[test]
    fn the_oldest_line_goes_when_the_ring_is_full() {
        // The same bound the service keeps, so a reconnection refills exactly
        // what is here rather than more or less of it.
        let mut conversation = Conversation::new("p");
        for index in 0..LINES + 5 {
            conversation.said(said(&format!("line {index}")));
        }
        let lines = conversation.lines();
        assert_eq!(lines.len(), LINES);
        assert_eq!(lines[0].text, "line 5");
        assert_eq!(lines[LINES - 1].text, format!("line {}", LINES + 4));
    }

    #[test]
    fn a_typed_line_is_sent_under_an_identifier_of_its_own() {
        let mut conversation = Conversation::new("abc");
        let id = conversation.typed("hello").expect("the line is sent");
        assert_eq!(id, "abc-h1");
        assert!(conversation.notice().sending);
    }

    #[test]
    fn the_hud_never_names_a_line_what_the_pill_named_one() {
        // Both senders share the process prefix, because that is what makes an
        // identifier unique to this companion. Only the counter keeps them
        // apart, so the shape has to differ and not just the number.
        let mut conversation = Conversation::new("abc");
        let first = conversation.typed("one").expect("the line is sent");
        conversation.accepted(&first);
        let second = conversation.typed("two").expect("the line is sent");
        assert_eq!((first.as_str(), second.as_str()), ("abc-h1", "abc-h2"));
        assert!(!first.starts_with("abc-1"), "a pill identifier shape");
    }

    #[test]
    fn a_blank_line_is_a_stray_enter_rather_than_a_question() {
        let mut conversation = Conversation::new("p");
        assert_eq!(conversation.typed("   \n "), None);
        assert!(!conversation.notice().sending);
    }

    #[test]
    fn one_line_is_in_flight_at_a_time() {
        // The words are still in the field, so a second Enter costs nothing to
        // refuse and would otherwise ask the same question twice.
        let mut conversation = Conversation::new("p");
        assert!(conversation.typed("first").is_some());
        assert_eq!(conversation.typed("second"), None);
    }

    #[test]
    fn a_line_the_service_took_stops_being_waited_for() {
        let mut conversation = Conversation::new("p");
        let id = conversation.typed("hello").expect("the line is sent");
        assert!(conversation.accepted(&id));
        assert_eq!(
            conversation.notice(),
            Notice {
                sending: false,
                trouble: String::new(),
            }
        );
        // And the field is free for the next one.
        assert!(conversation.typed("again").is_some());
    }

    #[test]
    fn a_line_the_service_would_not_take_says_why() {
        let mut conversation = Conversation::new("p");
        let id = conversation.typed("hello").expect("the line is sent");
        assert!(conversation.refused(&id, "Scufris is not reachable."));
        assert_eq!(
            conversation.notice(),
            Notice {
                sending: false,
                trouble: "Scufris is not reachable.".into(),
            }
        );
    }

    #[test]
    fn an_answer_for_somebody_elses_line_changes_nothing() {
        // The pill submits on the same connection and its answers arrive here
        // too. One that is not this window's must not clear this window's.
        let mut conversation = Conversation::new("p");
        let id = conversation.typed("hello").expect("the line is sent");
        assert!(!conversation.accepted("p-4"));
        assert!(!conversation.refused("p-4", "nothing"));
        assert!(conversation.notice().sending);
        assert!(conversation.accepted(&id));
    }

    #[test]
    fn asking_again_clears_what_the_last_refusal_said() {
        let mut conversation = Conversation::new("p");
        let first = conversation.typed("hello").expect("the line is sent");
        conversation.refused(&first, "Scufris is not reachable.");
        conversation.typed("hello").expect("the line is sent");
        assert_eq!(conversation.notice().trouble, "");
    }

    #[test]
    fn a_reconnection_starts_from_the_replay_rather_than_doubling_it() {
        // The service pushes its whole ring to a frontend that connects. Kept
        // lines plus a replay is every line twice.
        let mut conversation = Conversation::new("p");
        conversation.said(said("half past four"));
        conversation.restart();
        assert!(conversation.lines().is_empty());
        assert_eq!(conversation.notice().trouble, "Loading conversation.");
        conversation.said(said("half past four"));
        conversation.ready();
        assert_eq!(conversation.notice().trouble, "");
        assert_eq!(conversation.lines().len(), 1);
    }

    #[test]
    fn a_line_in_flight_when_the_service_goes_away_is_given_up_on() {
        // No reconnection brings this answer back. A window that kept waiting
        // would refuse every line after it for the rest of the session.
        let mut conversation = Conversation::new("p");
        conversation.typed("hello").expect("the line is sent");
        assert!(conversation.dropped("Scufris is not reachable."));
        assert_eq!(
            conversation.notice(),
            Notice {
                sending: false,
                trouble: "Scufris is not reachable.".into(),
            }
        );
        assert!(conversation.typed("again").is_some());
    }

    #[test]
    fn a_disconnection_with_nothing_in_flight_says_nothing() {
        // The link reports one disconnection per failed reconnection attempt,
        // and a window that is not waiting for anything has no news in that.
        let mut conversation = Conversation::new("p");
        assert!(!conversation.dropped("Scufris is not reachable."));
        assert_eq!(conversation.notice().trouble, "");
    }
}
