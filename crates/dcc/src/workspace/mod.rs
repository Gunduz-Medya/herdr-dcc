//! Workspaces. A definition is a list of tabs, each with a split tree whose leaves are panes (a
//! folder and an optional startup command). Two kinds exist. A template is a tree we wrote, so it
//! is exact. A snapshot is read from a live space, and because Herdr does not tell the split
//! direction or size, its tree is only a row of panes in order.

pub mod def;
pub mod ops;
pub mod templates;

pub use def::{Definition, Direction, Node, TabDef};
