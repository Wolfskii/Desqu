//! The desktop layer's public contract. One process-wide layer serves every module (Vials,
//! Workspaces); each module talks to it through its own handle, tagged with an *owner* name.

use crate::model::{DesktopItem, Point};

#[derive(Debug, thiserror::Error)]
pub enum DesktopError {
    /// Another process (a standalone app, or Desqu) holds the desktop-layer lock.
    #[error("another Desqu app is arranging the desktop")]
    OwnedElsewhere,
    #[error("the desktop layer is not supported on this operating system")]
    Unsupported,
    /// Explorer's desktop view could not be reached (Explorer starting up or restarting).
    #[error("the desktop view is not available: {0}")]
    ViewUnavailable(String),
    /// "Auto arrange icons" is on, so the shell ignores icon positions.
    #[error("turn off \"Auto arrange icons\" on the desktop to hide items")]
    AutoArrange,
    #[error("{op} failed: {message}")]
    Native { op: &'static str, message: String },
    #[error("desktop item not found: {0}")]
    NotFound(String),
    #[error("i/o error: {0}")]
    Io(#[from] std::io::Error),
}

pub type DesktopResult<T> = Result<T, DesktopError>;

/// Result of a hide request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HideReport {
    /// Items now parked (including ones another owner had already parked).
    pub hidden: Vec<String>,
    /// Items the desktop view does not show (yet): it lags the file system by several seconds, so
    /// callers retry these after the next change notification.
    pub missing: Vec<String>,
    /// Items Explorer did not leave parked (it refused, timed out or moved them back). They are
    /// still claimed in the journal, so asking again parks them again.
    pub failed: Vec<String>,
}

/// The desktop folders changed (an item was added, removed or renamed). Coarse on purpose: the
/// consumer re-lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DesktopEvent {
    Changed,
}

pub type EventSink = Box<dyn Fn(DesktopEvent) + Send + 'static>;

/// Keeps event delivery alive; dropping it stops the watcher threads.
pub struct Subscription {
    stop: Option<Box<dyn FnOnce() + Send>>,
}

impl Subscription {
    pub fn new(stop: impl FnOnce() + Send + 'static) -> Self {
        Self { stop: Some(Box::new(stop)) }
    }
}

impl Drop for Subscription {
    fn drop(&mut self) {
        if let Some(stop) = self.stop.take() {
            stop();
        }
    }
}

pub trait DesktopLayer: Send + Sync {
    /// Everything the desktop shows right now, including parked items.
    fn items(&self) -> DesktopResult<Vec<DesktopItem>>;

    /// The item's icon as a PNG, `size` x `size` pixels. Slow (about 6 ms each): call off the UI
    /// thread and cache.
    fn icon_png(&self, id: &str, size: u32) -> DesktopResult<Vec<u8>>;

    /// `false` when auto-arrange is on and hiding would not work.
    fn can_hide(&self) -> DesktopResult<bool>;

    /// Hides items for this owner. An item stays hidden until every owner that hid it shows it.
    fn hide(&self, ids: &[String]) -> DesktopResult<HideReport>;

    /// Stops hiding items for this owner.
    fn show(&self, ids: &[String]) -> DesktopResult<()>;

    /// Like `show`, but puts the item where the cursor is (`screen`: screen pixels) instead of
    /// where it was. Used when the user drags an item out of a container onto the desktop. If another
    /// owner still hides the item it stays hidden and nothing moves.
    fn show_at(&self, id: &str, screen: Point) -> DesktopResult<()>;

    /// Stops hiding everything this owner hid.
    fn show_all(&self) -> DesktopResult<()>;

    /// Ids this owner currently hides.
    fn hidden(&self) -> DesktopResult<Vec<String>>;

    /// Calls `sink` (on another thread) whenever the desktop folders change.
    fn subscribe(&self, sink: EventSink) -> DesktopResult<Subscription>;
}
