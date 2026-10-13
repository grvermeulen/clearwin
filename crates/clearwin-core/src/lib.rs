//! Scanning and cleaning engine for ClearWin.
//!
//! The engine works in two phases so nothing is ever deleted unseen:
//! [`scan`] builds a list of exactly what would be removed, and [`clean`]
//! deletes only entries from that list, re-checking each one first.

mod catalog;
mod engine;
mod firefox;
mod paths;
mod processes;
mod recycle_bin;

pub use catalog::{catalog, Action, Category, NameFilter, Target};
pub use engine::{
    clean_category, scan_category, CategoryClean, CategoryScan, CategorySummary, FileEntry,
};
pub use paths::Paths;
pub use processes::RunningApps;
