use std::{cell::RefCell, rc::Rc};

use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;
use rquickjs::{JsLifetime, class::Trace};
use wayland_client::{
    Connection,
    protocol::{wl_compositor, wl_pointer::WlPointer, wl_seat::WlSeat, wl_shm::WlShm},
};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::ZwlrLayerShellV1;

use crate::js::Agent;

pub struct App {
    /// Wayland states
    pub state: Rc<RefCell<State>>,
    /// JS agent
    pub agent: Agent,
}

impl App {
    pub fn new() -> Self {
        let state = Rc::new(RefCell::new(State::new()));
        let agent = Agent::new(state.clone());
        Self { state, agent }
    }

    pub fn run(&self) {
        let connection = Connection::connect_to_env().unwrap();
        let mut event_queue = connection.new_event_queue::<State>();
        let qh = event_queue.handle();
        let display = connection.display();

        display.get_registry(&qh, ());
        event_queue
            .roundtrip(&mut *self.state.borrow_mut())
            .unwrap();

        let mut event_loop: EventLoop<State> = EventLoop::try_new().unwrap();
        WaylandSource::new(connection, event_queue)
            .insert(event_loop.handle())
            .unwrap();

        // the event loop
        while self.state.borrow().running {
            // 1. handle external events
            event_loop
                .dispatch(None, &mut *self.state.borrow_mut())
                .unwrap();

            // TODO: the full event loop
            // 2. determine render opportunity
            // 3. enqueue app task queue (for js)
            // 4. a macro task to js
            // 5. run js till no microtask
            // 6. render
        }
    }
}

/// The full app state
#[derive(Debug, JsLifetime)]
pub struct State {
    pub running: bool,
    pub render_opportunity: bool,

    // Wayland globals
    pub wl_compositor: Option<wl_compositor::WlCompositor>,
    pub wl_pointer: Option<WlPointer>,
    pub wl_shm: Option<WlShm>,
    pub wl_seat: Option<WlSeat>,
    pub wlr_layer_shell: Option<ZwlrLayerShellV1>,
}

impl State {
    pub fn new() -> Self {
        State {
            running: true,
            render_opportunity: true,
            wl_compositor: None,
            wl_pointer: None,
            wl_shm: None,
            wl_seat: None,
            wlr_layer_shell: None,
        }
    }
}

impl<'js> Trace<'js> for State {
    fn trace<'a>(&self, _tracer: rquickjs::class::Tracer<'a, 'js>) {
        // there is nothing to trace
    }
}
