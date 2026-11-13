//! Platform-neutral contracts for native event delivery and surface hosting.
//!
//! The platform backend does not know about JavaScript, canvas objects, or task
//! queues. It only waits for native events and performs native surface
//! operations requested by `ScriptThread` through these traits.

use std::{error::Error, fmt, time::Duration};

use crate::{
    canvas::{Snapshot, WidgetId, WidgetSpec},
    task::PointerKind,
};

#[derive(Debug, Clone)]
pub enum PlatformEvent {
    Pointer {
        widget_id: WidgetId,
        kind: PointerKind,
        x: f64,
        y: f64,
        button: u32,
        buttons: u32,
    },
    Resized {
        widget_id: WidgetId,
        width: u32,
        height: u32,
    },
    Closed {
        widget_id: WidgetId,
    },
    /// This surface's native frame clock has made its rAF callbacks eligible.
    FrameReady {
        widget_id: WidgetId,
    },
}

#[derive(Debug, Clone)]
pub struct PlatformError {
    message: String,
}

impl PlatformError {
    pub fn new(message: impl Into<String>) -> Self {
        Self { message: message.into() }
    }
}

impl fmt::Display for PlatformError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl Error for PlatformError {}

/// Wait for native events and return the events observed during one dispatch.
///
/// `None` means wait until a native event arrives. Implementations must never
/// call JS or enqueue application tasks.
pub trait PlatformEventSource {
    fn dispatch(
        &mut self,
        timeout: Option<Duration>,
    ) -> Result<Vec<PlatformEvent>, PlatformError>;
}

/// Native surface operations used by `ScriptThread`.
///
/// Surface configuration is passed as a complete desired state, not as a
/// stream of commands. The script thread compares revisions in its shared
/// canvas model and invokes `update_widget` only when the state changes.
pub trait PlatformHost {
    fn create_widget(&mut self, widget_id: WidgetId, spec: &WidgetSpec);
    fn update_widget(&mut self, widget_id: WidgetId, spec: &WidgetSpec);
    fn destroy_widget(&mut self, widget_id: WidgetId);

    /// Submit a complete canvas snapshot. `false` means the surface could not
    /// accept it yet; the canvas stays dirty and will be retried.
    fn present(&mut self, widget_id: WidgetId, snapshot: &Snapshot, wants_frame: bool) -> bool;

    fn is_widget_configured(&self, widget_id: WidgetId) -> bool;
    fn has_pending_frame_callback(&self, widget_id: WidgetId) -> bool;
    fn request_frame_callback(&mut self, widget_id: WidgetId);
    fn reset_frame_callback(&mut self, widget_id: WidgetId);
}

/// A native backend combines event delivery and surface hosting.
pub trait Platform: PlatformEventSource + PlatformHost {}

impl<T> Platform for T where T: PlatformEventSource + PlatformHost {}
