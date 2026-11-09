use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;
use wayland_client::{
    Connection,
    protocol::{wl_compositor, wl_pointer::WlPointer, wl_seat::WlSeat, wl_shm::WlShm},
};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::ZwlrLayerShellV1;

use crate::js::Agent;

/// The full app state
pub struct App {
    pub running: bool,

    // Wayland globals
    pub wl_compositor: Option<wl_compositor::WlCompositor>,
    pub wl_pointer: Option<WlPointer>,
    pub wl_shm: Option<WlShm>,
    pub wl_seat: Option<WlSeat>,
    pub wlr_layer_shell: Option<ZwlrLayerShellV1>,

    // JS agent
    pub agent: Agent,

    pub render_opportunity: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            running: true,
            wl_compositor: None,
            wl_pointer: None,
            wl_shm: None,
            wl_seat: None,
            wlr_layer_shell: None,
            agent: Agent::new(),
            render_opportunity: true,
        }
    }

    pub fn run(&mut self) {
        let connection = Connection::connect_to_env().unwrap();
        let mut event_queue = connection.new_event_queue::<App>();
        let qh = event_queue.handle();
        let display = connection.display();

        display.get_registry(&qh, ());
        event_queue.roundtrip(self).unwrap();

        let mut event_loop: EventLoop<App> = EventLoop::try_new().unwrap();
        WaylandSource::new(connection, event_queue)
            .insert(event_loop.handle())
            .unwrap();

        // the event loop
        while self.running {
            // 1. handle external events
            event_loop.dispatch(None, self).unwrap();

            // TODO: the full event loop
            // 1. handle calloop
            // 2. determine render opportunity
            // 3. enqueue app task queue (for js)
            // 4. a macro task to js
            // 5. run js till no microtask
            // 6. render
        }
    }
}
