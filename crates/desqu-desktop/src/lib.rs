//! Shared desktop-icon layer for Vials and Workspaces.
//!
//! It lists what the desktop shows, extracts icons, and hides items by *parking* them off-screen
//! (and restoring them), journaling every change so a crash never loses an item's position. The
//! approach comes from the P8.1 spike: see `apps/vials/docs/desktop-research.md`.
//!
//! One layer exists per process, shared by every module through [`open`]; a named lock keeps two
//! *processes* (for example a standalone Vials and Desqu) from rearranging the desktop at once.

pub mod layer;
pub mod model;
mod platform;

use std::path::Path;
use std::sync::Arc;

pub use layer::{DesktopError, DesktopEvent, DesktopLayer, DesktopResult, EventSink, HideReport, Subscription};
pub use model::{
    classify, is_parked, plan_recovery, DesktopItem, ItemKind, Journal, ParkedItem, Point, RecoveryPlan,
    FALLBACK_ORIGIN, PARK_POINT,
};

/// Connects `owner` (for example `"vials"`) to the process-wide layer, creating it if needed.
///
/// The first call decides where the journal lives (`journal_path`); later calls in the same process
/// share that layer and its journal. Creating the layer takes the cross-process lock
/// ([`DesktopError::OwnedElsewhere`] if another process has it) and first restores any items a
/// previous run left parked. Dropping the last handle restores everything.
pub fn open(owner: &str, journal_path: &Path) -> DesktopResult<Arc<dyn DesktopLayer>> {
    platform::open(owner, journal_path)
}

/// Restores items listed in another journal (for example a standalone app that crashed while
/// Desqu hosts the module now) and deletes it. Returns how many items were moved back.
pub fn recover_from(journal_path: &Path) -> DesktopResult<usize> {
    platform::recover_from(journal_path)
}

/// Where the desktop shows `item_id` right now, or `None` if it is not there. Read-only and takes
/// no lock, so it also works while another process owns the layer (for tests and diagnostics).
pub fn item_position(item_id: &str) -> DesktopResult<Option<Point>> {
    platform::item_position(item_id)
}
