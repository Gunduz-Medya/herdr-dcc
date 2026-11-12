//! Everything that talks to Herdr. Herdr itself is never modified: dcc only calls the two programs
//! it ships, `herdr-app` (the window) and `hc` (herdr-core).

pub mod api;
pub mod compat;
pub mod core;
pub mod fake;
pub mod install;
pub mod isolation;
pub mod layout;
pub mod mods;
pub mod tabbar;
pub mod tabs;
