//! Pausing a session's bots without dropping its socket.
//!
//! Shared between the HTTP handler (the Pause button), the proxy link (the human acting in the browser) and the
//! session loop, like [`super::PaceControl`]. While paused nothing is sent by the bots, but every frame is still
//! read: navigation, stock, commanders and marches keep updating, so resuming continues from the live state. A
//! step in flight when the pause starts is abandoned as a failure (`interrupt` on each bot); nothing resumes half
//! way. Held in memory only.

use std::sync::{
    Arc, Mutex,
    atomic::{AtomicBool, AtomicI64, Ordering},
};

/// How long the bots stay off the connection after the human changed the view under them.
pub const HUMAN_HOLD_MS: i64 = 60_000;

/// Client commands that move the view or act, so a bot mid-sequence can no longer trust what it last saw.
pub const VIEW_COMMANDS: [&str; 7] = ["jca", "gaa", "cra", "adi", "bup", "crst", "kut"];

#[derive(Debug, Clone, Default)]
pub struct PauseControl {
    manual: Arc<AtomicBool>,
    hold_until_ms: Arc<AtomicI64>,
    /// The latest human view command not yet looked at by the session loop.
    human: Arc<Mutex<Option<String>>>,
}

impl PauseControl {
    pub fn set_manual(&self, paused: bool) {
        self.manual.store(paused, Ordering::Relaxed);
    }

    pub fn manual(&self) -> bool {
        self.manual.load(Ordering::Relaxed)
    }

    pub fn hold_until(&self, until_ms: i64) {
        self.hold_until_ms.fetch_max(until_ms, Ordering::Relaxed);
    }

    pub fn held_until_ms(&self) -> i64 {
        self.hold_until_ms.load(Ordering::Relaxed)
    }

    /// Why the bots may not send now, if they may not.
    pub fn reason(&self, now_ms: i64) -> Option<String> {
        if self.manual() {
            return Some("Paused: listening only. Press Resume to continue.".to_owned());
        }
        let until = self.held_until_ms();
        (until > now_ms).then(|| format!("You changed the view in the game; bots wait {} s, still listening", (until - now_ms + 999) / 1_000))
    }

    /// The proxy saw the human send `command`.
    pub fn note_human(&self, command: &str) {
        if VIEW_COMMANDS.contains(&command) {
            *self.human.lock().expect("pause lock") = Some(command.to_owned());
        }
    }

    pub fn take_human(&self) -> Option<String> {
        self.human.lock().expect("pause lock").take()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_pause_and_a_human_hold_both_stop_the_bots_until_they_end() {
        let pause = PauseControl::default();
        assert!(pause.reason(0).is_none());
        pause.set_manual(true);
        assert!(pause.clone().reason(0).unwrap().starts_with("Paused"));
        pause.set_manual(false);
        pause.hold_until(HUMAN_HOLD_MS);
        assert!(pause.reason(HUMAN_HOLD_MS - 1).is_some());
        assert!(pause.reason(HUMAN_HOLD_MS).is_none());
    }

    #[test]
    fn only_view_commands_count_as_the_human_acting() {
        let pause = PauseControl::default();
        pause.note_human("pin");
        assert!(pause.take_human().is_none());
        pause.note_human("jca");
        assert_eq!(pause.take_human().as_deref(), Some("jca"));
        assert!(pause.take_human().is_none());
    }
}
