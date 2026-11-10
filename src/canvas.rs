use std::fs::File;

use wayland_client::protocol::{wl_shm::WlShm, wl_shm_pool::WlShmPool};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1;

// TODO: this should be changed to a trait to support windows and mac,
// and alias default as `PlatformCanvasBackend`.
//
// better: trait WidgetBackend { type Canvas = ... }, so that we do not
// need to expose platform specific type like WlShm.
//
// and the file should be like platform.rs.
//
// There should be a separate trait SurfaceBackend, so that we can use
// PixMap instead of platform surface to support OffscreenCanvas.
pub struct CanvasBackend {
    wlr_layer_surface: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,

    // NOTE: we use a separate memory pool per canvas instead of global shm,
    // so that memory can be reclaimed each time a canvas is destroyed.
    tempfile: File,
    pool: WlShmPool,
}

impl CanvasBackend {
    pub fn new(width: u32, height: u32, shm: &WlShm) -> Self {
        todo!()
    }
}
