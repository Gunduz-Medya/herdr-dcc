//! SSH profiles, forwards and saved commands. Connections and forwards run as terminals of the
//! Herdr core (`hc run`), started through `dcc ssh run`, which hands the Keychain secret to ssh
//! over a private socket (see `askpass`).

pub mod askpass;
pub mod db;
pub mod model;
pub mod ops;
pub mod scan;
