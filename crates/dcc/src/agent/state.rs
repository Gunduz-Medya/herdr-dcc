//! The common state model. Herdr reports working, blocked, idle and done (plus error in its docs).
//! DCC shows Running, Waiting, Completed, Failed, Stopped and Unknown. The first matching rule wins.
//!
//! | rule | facts                                              | state       |
//! | ---- | -------------------------------------------------- | ----------- |
//! | 1    | terminal finished, exit code 0                     | Completed   |
//! | 2    | finished, killed or signalled, or no exit code     | Stopped     |
//! | 3    | finished, other exit code                          | Failed      |
//! | 4    | status channel marked unreliable (live terminal)   | Unknown     |
//! | 5    | status error                                       | Failed      |
//! | 6    | working                                            | Running     |
//! | 7    | blocked                                            | Waiting     |
//! | 8    | idle                                               | Waiting     |
//! | 9    | done                                               | Completed   |
//! | 10   | no status, last hook was SessionEnd                | Completed   |
//! | 11   | no status, claude is the foreground program        | Unknown     |
//! | 12   | anything else                                      | Unknown     |
//!
//! The exit code and the reason only exist for terminals started by `hc run` (Phase 0 test 4).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DccState {
    Running,
    Waiting,
    Completed,
    Failed,
    Stopped,
    Unknown,
}

impl DccState {
    pub fn as_str(self) -> &'static str {
        match self {
            DccState::Running => "Running",
            DccState::Waiting => "Waiting",
            DccState::Completed => "Completed",
            DccState::Failed => "Failed",
            DccState::Stopped => "Stopped",
            DccState::Unknown => "Unknown",
        }
    }

    pub fn is_final(self) -> bool {
        matches!(
            self,
            DccState::Completed | DccState::Failed | DccState::Stopped
        )
    }
}

#[derive(Debug, Clone, Default)]
pub struct StateInput<'a> {
    /// The terminal still exists and runs.
    pub live: bool,
    /// Herdr's agent status: working, blocked, idle, done, error.
    pub status: Option<&'a str>,
    /// False when the OSC 7501 contract test failed for the installed Herdr version.
    pub status_unreliable: bool,
    pub exit_code: Option<i64>,
    pub reason: Option<&'a str>,
    pub signal: Option<i64>,
    pub claude_foreground: bool,
    pub last_hook: Option<&'a str>,
}

/// The state and a short machine readable code that says why (the panel turns the code into text).
pub fn map_state(i: &StateInput) -> (DccState, &'static str) {
    if !i.live {
        return match (i.reason, i.signal, i.exit_code) {
            (Some("killed" | "signal"), _, _) | (_, Some(_), _) => (DccState::Stopped, "killed"),
            (_, _, Some(0)) => (DccState::Completed, "exited_ok"),
            (_, _, None) => (DccState::Stopped, "no_exit_code"),
            _ => (DccState::Failed, "exit_nonzero"),
        };
    }
    if i.status_unreliable {
        return (DccState::Unknown, "status_unreliable");
    }
    match i.status {
        Some("error") => (DccState::Failed, "agent_error"),
        Some("working") => (DccState::Running, "working"),
        Some("blocked") => (DccState::Waiting, "needs_permission"),
        Some("idle") => (DccState::Waiting, "ready_for_input"),
        Some("done") => (DccState::Completed, "turn_done"),
        _ if i.last_hook == Some("SessionEnd") => (DccState::Completed, "session_ended"),
        _ if i.claude_foreground => (DccState::Unknown, "no_status_claude"),
        _ => (DccState::Unknown, "no_status"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn live(status: Option<&str>) -> StateInput<'_> {
        StateInput {
            live: true,
            status,
            ..Default::default()
        }
    }

    #[test]
    fn live_statuses() {
        for (s, want) in [
            ("working", DccState::Running),
            ("blocked", DccState::Waiting),
            ("idle", DccState::Waiting),
            ("done", DccState::Completed),
            ("error", DccState::Failed),
        ] {
            assert_eq!(map_state(&live(Some(s))).0, want, "{s}");
        }
        assert_eq!(map_state(&live(Some("blocked"))).1, "needs_permission");
        assert_eq!(map_state(&live(Some("idle"))).1, "ready_for_input");
    }

    #[test]
    fn no_status_cases() {
        assert_eq!(map_state(&live(None)), (DccState::Unknown, "no_status"));
        let claude = StateInput {
            claude_foreground: true,
            ..live(None)
        };
        assert_eq!(map_state(&claude), (DccState::Unknown, "no_status_claude"));
        let ended = StateInput {
            last_hook: Some("SessionEnd"),
            claude_foreground: true,
            ..live(None)
        };
        assert_eq!(map_state(&ended).0, DccState::Completed);
    }

    #[test]
    fn finished_terminals() {
        let fin = |code, reason, signal| StateInput {
            live: false,
            exit_code: code,
            reason,
            signal,
            ..Default::default()
        };
        assert_eq!(
            map_state(&fin(Some(0), Some("exited"), None)).0,
            DccState::Completed
        );
        assert_eq!(
            map_state(&fin(Some(3), Some("exited"), None)).0,
            DccState::Failed
        );
        assert_eq!(
            map_state(&fin(None, Some("killed"), Some(1))).0,
            DccState::Stopped
        );
        assert_eq!(
            map_state(&fin(Some(1), Some("killed"), None)).0,
            DccState::Stopped
        );
        assert_eq!(map_state(&fin(None, None, None)).0, DccState::Stopped);
    }

    #[test]
    fn an_unreliable_channel_makes_live_terminals_unknown_but_not_finished_ones() {
        let bad = StateInput {
            status_unreliable: true,
            ..live(Some("working"))
        };
        assert_eq!(map_state(&bad), (DccState::Unknown, "status_unreliable"));
        let finished = StateInput {
            live: false,
            status_unreliable: true,
            exit_code: Some(0),
            reason: Some("exited"),
            ..Default::default()
        };
        assert_eq!(map_state(&finished).0, DccState::Completed);
    }

    #[test]
    fn finality() {
        assert!(
            DccState::Failed.is_final()
                && DccState::Stopped.is_final()
                && DccState::Completed.is_final()
        );
        assert!(
            !DccState::Running.is_final()
                && !DccState::Waiting.is_final()
                && !DccState::Unknown.is_final()
        );
    }
}
