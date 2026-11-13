use std::{
    collections::{HashMap, VecDeque},
    fs::File,
    os::fd::AsFd,
    time::Duration,
};

use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;

use memmap2::{MmapMut, MmapOptions};
use wayland_client::{
    Connection, Dispatch, Proxy, QueueHandle, WEnum, delegate_noop,
    protocol::{
        wl_buffer, wl_callback, wl_compositor, wl_pointer, wl_registry, wl_seat, wl_shm,
        wl_shm_pool, wl_surface,
    },
};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::{self, Layer, ZwlrLayerShellV1},
    zwlr_layer_surface_v1::{self, Anchor, ZwlrLayerSurfaceV1},
};

use crate::{
    canvas::{Snapshot, WidgetId, WidgetSpec},
    platform::{PlatformError, PlatformEvent, PlatformEventSource, PlatformHost},
    task::PointerKind,
};

const BTN_LEFT: u32 = 0x110;
const BTN_RIGHT: u32 = 0x111;
const BTN_MIDDLE: u32 = 0x112;

const ANCHOR_TOP: u32 = 0x1;
const ANCHOR_BOTTOM: u32 = 0x2;
const ANCHOR_LEFT: u32 = 0x4;
const ANCHOR_RIGHT: u32 = 0x8;

fn anchor_from_flags(flags: u32) -> Anchor {
    let mut anchor = Anchor::empty();
    if flags & ANCHOR_TOP != 0 {
        anchor |= Anchor::Top;
    }
    if flags & ANCHOR_BOTTOM != 0 {
        anchor |= Anchor::Bottom;
    }
    if flags & ANCHOR_LEFT != 0 {
        anchor |= Anchor::Left;
    }
    if flags & ANCHOR_RIGHT != 0 {
        anchor |= Anchor::Right;
    }
    anchor
}

/// A zero-sized dimension is stretched by the layer-shell protocol, which
/// requires both opposite edges to be anchored.
fn resolve_anchor(flags: u32, width: u32, height: u32) -> Anchor {
    let mut anchor = anchor_from_flags(flags);
    if width == 0 {
        anchor |= Anchor::Left | Anchor::Right;
    }
    if height == 0 {
        anchor |= Anchor::Top | Anchor::Bottom;
    }
    anchor
}

pub struct WaylandState {
    pub qh: QueueHandle<WaylandState>,
    pub wl_compositor: Option<wl_compositor::WlCompositor>,
    pub wl_shm: Option<wl_shm::WlShm>,
    pub wlr_layer_shell: Option<ZwlrLayerShellV1>,
    pub wl_seat: Option<wl_seat::WlSeat>,
    pub wl_pointer: Option<wl_pointer::WlPointer>,
    windows: HashMap<WidgetId, WlWindow>,
    pointer_focus: Option<WidgetId>,
    pointer_pressed: Option<WidgetId>,
    pointer_buttons: u32,
    pointer_x: f64,
    pointer_y: f64,
    events: VecDeque<PlatformEvent>,
}

#[derive(Clone, Copy, Debug)]
struct FrameRequest {
    widget: WidgetId,
}

struct WlWindow {
    surface: wl_surface::WlSurface,
    layer_surface: ZwlrLayerSurfaceV1,
    width: u32,
    height: u32,
    anchor: u32,
    configured: bool,
    has_buffer: bool,
    frame_callback: Option<wl_callback::WlCallback>,
    // One shm pool per WidgetCanvas. It is recreated only when the canvas
    // dimensions change and no buffer is in flight, then dropped with the
    // widget surface.
    pool: Option<WidgetShmPool>,
}

#[derive(Clone, Copy, Debug)]
struct BufferSlot {
    widget: WidgetId,
    offset: usize,
}

struct WidgetShmPool {
    pool: wl_shm_pool::WlShmPool,
    file: File,
    map: MmapMut,
    slot_len: usize,
    slots: usize,
    free: Vec<usize>,
    in_flight: Vec<wl_buffer::WlBuffer>,
}

impl WidgetShmPool {
    fn new(
        shm: &wl_shm::WlShm,
        qh: &QueueHandle<WaylandState>,
        slot_len: usize,
        slots: usize,
    ) -> Option<Self> {
        let total_len = slot_len.checked_mul(slots)?;
        let file = tempfile::tempfile().ok()?;
        file.set_len(total_len as u64).ok()?;
        let map = unsafe { MmapOptions::new().len(total_len).map_mut(&file) }.ok()?;
        let pool = shm.create_pool(file.as_fd(), total_len as i32, qh, ());
        let free = (0..slots).rev().map(|slot| slot * slot_len).collect();
        Some(Self {
            pool,
            file,
            map,
            slot_len,
            slots,
            free,
            in_flight: Vec::new(),
        })
    }

    fn alloc_offset(&mut self) -> Option<usize> {
        if let Some(offset) = self.free.pop() {
            return Some(offset);
        }

        // All slots are in flight. Grow the pool by one slot; existing buffer
        // offsets stay valid because wl_shm_pool.resize only extends the pool.
        let offset = self.slots.checked_mul(self.slot_len)?;
        self.slots = self.slots.checked_add(1)?;
        let total_len = self.slots.checked_mul(self.slot_len)?;
        self.file.set_len(total_len as u64).ok()?;
        self.pool.resize(total_len as i32);
        self.map = unsafe { MmapOptions::new().len(total_len).map_mut(&self.file) }.ok()?;
        Some(offset)
    }

    fn write_snapshot(&mut self, offset: usize, snapshot: &Snapshot) {
        let byte_len = snapshot.size.width as usize * snapshot.size.height as usize * 4;
        let destination = &mut self.map[offset..offset + byte_len];
        for (destination, source) in destination
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(snapshot.data.as_chunks::<4>().0)
        {
            destination[0] = source[2];
            destination[1] = source[1];
            destination[2] = source[0];
            destination[3] = source[3];
        }
    }

    fn create_buffer(
        &self,
        qh: &QueueHandle<WaylandState>,
        slot: BufferSlot,
        width: u32,
        height: u32,
        stride: i32,
    ) -> wl_buffer::WlBuffer {
        self.pool.create_buffer(
            slot.offset as i32,
            width as i32,
            height as i32,
            stride,
            wl_shm::Format::Argb8888,
            qh,
            slot,
        )
    }

    fn release(&mut self, buffer: &wl_buffer::WlBuffer, offset: usize) {
        let released = buffer.id();
        self.in_flight
            .retain(|in_flight| in_flight.id() != released);
        self.free.push(offset);
    }
}

impl Drop for WidgetShmPool {
    fn drop(&mut self) {
        self.pool.destroy();
    }
}

impl WlWindow {
    fn prepare_pool(
        &mut self,
        shm: &wl_shm::WlShm,
        qh: &QueueHandle<WaylandState>,
        byte_len: usize,
    ) -> bool {
        if let Some(pool) = &self.pool {
            if pool.slot_len == byte_len {
                return true;
            }

            // Shrinking (or growing) changes slot offsets, so only rebuild when
            // nothing is in flight. While waiting, reuse the old slots if they
            // are large enough.
            if !pool.in_flight.is_empty() {
                return pool.slot_len >= byte_len;
            }
        }

        self.pool = WidgetShmPool::new(shm, qh, byte_len, 4);
        self.pool.is_some()
    }
}

impl WaylandState {
    pub fn new(qh: QueueHandle<WaylandState>) -> Self {
        Self {
            qh,
            wl_compositor: None,
            wl_shm: None,
            wlr_layer_shell: None,
            wl_seat: None,
            wl_pointer: None,
            windows: HashMap::new(),
            pointer_focus: None,
            pointer_pressed: None,
            pointer_buttons: 0,
            pointer_x: 0.0,
            pointer_y: 0.0,
            events: VecDeque::new(),
        }
    }

    fn take_events(&mut self) -> Vec<PlatformEvent> {
        self.events.drain(..).collect()
    }

    fn push_pointer(
        &mut self,
        widget_id: WidgetId,
        kind: PointerKind,
        x: f64,
        y: f64,
        button: u32,
        buttons: u32,
    ) {
        self.events.push_back(PlatformEvent::Pointer {
            widget_id,
            kind,
            x,
            y,
            button,
            buttons,
        });
    }

    fn handle_configure(&mut self, widget_id: WidgetId, serial: u32, width: u32, height: u32) {
        self.ack_configure(widget_id, serial);
        if let Some(window) = self.windows.get_mut(&widget_id) {
            window.configured = true;
            window.width = width;
            window.height = height;
        }
        self.events.push_back(PlatformEvent::Resized {
            widget_id,
            width,
            height,
        });
    }

    fn widget_for_surface(&self, surface: &wl_surface::WlSurface) -> Option<WidgetId> {
        self.windows
            .iter()
            .find(|(_, window)| window.surface.id() == surface.id())
            .map(|(widget_id, _)| *widget_id)
    }

    pub fn create_widget(&mut self, id: WidgetId, spec: &WidgetSpec) {
        let (Some(compositor), Some(shell)) = (&self.wl_compositor, &self.wlr_layer_shell) else {
            return;
        };

        let surface = compositor.create_surface(&self.qh, id);
        let layer_surface = shell.get_layer_surface(
            &surface,
            None,
            Layer::Top,
            "widjet".to_string(),
            &self.qh,
            id,
        );

        layer_surface.set_anchor(resolve_anchor(spec.anchor, spec.width, spec.height));
        if spec.width != 0 || spec.height != 0 {
            layer_surface.set_size(spec.width, spec.height);
        }
        layer_surface.set_exclusive_zone(spec.exclusive_zone);
        layer_surface.set_margin(
            spec.margin_top,
            spec.margin_right,
            spec.margin_bottom,
            spec.margin_left,
        );
        surface.commit();

        self.windows.insert(
            id,
            WlWindow {
                surface,
                layer_surface,
                width: spec.width,
                height: spec.height,
                anchor: spec.anchor,
                configured: false,
                has_buffer: false,
                frame_callback: None,
                pool: None,
            },
        );
    }

    pub fn destroy_widget(&mut self, id: WidgetId) {
        if self.pointer_focus == Some(id) {
            self.pointer_focus = None;
        }
        if self.pointer_pressed == Some(id) {
            self.pointer_pressed = None;
        }
        if let Some(window) = self.windows.remove(&id) {
            window.layer_surface.destroy();
            window.surface.destroy();
        }
    }

    pub fn is_widget_configured(&self, widget_id: WidgetId) -> bool {
        self.windows
            .get(&widget_id)
            .is_some_and(|window| window.configured)
    }

    pub fn has_pending_frame_callback(&self, widget_id: WidgetId) -> bool {
        self.windows
            .get(&widget_id)
            .is_some_and(|window| window.frame_callback.is_some())
    }

    /// Request a frame callback for exactly one surface. Each surface has its
    /// own compositor clock; callbacks from other surfaces must not release
    /// this surface's requestAnimationFrame queue.
    pub fn request_frame_callback(&mut self, widget_id: WidgetId) {
        let qh = self.qh.clone();
        let Some(window) = self.windows.get_mut(&widget_id) else {
            return;
        };
        if window.configured && window.has_buffer && window.frame_callback.is_none() {
            window.frame_callback = Some(
                window
                    .surface
                    .frame(&qh, FrameRequest { widget: widget_id }),
            );
            window.surface.commit();
        }
    }

    pub fn reset_frame_callback(&mut self, widget_id: WidgetId) {
        if let Some(window) = self.windows.get_mut(&widget_id) {
            window.frame_callback = None;
        }
    }

    fn frame_done(&mut self, widget: WidgetId, callback: &wl_callback::WlCallback) -> bool {
        let Some(window) = self.windows.get_mut(&widget) else {
            return false;
        };
        let matches = window
            .frame_callback
            .as_ref()
            .is_some_and(|in_flight| in_flight.id() == callback.id());
        if matches {
            window.frame_callback = None;
        }
        matches
    }

    /// Apply a complete desired widget configuration. ScriptThread calls this
    /// only when the corresponding CanvasStore revision changes.
    pub fn update_widget(&mut self, id: WidgetId, spec: &WidgetSpec) {
        let Some(window) = self.windows.get_mut(&id) else {
            return;
        };
        window.width = spec.width;
        window.height = spec.height;
        window.anchor = spec.anchor;
        window.layer_surface.set_size(spec.width, spec.height);
        window
            .layer_surface
            .set_anchor(resolve_anchor(spec.anchor, spec.width, spec.height));
        window.layer_surface.set_exclusive_zone(spec.exclusive_zone);
        window.layer_surface.set_margin(
            spec.margin_top,
            spec.margin_right,
            spec.margin_bottom,
            spec.margin_left,
        );
        window.surface.commit();
    }

    pub fn ack_configure(&self, id: WidgetId, serial: u32) {
        if let Some(window) = self.windows.get(&id) {
            window.layer_surface.ack_configure(serial);
            window.surface.commit();
        }
    }

    /// Returns whether the snapshot was committed to the compositor.
    pub fn present(&mut self, id: WidgetId, snapshot: &Snapshot, wants_frame: bool) -> bool {
        let Some(shm) = &self.wl_shm else {
            return false;
        };
        let qh = self.qh.clone();
        let Some(window) = self.windows.get_mut(&id) else {
            return false;
        };
        if !window.configured || snapshot.size.is_empty() {
            return false;
        }

        let width = snapshot.size.width;
        let height = snapshot.size.height;
        let stride = width as i32 * 4;
        let byte_len = stride as usize * height as usize;

        if !window.prepare_pool(shm, &self.qh, byte_len) {
            return false;
        }
        let Some(pool) = window.pool.as_mut() else {
            return false;
        };
        let Some(offset) = pool.alloc_offset() else {
            return false;
        };
        pool.write_snapshot(offset, snapshot);
        let slot = BufferSlot { widget: id, offset };
        let buffer = pool.create_buffer(&qh, slot, width, height, stride);

        if wants_frame && window.frame_callback.is_none() {
            window.frame_callback = Some(window.surface.frame(&qh, FrameRequest { widget: id }));
        }

        window.surface.attach(Some(&buffer), 0, 0);
        window.surface.damage(0, 0, width as i32, height as i32);
        window.surface.commit();
        window.has_buffer = true;
        if let Some(pool) = window.pool.as_mut() {
            pool.in_flight.push(buffer);
        }
        true
    }

    fn release_buffer(&mut self, slot: BufferSlot, buffer: &wl_buffer::WlBuffer) {
        let Some(window) = self.windows.get_mut(&slot.widget) else {
            return;
        };
        if let Some(pool) = window.pool.as_mut() {
            pool.release(buffer, slot.offset);
        }
        buffer.destroy();
    }
}

/// Wayland implementation of the platform event source and surface host.
pub struct WaylandPlatform {
    event_loop: EventLoop<'static, WaylandState>,
    state: WaylandState,
}

impl WaylandPlatform {
    pub fn new() -> Result<Self, PlatformError> {
        let connection = Connection::connect_to_env().map_err(|error| {
            PlatformError::new(format!("failed to connect to Wayland: {error}"))
        })?;
        let mut event_queue = connection.new_event_queue::<WaylandState>();
        let qh = event_queue.handle();
        let mut state = WaylandState::new(qh);

        connection.display().get_registry(&state.qh, ());
        event_queue.roundtrip(&mut state).map_err(|error| {
            PlatformError::new(format!("Wayland registry roundtrip failed: {error:?}"))
        })?;

        if state.wl_compositor.is_none()
            || state.wl_shm.is_none()
            || state.wlr_layer_shell.is_none()
        {
            return Err(PlatformError::new(
                "Wayland compositor, wl_shm, or zwlr_layer_shell_v1 is unavailable",
            ));
        }

        let event_loop: EventLoop<'static, WaylandState> =
            EventLoop::try_new().map_err(|error| {
                PlatformError::new(format!("failed to create event loop: {error:?}"))
            })?;
        WaylandSource::new(connection, event_queue)
            .insert(event_loop.handle())
            .map_err(|error| {
                PlatformError::new(format!("failed to register Wayland source: {error:?}"))
            })?;

        Ok(Self { event_loop, state })
    }
}

impl PlatformEventSource for WaylandPlatform {
    fn dispatch(&mut self, timeout: Option<Duration>) -> Result<Vec<PlatformEvent>, PlatformError> {
        self.event_loop
            .dispatch(timeout, &mut self.state)
            .map_err(|error| PlatformError::new(format!("Wayland dispatch failed: {error:?}")))?;
        Ok(self.state.take_events())
    }
}

impl PlatformHost for WaylandPlatform {
    fn create_widget(&mut self, widget_id: WidgetId, spec: &WidgetSpec) {
        self.state.create_widget(widget_id, spec);
    }

    fn destroy_widget(&mut self, widget_id: WidgetId) {
        self.state.destroy_widget(widget_id);
    }

    fn update_widget(&mut self, widget_id: WidgetId, spec: &WidgetSpec) {
        self.state.update_widget(widget_id, spec);
    }

    fn present(&mut self, widget_id: WidgetId, snapshot: &Snapshot, wants_frame: bool) -> bool {
        self.state.present(widget_id, snapshot, wants_frame)
    }

    fn is_widget_configured(&self, widget_id: WidgetId) -> bool {
        self.state.is_widget_configured(widget_id)
    }

    fn has_pending_frame_callback(&self, widget_id: WidgetId) -> bool {
        self.state.has_pending_frame_callback(widget_id)
    }

    fn request_frame_callback(&mut self, widget_id: WidgetId) {
        self.state.request_frame_callback(widget_id);
    }

    fn reset_frame_callback(&mut self, widget_id: WidgetId) {
        self.state.reset_frame_callback(widget_id);
    }
}

impl Dispatch<wl_registry::WlRegistry, ()> for WaylandState {
    fn event(
        state: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_registry::Event::Global {
            name,
            interface,
            version,
        } = event
        else {
            return;
        };

        match interface.as_str() {
            "wl_compositor" => {
                state.wl_compositor = Some(registry.bind(name, version.min(6), qh, ()))
            }
            "wl_shm" => state.wl_shm = Some(registry.bind(name, version.min(1), qh, ())),
            "zwlr_layer_shell_v1" => {
                state.wlr_layer_shell = Some(registry.bind(name, version.min(5), qh, ()))
            }
            "wl_seat" => state.wl_seat = Some(registry.bind(name, version.min(7), qh, ())),
            _ => {}
        }
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, WidgetId> for WaylandState {
    fn event(
        state: &mut Self,
        _: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        widget_id: &WidgetId,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width,
                height,
            } => state.handle_configure(*widget_id, serial, width, height),
            zwlr_layer_surface_v1::Event::Closed => {
                state.destroy_widget(*widget_id);
                state.events.push_back(PlatformEvent::Closed {
                    widget_id: *widget_id,
                });
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_callback::WlCallback, FrameRequest> for WaylandState {
    fn event(
        state: &mut Self,
        callback: &wl_callback::WlCallback,
        event: wl_callback::Event,
        data: &FrameRequest,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_callback::Event::Done { .. } = event
            && state.frame_done(data.widget, callback)
        {
            state.events.push_back(PlatformEvent::FrameReady {
                widget_id: data.widget,
            });
        }
    }
}

impl Dispatch<wl_shm::WlShm, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &wl_shm::WlShm,
        _: <wl_shm::WlShm as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        // TODO: record supported formats.
    }
}

impl Dispatch<wl_shm_pool::WlShmPool, ()> for WaylandState {
    fn event(
        _: &mut Self,
        _: &wl_shm_pool::WlShmPool,
        _: <wl_shm_pool::WlShmPool as Proxy>::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

impl Dispatch<wl_buffer::WlBuffer, BufferSlot> for WaylandState {
    fn event(
        state: &mut Self,
        buffer: &wl_buffer::WlBuffer,
        event: <wl_buffer::WlBuffer as Proxy>::Event,
        slot: &BufferSlot,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        if let wl_buffer::Event::Release = event {
            state.release_buffer(*slot, buffer);
        }
    }
}

impl Dispatch<wl_surface::WlSurface, WidgetId> for WaylandState {
    fn event(
        _: &mut Self,
        _: &wl_surface::WlSurface,
        _: <wl_surface::WlSurface as Proxy>::Event,
        _: &WidgetId,
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
    }
}

delegate_noop!(WaylandState: ignore wl_compositor::WlCompositor);
delegate_noop!(WaylandState: ignore zwlr_layer_shell_v1::ZwlrLayerShellV1);

impl Dispatch<wl_seat::WlSeat, ()> for WaylandState {
    fn event(
        state: &mut Self,
        seat: &wl_seat::WlSeat,
        event: wl_seat::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        let wl_seat::Event::Capabilities {
            capabilities: WEnum::Value(capabilities),
        } = event
        else {
            return;
        };

        if capabilities.contains(wl_seat::Capability::Pointer) {
            if state.wl_pointer.is_none() {
                state.wl_pointer = Some(seat.get_pointer(qh, ()));
            }
        } else if let Some(pointer) = state.wl_pointer.take() {
            pointer.release();
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for WaylandState {
    fn event(
        state: &mut Self,
        _: &wl_pointer::WlPointer,
        event: wl_pointer::Event,
        _: &(),
        _: &Connection,
        _: &QueueHandle<Self>,
    ) {
        match event {
            wl_pointer::Event::Enter {
                surface,
                surface_x,
                surface_y,
                ..
            } => {
                let widget_id = state.widget_for_surface(&surface);
                state.pointer_focus = widget_id;
                state.pointer_x = surface_x;
                state.pointer_y = surface_y;
                if let Some(widget_id) = widget_id {
                    state.push_pointer(
                        widget_id,
                        PointerKind::Enter,
                        surface_x,
                        surface_y,
                        0,
                        state.pointer_buttons,
                    );
                }
            }
            wl_pointer::Event::Leave { .. } => {
                if let Some(widget_id) = state.pointer_focus.take() {
                    state.push_pointer(
                        widget_id,
                        PointerKind::Leave,
                        state.pointer_x,
                        state.pointer_y,
                        0,
                        state.pointer_buttons,
                    );
                }
                state.pointer_pressed = None;
            }
            wl_pointer::Event::Motion {
                surface_x,
                surface_y,
                ..
            } => {
                state.pointer_x = surface_x;
                state.pointer_y = surface_y;
                if let Some(widget_id) = state.pointer_focus {
                    state.push_pointer(
                        widget_id,
                        PointerKind::Move,
                        surface_x,
                        surface_y,
                        0,
                        state.pointer_buttons,
                    );
                }
            }
            wl_pointer::Event::Button {
                button,
                state: button_state,
                ..
            } => {
                let Some(widget_id) = state.pointer_focus else {
                    return;
                };

                let (mask, dom_button) = match button {
                    BTN_LEFT => (1, 0),
                    BTN_RIGHT => (2, 2),
                    BTN_MIDDLE => (4, 1),
                    _ => return,
                };

                match button_state {
                    WEnum::Value(wl_pointer::ButtonState::Pressed) => {
                        state.pointer_buttons |= mask;
                        state.push_pointer(
                            widget_id,
                            PointerKind::Down,
                            state.pointer_x,
                            state.pointer_y,
                            dom_button,
                            state.pointer_buttons,
                        );

                        if dom_button == 0 {
                            state.pointer_pressed = Some(widget_id);
                        } else if dom_button == 2 {
                            state.push_pointer(
                                widget_id,
                                PointerKind::ContextMenu,
                                state.pointer_x,
                                state.pointer_y,
                                dom_button,
                                state.pointer_buttons,
                            );
                        }
                    }
                    WEnum::Value(wl_pointer::ButtonState::Released) => {
                        state.pointer_buttons &= !mask;
                        state.push_pointer(
                            widget_id,
                            PointerKind::Up,
                            state.pointer_x,
                            state.pointer_y,
                            dom_button,
                            state.pointer_buttons,
                        );

                        if dom_button == 0 && state.pointer_pressed == Some(widget_id) {
                            state.push_pointer(
                                widget_id,
                                PointerKind::Click,
                                state.pointer_x,
                                state.pointer_y,
                                dom_button,
                                state.pointer_buttons,
                            );
                        }
                        if dom_button == 0 {
                            state.pointer_pressed = None;
                        }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}
