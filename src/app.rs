use std::{cell::RefCell, collections::HashMap, rc::Rc, time::Duration};

use calloop::EventLoop;
use calloop_wayland_source::WaylandSource;
use wayland_client::{Connection, EventQueue, QueueHandle};

use crate::{
    bridge::{BridgeCommand, JsBridge, WidgetPatch, WidgetSpec},
    canvas::{CanvasId, CanvasOwner, SharedCanvasStore, Size, WidgetId},
    js::JsAgent,
    task::{Task, TaskQueue, TaskSource},
    wayland::WaylandState,
};

pub struct App {
    // Keep `state` before `agent`: any JS persistent values owned by the app
    // must be dropped before the rquickjs runtime.
    pub state: State,
    pub agent: JsAgent,

    // TODO: connection is platform dependent and should not be here.
    pub connection: Connection,
    pub event_queue: EventQueue<State>,
    pub qh: QueueHandle<State>,
}

impl App {
    pub fn new() -> Self {
        let connection = Connection::connect_to_env().expect("failed to connect to Wayland");
        let event_queue = connection.new_event_queue::<State>();
        let qh = event_queue.handle();

        let canvases: SharedCanvasStore = Rc::new(RefCell::new(crate::canvas::CanvasStore::new()));
        let bridge = Rc::new(RefCell::new(JsBridge::default()));
        let state = State::new(qh.clone(), canvases.clone(), bridge.clone());
        let agent = JsAgent::new(canvases, bridge);

        Self {
            state,
            agent,
            connection,
            event_queue,
            qh,
        }
    }

    pub fn run(mut self, user_code: &str) {
        let display = self.connection.display();

        display.get_registry(&self.qh, ());
        self.event_queue
            .roundtrip(&mut self.state)
            .expect("initial Wayland roundtrip failed");

        if let Err(error) = self.agent.eval_script(user_code) {
            eprintln!("widjet: user script failed: {error}");
            return;
        }
        self.agent.perform_microtask_checkpoint();
        self.state.process_bridge_commands();

        // create event loop
        let mut event_loop: EventLoop<'static, State> = EventLoop::try_new().expect("event loop");
        WaylandSource::new(self.connection, self.event_queue)
            .insert(event_loop.handle())
            .expect("failed to insert Wayland source");

        while self.state.running {
            // 1. Turn deadlines and platform frame opportunities into tasks.
            Self::prepare_tasks(&mut self.state, &self.agent);

            // 2. Run tasks. A microtask checkpoint runs after every task.
            Self::run_tasks(&mut self.state, &self.agent);

            // 3. Update the rendering once at the end of the turn.
            let wants_frame = self.agent.has_animation_frame();
            self.state.update_rendering(wants_frame);
            if wants_frame && !self.state.wayland.has_pending_frame_callback() {
                self.state.wayland.request_frame_callbacks();
            }
            self.agent.perform_microtask_checkpoint();

            // 4. Wait for Wayland or the next timer deadline.
            if !self.state.has_pending_tasks() {
                let timeout = Self::next_timeout(&self.state, &self.agent);
                event_loop
                    .dispatch(timeout, &mut self.state)
                    .expect("event loop dispatch failed");

                // A compositor can stop delivering frame callbacks for a
                // hidden surface. Do not let that freeze rAF forever.
                if !self.state.has_pending_tasks()
                    && self.agent.has_animation_frame()
                    && self.state.wayland.has_pending_frame_callback()
                    && self.agent.now_ms() - self.state.last_animation_frame_ms > 250.0
                {
                    self.state.wayland.reset_frame_callbacks();
                }
            }
        }
    }

    /// Queue work that is already due. This does not call into JS.
    fn prepare_tasks(state: &mut State, agent: &JsAgent) {
        Self::schedule_due_timer(state, agent);

        // A real Wayland frame callback is preferred. If no surface has a
        // frame callback in flight, fall back to a short timer so the first
        // frame (and scripts without a visible surface yet) still runs.
        let can_animate = !state.wayland.has_windows() || state.wayland.has_configured_window();
        if can_animate
            && agent.has_animation_frame()
            && !state.wayland.has_pending_frame_callback()
            && state.animation_frame_due(agent.now_ms())
        {
            state.queue_task(TaskSource::Rendering, Task::AnimationFrame);
        }
    }

    fn schedule_due_timer(state: &mut State, agent: &JsAgent) {
        if state.has_timer_task() {
            return;
        }
        if let Some(deadline) = agent.next_timer_deadline_ms()
            && deadline <= agent.now_ms()
        {
            state.queue_task(TaskSource::Timer, Task::Timer);
        }
    }

    fn run_tasks(state: &mut State, agent: &JsAgent) {
        while let Some(task) = state.pop_task() {
            match task {
                Task::Pointer {
                    widget_id,
                    kind,
                    x,
                    y,
                    button,
                    buttons,
                } => agent.dispatch_pointer_event(widget_id, kind.as_str(), x, y, button, buttons),
                Task::Resize {
                    widget_id,
                    width,
                    height,
                } => agent.dispatch_resize_event(widget_id, width, height),
                Task::AnimationFrame => {
                    let timestamp = agent.now_ms();
                    agent.run_animation_frame(timestamp);
                    state.note_animation_frame(agent.now_ms());
                }
                Task::Timer => {
                    agent.run_next_due_timer();
                }
            }

            agent.perform_microtask_checkpoint();
            state.process_bridge_commands();
            Self::schedule_due_timer(state, agent);
        }
    }

    fn next_timeout(state: &State, agent: &JsAgent) -> Option<Duration> {
        if state.has_pending_tasks() {
            return Some(Duration::ZERO);
        }

        let now = agent.now_ms();
        let mut deadline = agent.next_timer_deadline_ms();

        let can_animate = !state.wayland.has_windows() || state.wayland.has_configured_window();
        if agent.has_animation_frame() {
            if state.wayland.has_pending_frame_callback() {
                // Wait for the compositor. Keep a watchdog so a compositor
                // that stops delivering callbacks cannot freeze the runtime.
                let watchdog = now + 250.0;
                deadline = Some(deadline.map_or(watchdog, |value| value.min(watchdog)));
            } else if can_animate {
                // No platform callback available yet: fall back to a 60 Hz
                // clock instead of spinning.
                let fallback = state.next_animation_frame_ms();
                deadline = Some(deadline.map_or(fallback, |value| value.min(fallback)));
            }
        }

        deadline.map(|deadline| Duration::from_secs_f64(((deadline - now).max(0.0)) / 1000.0))
    }
}

/// The full app state.
pub struct State {
    /// Whether the app should continue running.
    pub running: bool,
    pub widgets: HashMap<WidgetId, Widget>,
    pub canvases: SharedCanvasStore,
    pub bridge: Rc<RefCell<JsBridge>>,
    pub wayland: WaylandState,
    tasks: TaskQueue,
    last_animation_frame_ms: f64,
}

pub struct Widget {
    pub spec: WidgetSpec,
    pub size: Size,
}

impl State {
    pub fn new(
        qh: QueueHandle<Self>,
        canvases: SharedCanvasStore,
        bridge: Rc<RefCell<JsBridge>>,
    ) -> Self {
        Self {
            running: true,
            widgets: HashMap::new(),
            canvases,
            bridge,
            wayland: WaylandState::new(qh),
            tasks: TaskQueue::default(),
            last_animation_frame_ms: f64::NEG_INFINITY,
        }
    }

    pub fn queue_task(&mut self, source: TaskSource, task: Task) {
        self.tasks.push(source, task);
    }

    pub fn pop_task(&mut self) -> Option<Task> {
        self.tasks.pop()
    }

    pub fn has_pending_tasks(&self) -> bool {
        !self.tasks.is_empty()
    }

    pub fn has_timer_task(&self) -> bool {
        self.tasks.has_timer()
    }

    pub fn animation_frame_due(&self, now_ms: f64) -> bool {
        now_ms >= self.last_animation_frame_ms + 16.0
    }

    pub fn next_animation_frame_ms(&self) -> f64 {
        self.last_animation_frame_ms + 16.0
    }

    pub fn note_animation_frame(&mut self, now_ms: f64) {
        self.last_animation_frame_ms = now_ms;
    }

    pub fn process_bridge_commands(&mut self) {
        let commands = self.bridge.borrow().take_commands();
        for command in commands {
            match command {
                BridgeCommand::CreateWidget { widget_id, spec } => {
                    self.wayland.create_widget(widget_id, &spec);
                    self.widgets.insert(
                        widget_id,
                        Widget {
                            size: Size::new(spec.width, spec.height),
                            spec,
                        },
                    );
                }
                BridgeCommand::DestroyWidget { widget_id } => {
                    self.wayland.destroy_widget(widget_id);
                    self.widgets.remove(&widget_id);
                    if self.widgets.is_empty() {
                        self.running = false;
                    }
                }
                BridgeCommand::UpdateWidget { widget_id, patch } => {
                    self.wayland.apply_widget(widget_id, &patch);
                    let Some(widget) = self.widgets.get_mut(&widget_id) else {
                        continue;
                    };
                    match patch {
                        WidgetPatch::SetSize { width, height } => {
                            widget.spec.width = width;
                            widget.spec.height = height;
                            // The canvas bitmap is resized by the JS setter when
                            // a context already exists. Configure events are
                            // handled in `handle_configure`.
                        }
                        WidgetPatch::SetAnchor(anchor) => {
                            widget.spec.anchor = anchor;
                        }
                        WidgetPatch::SetExclusiveZone(zone) => {
                            widget.spec.exclusive_zone = zone;
                        }
                        WidgetPatch::SetMargin {
                            top,
                            right,
                            bottom,
                            left,
                        } => {
                            widget.spec.margin_top = top;
                            widget.spec.margin_right = right;
                            widget.spec.margin_bottom = bottom;
                            widget.spec.margin_left = left;
                        }
                    }
                }
            }
        }
    }

    pub(crate) fn handle_configure(
        &mut self,
        widget_id: WidgetId,
        serial: u32,
        width: u32,
        height: u32,
    ) {
        self.wayland.ack_configure(widget_id, serial);
        self.wayland.mark_configured(widget_id);

        let size = Size::new(width, height);
        let mut changed = false;
        if let Some(widget) = self.widgets.get_mut(&widget_id) {
            changed = widget.size != size;
            widget.size = size;
            widget.spec.width = width;
            widget.spec.height = height;
        }

        let canvas_id = self.canvases.borrow().find_for_widget(widget_id);
        if let Some(canvas_id) = canvas_id
            && self.canvases.borrow().size(canvas_id) != Some(size)
        {
            self.canvases.borrow_mut().recreate(canvas_id, size);
        }

        if changed {
            self.queue_task(
                TaskSource::Rendering,
                Task::Resize {
                    widget_id,
                    width,
                    height,
                },
            );
        }
    }

    pub(crate) fn handle_closed(&mut self, widget_id: WidgetId) {
        let canvas_id = self.canvases.borrow().find_for_widget(widget_id);
        if let Some(canvas_id) = canvas_id {
            self.canvases.borrow_mut().destroy(canvas_id);
        }
        self.wayland.destroy_widget(widget_id);
        self.widgets.remove(&widget_id);
        if self.widgets.is_empty() {
            self.running = false;
        }
    }

    /// Flush canvas commands and present every dirty onscreen/placeholder
    /// canvas exactly once.
    pub fn update_rendering(&mut self, wants_frame: bool) {
        let dirty = self.canvases.borrow().dirty_ids();
        for canvas_id in dirty {
            self.present_canvas(canvas_id, wants_frame);
        }
    }

    fn present_canvas(&mut self, canvas_id: CanvasId, wants_frame: bool) {
        let owner = self.canvases.borrow().owner(canvas_id);
        let Some(snapshot) = self.canvases.borrow_mut().snapshot(canvas_id) else {
            return;
        };

        let widget = match owner {
            Some(CanvasOwner::Onscreen { widget }) => Some(widget),
            Some(CanvasOwner::Offscreen {
                placeholder: Some(widget),
            }) => Some(widget),
            _ => None,
        };

        let presented = match widget {
            Some(widget) => self.wayland.present(widget, &snapshot, wants_frame),
            // Offscreen canvases have no compositor surface of their own.
            None => true,
        };
        if presented {
            self.canvases.borrow_mut().mark_clean(canvas_id);
        }
    }
}
