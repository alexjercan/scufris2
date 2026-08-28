//! One widget decision at a time, and the way past it for the thread that must
//! not wait for one.
//!
//! Widget work is serialised. Two threads interleaving a shelf reflow with a
//! widget opening would put two windows in one column, so whoever is deciding
//! holds the turn until everything that decision decided has been carried out.
//! A lock is the obvious way to say that, and for every thread but one it is
//! all this is.
//!
//! The exception is the event loop. A thread holding the turn is very likely
//! waiting for that loop: showing a window, sizing it and moving it are all
//! requests the loop carries out, and asking a window which monitor it is on is
//! a question only the loop can answer - and the toolkit answers such a question
//! by waiting for the loop with no timeout at all. An event loop that waited
//! here would be waiting for a thread that is waiting for it, and the whole
//! companion would stop: every window it owns stops repainting, which on a
//! screen looks like one gray rectangle that smears over everything it is
//! dragged across.
//!
//! So the loop never waits. It takes the turn if the turn is free, and hands
//! the decision to a thread that is allowed to wait if it is not.

use std::{
    sync::{
        Arc, Mutex, MutexGuard, TryLockError,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Receiver, Sender},
    },
    thread,
};

/// The turn, and the queue for whoever cannot wait for it.
pub struct Turn<T> {
    one: Mutex<()>,
    later: Sender<T>,
    /// How many decisions are in the queue or being made out of it.
    ///
    /// So that a decision handed over is not overtaken by one asked for after
    /// it. Without this the loop could hand over a reading, find the turn free
    /// a moment later, and put the newer reading of the same panel on screen
    /// first - leaving the older one showing until something replaced it.
    waiting: Arc<AtomicUsize>,
    /// The other end, until a thread is given it. See [`Turn::staff`].
    queue: Mutex<Option<Receiver<T>>>,
}

impl<T: Send + 'static> Turn<T> {
    pub fn new() -> Self {
        let (later, queue) = mpsc::channel();
        Self {
            one: Mutex::new(()),
            later,
            waiting: Arc::new(AtomicUsize::new(0)),
            queue: Mutex::new(Some(queue)),
        }
    }

    /// Gives the queue a thread of its own.
    ///
    /// Separate from [`Turn::new`] because what makes a handed-over decision is
    /// whatever holds this turn, and that does not exist until this does.
    /// Anything handed over before this is waiting in the queue rather than
    /// lost. Said once; a second time is ignored.
    pub fn staff(&self, make: impl Fn(T) + Send + 'static) {
        let Some(queue) = self
            .queue
            .lock()
            .unwrap_or_else(|error| error.into_inner())
            .take()
        else {
            return;
        };
        let waiting = Arc::clone(&self.waiting);
        thread::spawn(move || {
            for one in queue {
                make(one);
                // Cleared after the decision is made rather than after it is
                // taken. One still being made is one the loop must queue behind.
                waiting.fetch_sub(1, Ordering::SeqCst);
            }
        });
    }

    /// The turn, waited for. For a thread that is allowed to wait.
    pub fn wait(&self) -> MutexGuard<'_, ()> {
        self.one.lock().unwrap_or_else(|error| error.into_inner())
    }

    /// The turn if it is free and nothing is already waiting for it.
    ///
    /// For the event loop, which has no third answer. See the module
    /// documentation for why it is not allowed the other one. Nothing waiting
    /// is part of the answer because a caller that gets the turn here is
    /// deciding ahead of everything in the queue, and the queue holds what that
    /// same caller could not decide a moment ago.
    pub fn free(&self) -> Option<MutexGuard<'_, ()>> {
        if self.waiting.load(Ordering::SeqCst) > 0 {
            return None;
        }
        match self.one.try_lock() {
            Ok(held) => Some(held),
            // A panic under the turn left the runtime as it was found, and the
            // widget that panicked is the one that pays for it. Every other
            // lock here is taken the same way.
            Err(TryLockError::Poisoned(error)) => Some(error.into_inner()),
            Err(TryLockError::WouldBlock) => None,
        }
    }

    /// Hands one decision to the thread that is allowed to wait for its turn.
    ///
    /// Answers whether anything took it, which is false only once that thread
    /// has gone.
    pub fn later(&self, one: T) -> bool {
        // Counted before it is sent, so that nothing can be found waiting for a
        // turn it is not yet in the queue for.
        self.waiting.fetch_add(1, Ordering::SeqCst);
        if self.later.send(one).is_err() {
            self.waiting.fetch_sub(1, Ordering::SeqCst);
            return false;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use std::{
        sync::{Arc, mpsc::RecvTimeoutError},
        time::Duration,
    };

    use super::*;

    /// Long enough that a thread which was going to answer has, and short
    /// enough that a test which hangs fails instead.
    const PATIENCE: Duration = Duration::from_secs(2);

    fn made() -> (Arc<Turn<u32>>, Receiver<u32>) {
        let turn = Arc::new(Turn::new());
        let (done, made) = mpsc::channel();
        let working = Arc::clone(&turn);
        turn.staff(move |one| {
            // The waiting the event loop was spared. A thread that takes it is
            // the whole point of there being another thread.
            let _turn = working.wait();
            let _ = done.send(one);
        });
        (turn, made)
    }

    #[test]
    fn the_turn_is_not_free_while_somebody_holds_it() {
        let (turn, _made) = made();
        let held = turn.wait();
        // The answer, rather than a wait for one. This is the call the event
        // loop makes, and the loop is what the holder is waiting for.
        assert!(turn.free().is_none());
        drop(held);
        assert!(turn.free().is_some());
    }

    #[test]
    fn a_decision_handed_over_while_the_turn_is_held_is_made_once_it_is_free() {
        let (turn, made) = made();
        let held = turn.wait();
        assert!(turn.later(7));
        // Nothing yet: the thread that took it is waiting for the turn, which
        // is the wait the caller refused to do.
        assert_eq!(
            made.recv_timeout(Duration::from_millis(100)),
            Err(RecvTimeoutError::Timeout)
        );
        drop(held);
        assert_eq!(made.recv_timeout(PATIENCE), Ok(7));
    }

    #[test]
    fn a_decision_handed_over_before_there_was_a_thread_is_still_made() {
        // The queue outlives the gap between the turn existing and the runtime
        // that works it existing, because a welcome can arrive inside it.
        let turn = Arc::new(Turn::new());
        assert!(turn.later(3));
        let (done, made) = mpsc::channel();
        turn.staff(move |one| {
            let _ = done.send(one);
        });
        assert_eq!(made.recv_timeout(PATIENCE), Ok(3));
    }

    #[test]
    fn the_queue_keeps_the_order_it_was_given() {
        let (turn, made) = made();
        let held = turn.wait();
        for one in 1..=3 {
            assert!(turn.later(one));
        }
        drop(held);
        for one in 1..=3 {
            assert_eq!(made.recv_timeout(PATIENCE), Ok(one));
        }
    }

    #[test]
    fn a_decision_in_the_queue_keeps_the_turn_from_being_taken_around_it() {
        // Nothing holds the mutex here. What refuses the turn is the queue, and
        // it has to: the caller asking is the one that filled it, and a caller
        // that decided around its own queue would put an older reading of a
        // panel on screen after a newer one.
        let turn = Arc::new(Turn::<u32>::new());
        let (start, go) = mpsc::channel();
        let (done, made) = mpsc::channel();
        turn.staff(move |one| {
            let _ = go.recv();
            let _ = done.send(one);
        });
        assert!(turn.free().is_some());
        assert!(turn.later(1));
        assert!(turn.free().is_none());
        let _ = start.send(());
        assert_eq!(made.recv_timeout(PATIENCE), Ok(1));
        assert!(freed(&turn), "the turn came back once the queue was empty");
    }

    /// Waits for the turn to be takeable again, and answers whether it was.
    ///
    /// The count clears after the decision is made rather than as it is handed
    /// over, so the last of it lands just behind the answer the test read.
    fn freed(turn: &Turn<u32>) -> bool {
        let deadline = std::time::Instant::now() + PATIENCE;
        while std::time::Instant::now() < deadline {
            if turn.free().is_some() {
                return true;
            }
            thread::sleep(Duration::from_millis(5));
        }
        false
    }

    #[test]
    fn a_poisoned_turn_is_still_a_turn() {
        // The alternative is a companion that stops deciding anything about
        // widgets for as long as it runs, over one panic in one of them.
        let turn = Arc::new(Turn::<u32>::new());
        let poisoning = Arc::clone(&turn);
        let _ = thread::spawn(move || {
            let _held = poisoning.wait();
            panic!("a decision that did not finish");
        })
        .join();
        assert!(turn.free().is_some());
    }
}
