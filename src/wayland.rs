use wayland_client::{
    Connection, Dispatch, QueueHandle, delegate_noop,
    protocol::{wl_buffer, wl_callback, wl_compositor, wl_pointer, wl_registry, wl_seat, wl_shm},
};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_surface_v1;

use crate::app::State;

impl Dispatch<wl_registry::WlRegistry, ()> for State {
    fn event(
        this: &mut Self,
        registry: &wl_registry::WlRegistry,
        event: wl_registry::Event,
        _: &(),
        _: &Connection,
        qh: &QueueHandle<Self>,
    ) {
        match event {
            wl_registry::Event::Global {
                name,
                interface,
                version,
            } => match interface.as_str() {
                "wl_compositor" => {
                    this.wl_compositor = Some(registry.bind(name, version.min(6), qh, ()))
                }
                "wl_shm" => this.wl_shm = Some(registry.bind(name, version.min(1), qh, ())),
                "wl_seat" => {}
                "zwlr_layer_shell_v1" => {}
                _ => {}
            },
            _ => {}
        }
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, u32> for State {
    fn event(
        state: &mut Self,
        proxy: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
        event: zwlr_layer_surface_v1::Event,
        canvas_id: &u32,
        conn: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        match event {
            zwlr_layer_surface_v1::Event::Configure {
                serial,
                width,
                height,
            } => {
                // TODO: get and resize canvas
            }
            zwlr_layer_surface_v1::Event::Closed => {
                println!("layer surface for canvas {canvas_id} closed");
                // TODO: should turn into js event queue
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_shm::WlShm, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &wl_shm::WlShm,
        event: <wl_shm::WlShm as wayland_client::Proxy>::Event,
        data: &(),
        conn: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        match event {
            wl_shm::Event::Format { format } => {
                // TODO: store allowed formats
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_buffer::WlBuffer, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &wl_buffer::WlBuffer,
        event: <wl_buffer::WlBuffer as wayland_client::Proxy>::Event,
        data: &(),
        conn: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        match event {
            wl_buffer::Event::Release => {
                // TODO: this is important for double buffer
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_seat::WlSeat, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &wl_seat::WlSeat,
        event: <wl_seat::WlSeat as wayland_client::Proxy>::Event,
        data: &(),
        conn: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        match event {
            wl_seat::Event::Capabilities { capabilities } => {
                // TODO
            }
            wl_seat::Event::Name { name } => {
                // TODO
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &wl_pointer::WlPointer,
        event: <wl_pointer::WlPointer as wayland_client::Proxy>::Event,
        data: &(),
        conn: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        // TODO: handle mouse event here (mapping to js event)
        match event {
            wl_pointer::Event::Enter {
                serial,
                surface,
                surface_x,
                surface_y,
            } => {}
            wl_pointer::Event::Leave { serial, surface } => {}
            wl_pointer::Event::Motion {
                time,
                surface_x,
                surface_y,
            } => {}
            wl_pointer::Event::Button {
                serial,
                time,
                button,
                state,
            } => {}
            wl_pointer::Event::Axis { time, axis, value } => {}
            wl_pointer::Event::Frame => {}
            wl_pointer::Event::AxisSource { axis_source } => {}
            wl_pointer::Event::AxisStop { time, axis } => {}
            wl_pointer::Event::AxisDiscrete { axis, discrete } => {}
            wl_pointer::Event::AxisValue120 { axis, value120 } => {}
            wl_pointer::Event::AxisRelativeDirection { axis, direction } => {}
            wl_pointer::Event::Warp {
                surface_x,
                surface_y,
            } => {}
            _ => {}
        }
    }
}

// TODO: should differentialize userdata to ensure that is from frame.
// also this should be per surface, (e.g. different output may have different vsync).
// So e.g. `FrameCallback(usize)`
impl Dispatch<wl_callback::WlCallback, ()> for State {
    fn event(
        state: &mut Self,
        proxy: &wl_callback::WlCallback,
        event: <wl_callback::WlCallback as wayland_client::Proxy>::Event,
        data: &(),
        conn: &Connection,
        qhandle: &QueueHandle<Self>,
    ) {
        match event {
            wl_callback::Event::Done { callback_data } => {
                // TODO: combine event and ... to draw.
            }
            _ => {}
        }
    }
}

delegate_noop!(State: ignore wl_compositor::WlCompositor);
