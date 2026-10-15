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
    /// Items now hidden (including ones another owner had already hidden).
    pub hidden: Vec<String>,
    /// Items the desktop view does not show (yet): it lags the file system by several seconds, so
    /// callers retry these after the next change notification.
    pub missing: Vec<String>,
    /// Items that cannot be hidden: system items such as the Recycle Bin (no file to mark), or files
    /// Windows would not let the attribute be set on (the Public Desktop needs administrator rights).
    /// They stay visible and are not claimed.
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

/// What a claim change tells its listeners: some owner hid or showed items.
pub type ClaimSink = Box<dyn Fn() + Send + 'static>;

pub trait DesktopLayer: Send + Sync {
    /// Everything the desktop shows right now, including parked items.
    fn items(&self) -> DesktopResult<Vec<DesktopItem>>;

    /// The item's icon as a PNG, `size` x `size` pixels. Slow (about 6 ms each): call off the UI
    /// thread and cache.
    fn icon_png(&self, id: &str, size: u32) -> DesktopResult<Vec<u8>>;

    /// `false` when Windows is set to show hidden files, so hidden items would stay visible.
    fn can_hide(&self) -> DesktopResult<bool>;

    /// Hides items for this owner (they get the hidden attribute, or while a surface is active just
    /// a claim). An item stays hidden until every owner that hid it shows it. A hidden item is still
    /// listed by [`DesktopLayer::items`], flagged `parked`.
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

    /// Switches the desktop's own icon layer off (`true`) or back on, for this owner. While it is
    /// off, the owner draws the desktop's icons itself, and `hide`/`show` no longer move anything:
    /// they only record which items *no one should draw* (see [`DesktopItem::parked`]). It works
    /// where Windows refuses to park icons off-screen. Items parked before the switch are put back.
    /// Turning it off again puts the owners' hidden items back to parking.
    fn set_surface(&self, on: bool) -> DesktopResult<()>;

    /// Whether the icon layer is switched off by an owner (see [`DesktopLayer::set_surface`]).
    fn surface_active(&self) -> bool;

    /// Calls `sink` (on another thread) whenever any owner hides or shows items or the surface is
    /// switched, so a module drawing the desktop can redraw.
    fn subscribe_claims(&self, sink: ClaimSink) -> DesktopResult<Subscription>;
}
