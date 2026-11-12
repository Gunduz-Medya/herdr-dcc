//! DCC companion library. The `dcc` binary is a thin command line over these modules.

pub mod error;
pub mod herdr;
pub mod jobs;
pub mod secret;
pub mod store;
pub mod util;
pub mod workspace;

pub use error::{Error, Result};
