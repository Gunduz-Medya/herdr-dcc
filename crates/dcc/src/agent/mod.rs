//! Agent support. A run is one agent process in one terminal. A run can hold several sessions,
//! because `/clear` starts a new session in the same process (Phase 0 test 1).
//!
//! - `state`    the common state model and the rules that map Herdr facts to it
//! - `db`       runs, sessions and events in SQLite
//! - `hooks`    what a Claude Code hook is allowed to leave behind (ids and names only)
//! - `statusline` key names and types of the status line JSON, never values
//! - `adapter`  one trait per agent kind, Claude Code first
//! - `exec`, `launch`  starting an agent the DCC way
//! - `record`   the snapshot: read the core, update the records, return the rows for the panel
//! - `contract` the OSC 7501 contract test

pub mod adapter;
pub mod contract;
pub mod db;
pub mod exec;
pub mod hooks;
pub mod launch;
pub mod record;
pub mod state;
pub mod statusline;
