use std::{cell::RefCell, rc::Rc};

use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;
use rquickjs::{JsLifetime, class::Trace};
use wayland_client::{
    Connection, EventQueue, QueueHandle,
    protocol::{wl_compositor, wl_pointer::WlPointer, wl_seat::WlSeat, wl_shm::WlShm},
};
use wayland_protocols_wlr::layer_shell::v1::client::zwlr_layer_shell_v1::ZwlrLayerShellV1;

use crate::js::JsAgent;

pub struct App {
    pub state: Rc<RefCell<State>>,
    pub agent: JsAgent,

    pub connection: Connection,
    pub event_queue: EventQueue<State>,
    pub qh: QueueHandle<State>,
}

impl App {
    pub fn new() -> Self {
        let connection = Connection::connect_to_env().unwrap();
        let event_queue = connection.new_event_queue::<State>();
        let qh = event_queue.handle();

        let state = Rc::new(RefCell::new(State::new(qh.clone())));
        let agent = JsAgent::new(state.clone());

        Self {
            state,
            agent,
            connection,
            event_queue,
            qh,
        }
    }

    pub fn run(mut self) {
        let display = self.connection.display();

        display.get_registry(&self.qh, ());
        self.event_queue
            .roundtrip(&mut *self.state.borrow_mut())
            .unwrap();

        let mut event_loop: EventLoop<State> = EventLoop::try_new().unwrap();
        WaylandSource::new(self.connection, self.event_queue)
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

    pub qh: QueueHandle<Self>,

    // Wayland globals
    pub wl_compositor: Option<wl_compositor::WlCompositor>,
    pub wl_pointer: Option<WlPointer>,
    pub wl_shm: Option<WlShm>,
    pub wl_seat: Option<WlSeat>,
    pub wlr_layer_shell: Option<ZwlrLayerShellV1>,
}

impl State {
    pub fn new(qh: QueueHandle<Self>) -> Self {
        State {
            running: true,
            render_opportunity: true,
            qh,
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
