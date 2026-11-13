//! The single-threaded JS runtime and event loop.

use std::{cell::RefCell, rc::Rc, time::Duration};

use log::error;
use rquickjs::{Context, Function, Runtime};
use thiserror::Error;

use crate::{
    canvas::{CanvasId, CanvasOwner, CanvasStore, Size, WidgetId, WidgetStore},
    js::{self, TimerRegistry},
    platform::{Platform, PlatformError, PlatformEvent},
    task::{Message, MessageQueue, TaskSource},
};

#[derive(Debug, Error)]
pub enum ScriptThreadError {
    #[error("platform event loop failed: {0}")]
    Platform(PlatformError),
    #[error("user script failed: {0}")]
    UserScript(String),
}

impl From<PlatformError> for ScriptThreadError {
    fn from(value: PlatformError) -> Self {
        Self::Platform(value)
    }
}

pub struct ScriptThread<P: Platform> {
    // NOTE: drop order of these three matters
    timers: Rc<RefCell<TimerRegistry>>,
    context: Context,
    runtime: Runtime,

    canvases: Rc<RefCell<CanvasStore>>,
    widgets: Rc<RefCell<WidgetStore>>,

    platform: P,
    messages: MessageQueue,
    running: bool,
    global_last_animation_frame_ms: f64,
    global_animation_frame_task_queued: bool,
}

impl<P: Platform> ScriptThread<P> {
    pub fn new(platform: P) -> Self {
        let canvases = Rc::new(RefCell::new(CanvasStore::new()));
        let widgets = Rc::new(RefCell::new(WidgetStore::new()));

        let runtime = Runtime::new().expect("failed to create JS runtime");
        let context = Context::full(&runtime).expect("failed to create JS context");
        let timers = Rc::new(RefCell::new(TimerRegistry::new()));

        js::install(&context, timers.clone(), canvases.clone(), widgets.clone());
        let global_last_animation_frame_ms = timers.borrow().now_ms();

        Self {
            timers,
            context,
            runtime,
            canvases,
            widgets,
            platform,
            messages: MessageQueue::default(),
            running: true,
            global_last_animation_frame_ms,
            global_animation_frame_task_queued: false,
        }
    }

    pub fn run(mut self, user_code: &str) -> Result<(), ScriptThreadError> {
        // initial eval
        self.eval_script(user_code)
            .map_err(|error| ScriptThreadError::UserScript(error.to_string()))?;
        self.microtask_checkpoint();
        self.reconcile_platform_widgets();

        while self.running {
            // TODO: timer should be changed to poll till next deadline,
            // rather than expire check
            self.schedule_due_timer();

            if let Some(message) = self.messages.pop() {
                self.handle_message(message);
                self.microtask_checkpoint();
                self.reconcile_platform_widgets();
                self.schedule_due_timer();
            }

            self.update_rendering();
            if !self.running {
                break;
            }
            self.prepare_animation_frame_tasks();

            // TODO: the platform event gathering should be put at beginning of the loop
            let timeout = match self.messages.is_empty() {
                true => self.next_timeout(),
                false => Default::default(),
            };
            let events = self.platform.dispatch(timeout)?;
            self.queue_platform_events(events);

            // TODO: watchdog should be removed, we use platform vsync event
            self.check_frame_watchdog();
        }

        Ok(())
    }

    pub(crate) fn eval_script(&self, code: &str) -> rquickjs::Result<()> {
        self.context.with(|ctx| {
            ctx.eval::<rquickjs::Value, _>(code.as_bytes())?;
            Ok(())
        })
    }

    pub(crate) fn now_ms(&self) -> f64 {
        self.timers.borrow().now_ms()
    }

    pub(crate) fn next_timer_deadline_ms(&self) -> Option<f64> {
        self.timers.borrow().next_deadline_ms()
    }

    pub(crate) fn run_next_due_timer(&self) -> bool {
        let now = self.now_ms();
        let Some(timer) = self.timers.borrow_mut().take_due(now) else {
            return false;
        };

        let result = self.context.with(|ctx| {
            let callback = timer.callback.restore(&ctx)?;
            callback.call::<_, ()>(())
        });
        if let Err(error) = result {
            error!("widjet: timer callback failed: {error}");
        }
        true
    }

    pub(crate) fn microtask_checkpoint(&self) {
        loop {
            match self.runtime.execute_pending_job() {
                // if there is no more micro task, break
                Ok(false) => break,
                // print error message
                Err(exception) => {
                    let message = exception.0.with(|ctx| {
                        let value = ctx.catch();
                        if let Some(object) = value.as_object() {
                            if let Ok(stack) = object.get::<_, String>("stack") {
                                return stack;
                            }
                            if let Ok(message) = object.get::<_, String>("message") {
                                return message;
                            }
                        }
                        value
                            .get::<String>()
                            .unwrap_or_else(|_| "unhandled exception".to_string())
                    });
                    error!("widjet: unhandled exception in microtask: {message}");
                }
                _ => {}
            }
        }
    }

    pub(crate) fn has_animation_frame(&self, timeline_id: WidgetId) -> bool {
        self.context
            .with(|ctx| {
                let function: Function = ctx.globals().get("__widjet_has_frames")?;
                function.call::<_, bool>((timeline_id,))
            })
            .unwrap_or(false)
    }

    pub(crate) fn run_animation_frame(&self, timeline_id: WidgetId, timestamp_ms: f64) {
        let result = self.context.with(|ctx| {
            let function: Function = ctx.globals().get("__widjet_frame")?;
            function.call::<_, u32>((timeline_id, timestamp_ms))?;
            Ok::<_, rquickjs::Error>(())
        });
        if let Err(error) = result {
            error!("widjet: animation frame failed: {error}");
        }
    }

    fn forget_widget(&self, widget_id: WidgetId) {
        let result = self.context.with(|ctx| {
            let function: Function = ctx.globals().get("__widjet_forget_widget")?;
            function.call::<_, ()>((widget_id,))?;
            Ok::<_, rquickjs::Error>(())
        });
        if let Err(error) = result {
            error!("widjet: failed to forget widget {widget_id}: {error}");
        }
    }

    pub(crate) fn dispatch_pointer_event(
        &self,
        widget_id: WidgetId,
        kind: &str,
        x: f64,
        y: f64,
        button: u32,
        buttons: u32,
    ) {
        self.dispatch_event(widget_id, kind, x, y, button, buttons, 0, 0);
    }

    pub(crate) fn dispatch_resize_event(&self, widget_id: WidgetId, width: u32, height: u32) {
        self.dispatch_event(widget_id, "resize", 0.0, 0.0, 0, 0, width, height);
    }

    #[allow(clippy::too_many_arguments)]
    fn dispatch_event(
        &self,
        widget_id: WidgetId,
        kind: &str,
        x: f64,
        y: f64,
        button: u32,
        buttons: u32,
        width: u32,
        height: u32,
    ) {
        let result = self.context.with(|ctx| {
            let function: Function = ctx.globals().get("__widjet_dispatch")?;
            let params = vec![
                x,
                y,
                f64::from(button),
                f64::from(buttons),
                f64::from(width),
                f64::from(height),
            ];
            function.call::<_, bool>((widget_id, kind.to_string(), params))?;
            Ok::<_, rquickjs::Error>(())
        });
        if let Err(error) = result {
            error!("widjet: event dispatch failed: {error}");
        }
    }

    /// Sync the widget store state to platform inner.
    fn reconcile_platform_widgets(&mut self) {
        let entries = self.widgets.borrow().snapshots();
        for (widget_id, state) in entries {
            if state.removed {
                self.remove_widget(widget_id);
                continue;
            }
            if !state.materialized {
                continue;
            }

            match state.applied_revision {
                None => {
                    self.platform.create_widget(widget_id, &state.spec);
                    let now = self.now_ms();
                    self.widgets
                        .borrow_mut()
                        .update_runtime(widget_id, |entry| {
                            entry.applied_revision = Some(state.revision);
                            entry.last_animation_frame_ms.get_or_insert(now);
                        });
                }
                Some(applied_revision) if applied_revision != state.revision => {
                    self.platform.update_widget(widget_id, &state.spec);
                    self.widgets
                        .borrow_mut()
                        .update_runtime(widget_id, |entry| {
                            entry.applied_revision = Some(state.revision);
                        });
                }
                Some(_) => {}
            }
        }
    }

    fn remove_widget(&mut self, widget_id: WidgetId) {
        let had_native_surface = self
            .widgets
            .borrow()
            .get(widget_id)
            .is_some_and(|state| state.applied_revision.is_some());
        if had_native_surface {
            // A compositor Closed event may already have destroyed the
            // surface. Backends must make this operation idempotent.
            self.platform.destroy_widget(widget_id);
        }
        self.messages.remove_widget_tasks(widget_id);
        self.forget_widget(widget_id);
        self.widgets.borrow_mut().remove(widget_id);
    }

    fn prepare_animation_frame_tasks(&mut self) {
        let now = self.now_ms();

        // Global rAF remains a timer-clock compatibility path. Surface-bound
        // callbacks are driven independently by each native frame callback.
        if self.has_animation_frame(0) && now >= self.global_last_animation_frame_ms + 16.0 {
            self.queue_animation_frame(0);
        }

        let entries = self.widgets.borrow().snapshots();
        for (widget_id, state) in entries {
            if state.removed || !state.materialized || !self.has_animation_frame(widget_id) {
                continue;
            }
            if self.platform.is_widget_configured(widget_id)
                && !self.platform.has_pending_frame_callback(widget_id)
                && self.animation_frame_due(state, now)
            {
                self.queue_animation_frame(widget_id);
            }
        }
    }

    fn animation_frame_due(&self, state: crate::canvas::WidgetState, now_ms: f64) -> bool {
        now_ms >= state.last_animation_frame_ms.unwrap_or(now_ms) + 16.0
    }

    fn next_animation_frame_ms(&self, state: crate::canvas::WidgetState, now_ms: f64) -> f64 {
        state.last_animation_frame_ms.unwrap_or(now_ms) + 16.0
    }

    fn queue_animation_frame(&mut self, widget_id: WidgetId) {
        if widget_id == 0 {
            if self.global_animation_frame_task_queued {
                return;
            }
            self.global_animation_frame_task_queued = true;
        } else {
            let state = { self.widgets.borrow().get(widget_id) };
            let Some(state) = state else {
                return;
            };
            if state.removed || !state.materialized || state.animation_frame_task_queued {
                return;
            }
            self.widgets
                .borrow_mut()
                .update_runtime(widget_id, |state| {
                    state.animation_frame_task_queued = true;
                });
        }
        self.messages
            .push(TaskSource::Rendering, Message::AnimationFrame { widget_id });
    }

    /// Check if there are timer due.
    fn schedule_due_timer(&mut self) {
        if self.messages.has_timer() {
            return;
        }
        if self
            .next_timer_deadline_ms()
            .is_some_and(|deadline| deadline <= self.now_ms())
        {
            self.messages.push(TaskSource::Timer, Message::Timer);
        }
    }

    /// Handle an external message like platform message or timer message.
    /// For most case, this is equivalent to a macro task.
    fn handle_message(&mut self, task: Message) {
        match task {
            Message::Pointer {
                widget_id,
                kind,
                x,
                y,
                button,
                buttons,
            } => self.dispatch_pointer_event(widget_id, kind.as_str(), x, y, button, buttons),
            Message::Configure {
                widget_id,
                width,
                height,
            } => self.dispatch_resize_event(widget_id, width, height),
            Message::Close { widget_id } => self.close_widget(widget_id),
            Message::AnimationFrame { widget_id } => {
                if widget_id == 0 {
                    self.global_animation_frame_task_queued = false;
                    let now = self.now_ms();
                    self.run_animation_frame(widget_id, now);
                    self.global_last_animation_frame_ms = now;
                } else {
                    let active = self.widgets.borrow().is_active(widget_id);
                    self.widgets
                        .borrow_mut()
                        .update_runtime(widget_id, |state| {
                            state.animation_frame_task_queued = false;
                        });
                    if active {
                        self.widgets
                            .borrow_mut()
                            .update_runtime(widget_id, |state| {
                                state.frame_callback_since_ms = None;
                                state.watchdog_fallback = false;
                            });
                        let now = self.now_ms();
                        self.run_animation_frame(widget_id, now);
                        let frame_end = self.now_ms();
                        self.widgets
                            .borrow_mut()
                            .update_runtime(widget_id, |state| {
                                state.last_animation_frame_ms = Some(frame_end);
                            });
                    }
                }
            }
            Message::Timer => {
                self.run_next_due_timer();
            }
        }
    }

    fn queue_platform_events(&mut self, events: Vec<PlatformEvent>) {
        for event in events {
            match event {
                PlatformEvent::Pointer {
                    widget_id,
                    kind,
                    x,
                    y,
                    button,
                    buttons,
                } => self.messages.push(
                    TaskSource::UserInteraction,
                    Message::Pointer {
                        widget_id,
                        kind,
                        x,
                        y,
                        button,
                        buttons,
                    },
                ),
                PlatformEvent::Resized {
                    widget_id,
                    width,
                    height,
                } => self.handle_configure(widget_id, width, height),
                PlatformEvent::Closed { widget_id } => self
                    .messages
                    .push(TaskSource::UserInteraction, Message::Close { widget_id }),
                PlatformEvent::FrameReady { widget_id } => {
                    self.queue_animation_frame(widget_id);
                }
            }
        }
    }

    fn handle_configure(&mut self, widget_id: WidgetId, width: u32, height: u32) {
        let size = Size::new(width, height);
        let state = { self.widgets.borrow().get(widget_id) };
        let Some(state) = state else {
            return;
        };
        if state.removed {
            return;
        }
        let changed = state.size != size;
        self.widgets
            .borrow_mut()
            .update_runtime(widget_id, |state| state.size = size);

        let canvas_id = self.canvases.borrow().find_for_widget(widget_id);
        if let Some(canvas_id) = canvas_id {
            let needs_recreate = self.canvases.borrow().size(canvas_id) != Some(size);
            if needs_recreate {
                self.canvases.borrow_mut().recreate(canvas_id, size);
            }
        }

        if changed {
            self.messages.push(
                TaskSource::Rendering,
                Message::Configure {
                    widget_id,
                    width,
                    height,
                },
            );
        }
    }

    fn close_widget(&mut self, widget_id: WidgetId) {
        let canvas_id = self.canvases.borrow().find_for_widget(widget_id);
        if let Some(canvas_id) = canvas_id {
            self.canvases.borrow_mut().destroy(canvas_id);
        }
        self.widgets.borrow_mut().mark_removed(widget_id);
        self.remove_widget(widget_id);
    }

    fn update_rendering(&mut self) {
        let dirty = self.canvases.borrow().dirty_ids();
        for canvas_id in dirty {
            self.present_canvas(canvas_id);
        }

        let entries = self.widgets.borrow().snapshots();
        let now = self.now_ms();
        for (widget_id, state) in entries {
            if state.removed || !state.materialized {
                continue;
            }
            if !self.has_animation_frame(widget_id) {
                if self.platform.has_pending_frame_callback(widget_id) {
                    self.platform.reset_frame_callback(widget_id);
                }
                self.widgets
                    .borrow_mut()
                    .update_runtime(widget_id, |state| {
                        state.frame_callback_since_ms = None;
                        state.watchdog_fallback = false;
                    });
                continue;
            }

            if state.watchdog_fallback || !self.platform.is_widget_configured(widget_id) {
                continue;
            }

            if !self.platform.has_pending_frame_callback(widget_id) {
                self.platform.request_frame_callback(widget_id);
            }
            let pending = self.platform.has_pending_frame_callback(widget_id);
            self.widgets
                .borrow_mut()
                .update_runtime(widget_id, |state| {
                    state.frame_callback_since_ms = if pending {
                        Some(state.frame_callback_since_ms.unwrap_or(now))
                    } else {
                        None
                    };
                });
        }
    }

    fn present_canvas(&mut self, canvas_id: CanvasId) {
        let (owner, snapshot) = {
            let mut canvases = self.canvases.borrow_mut();
            (canvases.owner(canvas_id), canvases.snapshot(canvas_id))
        };
        let Some(snapshot) = snapshot else {
            return;
        };

        let widget_id = match owner {
            Some(CanvasOwner::Onscreen { widget }) => Some(widget),
            Some(CanvasOwner::Offscreen {
                placeholder: Some(widget),
            }) => Some(widget),
            _ => None,
        };
        let presented = match widget_id {
            Some(widget_id) => {
                let wants_frame = self.has_animation_frame(widget_id);
                self.platform.present(widget_id, &snapshot, wants_frame)
            }
            None => true,
        };
        if presented {
            self.canvases.borrow_mut().mark_clean(canvas_id);
        }
    }

    fn next_timeout(&self) -> Option<Duration> {
        let now = self.now_ms();
        let mut deadline = self.next_timer_deadline_ms();

        if self.has_animation_frame(0) {
            let fallback = self.global_last_animation_frame_ms + 16.0;
            deadline = Some(deadline.map_or(fallback, |value| value.min(fallback)));
        }

        for (widget_id, state) in self.widgets.borrow().snapshots() {
            if state.removed || !state.materialized || !self.has_animation_frame(widget_id) {
                continue;
            }
            let frame_deadline = if self.platform.has_pending_frame_callback(widget_id) {
                state.frame_callback_since_ms.unwrap_or(now) + 250.0
            } else if self.platform.is_widget_configured(widget_id) {
                self.next_animation_frame_ms(state, now)
            } else {
                continue;
            };
            deadline = Some(deadline.map_or(frame_deadline, |value| value.min(frame_deadline)));
        }

        deadline.map(|deadline| Duration::from_secs_f64(((deadline - now).max(0.0)) / 1000.0))
    }

    fn check_frame_watchdog(&mut self) {
        let now = self.now_ms();
        let entries = self.widgets.borrow().snapshots();
        for (widget_id, state) in entries {
            if state.removed
                || !state.materialized
                || !self.has_animation_frame(widget_id)
                || !self.platform.has_pending_frame_callback(widget_id)
            {
                continue;
            }
            let requested_at = state.frame_callback_since_ms.unwrap_or(now);
            if now - requested_at >= 250.0 {
                self.platform.reset_frame_callback(widget_id);
                self.widgets
                    .borrow_mut()
                    .update_runtime(widget_id, |state| {
                        state.frame_callback_since_ms = None;
                        state.watchdog_fallback = true;
                    });
                self.queue_animation_frame(widget_id);
            }
        }
    }
}
