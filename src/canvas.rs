use std::fs::File;

use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1;

pub struct CanvasImpl {
    wlr_layer_surface: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,

    // NOTE: we use a separate memory pool per canvas instead of global shm,
    // so that memory can be reclaimed each time a canvas is destroyed.
    tempfile: File,
}
