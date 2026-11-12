//! DCC companion library. The `dcc` binary is a thin command line over these modules.

pub mod agent;
pub mod config;
pub mod error;
pub mod git;
pub mod herdr;
pub mod jobs;
pub mod secret;
pub mod ssh;
pub mod store;
pub mod usage;
pub mod util;
pub mod workspace;

pub use error::{Error, Result};
