//! Isolation for development and tests. `HERDR_APP_HOME` picks the window, `HC_SESSION` the core,
//! and the XDG folders decide where the core keeps its sockets. With `DCC_REQUIRE_ISOLATION=1`
//! (set by the dev environment) dcc and the test tools refuse to talk to Herdr without them, so a
//! forgotten variable cannot touch a real installation.

use std::path::{Path, PathBuf};

use crate::error::{Error, Result};

/// The system allows 103 bytes for a socket path (the core reported it in the spike).
pub const MAX_SOCKET_BYTES: usize = 103;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Isolation {
    pub app_home: Option<PathBuf>,
    pub session: Option<String>,
}

pub fn current() -> Isolation {
    Isolation {
        app_home: std::env::var_os("HERDR_APP_HOME")
            .map(PathBuf::from)
            .filter(|p| !p.as_os_str().is_empty()),
        session: std::env::var("HC_SESSION").ok().filter(|s| !s.is_empty()),
    }
}

pub fn required() -> bool {
    std::env::var("DCC_REQUIRE_ISOLATION")
        .map(|v| v == "1")
        .unwrap_or(false)
}

pub fn require() -> Result<()> {
    if !required() {
        return Ok(());
    }
    let iso = current();
    let mut missing = Vec::new();
    if iso.app_home.is_none() {
        missing.push("HERDR_APP_HOME");
    }
    if iso.session.is_none() {
        missing.push("HC_SESSION");
    }
    if missing.is_empty() {
        Ok(())
    } else {
        Err(Error::isolation(format!(
            "isolation is required but {} is not set. Load the dev environment in the same command line.",
            missing.join(" and ")
        )))
    }
}

/// Length of the longest socket path the core makes for a session, the holder socket
/// `<state>/herdr-core/<session>/<session>.holder.<8 hex>.sock`. In the spike this formula gave
/// 118 bytes for a case the core itself reported as 118. It is an estimate for other layouts.
pub fn holder_socket_len(state_home: &Path, session: &str) -> usize {
    state_home.as_os_str().len()
        + "/herdr-core/".len()
        + session.len()
        + 1
        + session.len()
        + ".holder.".len()
        + 8
        + ".sock".len()
}

pub fn check_socket_len(state_home: &Path, session: &str) -> Result<usize> {
    let len = holder_socket_len(state_home, session);
    if len > MAX_SOCKET_BYTES {
        Err(Error::isolation(format!(
            "the core socket path would be {len} bytes, the limit is {MAX_SOCKET_BYTES}. Use a shorter session name or state folder."
        )))
    } else {
        Ok(len)
    }
}

/// The state folder the core will use: `XDG_STATE_HOME`, else `$HOME/.local/state` (a guess).
pub fn state_home() -> Option<PathBuf> {
    std::env::var_os("XDG_STATE_HOME")
        .map(PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".local").join("state")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formula_matches_the_value_the_core_reported() {
        let state = Path::new("/Users/barisgunduz/dev/dcc/.dev/scratch-xdg-remote/state");
        assert_eq!(holder_socket_len(state, "dcc-dev-remote"), 118);
        assert!(check_socket_len(state, "dcc-dev-remote").is_err());
    }

    #[test]
    fn short_names_pass() {
        let state = Path::new("/Users/barisgunduz/dev/dcc/.dev/rx/s");
        assert!(check_socket_len(state, "dcc-rx").unwrap() <= MAX_SOCKET_BYTES);
    }
}
