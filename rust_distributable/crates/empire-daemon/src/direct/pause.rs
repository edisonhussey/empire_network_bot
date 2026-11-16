//! Manually pausing an injection session's bots without dropping its socket.
//!
//! Shared between the HTTP handler (the Pause button) and the session loop, like [`super::PaceControl`].
//! While paused nothing is sent by the bots, but every frame is still read: navigation, stock, commanders
//! and marches keep updating, so resuming continues from the live state. A step in flight when the pause
//! starts is abandoned as a failure (`interrupt` on each bot); nothing resumes half way. Held in memory only.

use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

#[derive(Debug, Clone, Default)]
pub struct PauseControl {
    manual: Arc<AtomicBool>,
}

impl PauseControl {
    pub fn set_manual(&self, paused: bool) {
        self.manual.store(paused, Ordering::Relaxed);
    }

    pub fn manual(&self) -> bool {
        self.manual.load(Ordering::Relaxed)
    }

    /// Why the bots may not send now, if they may not.
    pub fn reason(&self, _now_ms: i64) -> Option<String> {
        self.manual().then(|| "Paused: listening only. Press Resume to continue.".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn manual_pause_is_shared_and_lasts_until_resumed() {
        let pause = PauseControl::default();
        let shared = pause.clone();
        assert!(!pause.manual());
        assert!(pause.reason(0).is_none());
        pause.set_manual(true);
        assert!(shared.manual());
        for now in [0, 60_000, i64::MAX] {
            assert_eq!(shared.reason(now).as_deref(), Some("Paused: listening only. Press Resume to continue."));
        }
        shared.set_manual(false);
        assert!(!pause.manual());
        assert!(pause.reason(60_000).is_none());
    }
}
