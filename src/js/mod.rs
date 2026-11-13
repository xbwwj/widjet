mod context_2d;
mod image_bitmap;
mod image_data;
mod offscreen_canvas;
mod path;
mod text_metrics;
mod timers;
mod widget_canvas;

use std::{cell::RefCell, rc::Rc};

use rquickjs::{Class, Context, Function, JsLifetime, Persistent, Runtime, class::Trace};

use crate::{
    bridge::JsBridge,
    canvas::{SharedCanvasStore, WidgetId},
};

const RUNTIME_JS: &str = include_str!("runtime.js");

pub use context_2d::CanvasRenderingContext2D;
pub use image_bitmap::ImageBitmap;
pub use image_data::ImageData;
pub use offscreen_canvas::OffscreenCanvas;
pub use text_metrics::TextMetrics;
pub use widget_canvas::WidgetCanvas;

/// Data stored in the rquickjs context. Only the narrow canvas subsystem and
/// the bridge are exposed to JS; `State` and platform objects are not.
#[derive(JsLifetime)]
pub struct JsRuntimeData {
    pub canvases: SharedCanvasStore,
    pub bridge: Rc<RefCell<JsBridge>>,
}

impl<'js> Trace<'js> for JsRuntimeData {
    fn trace<'a>(&self, _tracer: rquickjs::class::Tracer<'a, 'js>) {}
}

pub struct JsAgent {
    // Timer callbacks are persistent JS values, so they must be dropped before
    // the runtime.
    timers: Rc<RefCell<timers::TimerRegistry>>,
    // Context must be dropped before Runtime.
    ctx: Context,
    _rt: Runtime,
}

impl JsAgent {
    pub fn new(canvases: SharedCanvasStore, bridge: Rc<RefCell<JsBridge>>) -> Self {
        let rt = Runtime::new().expect("failed to create JS runtime");
        let ctx = Context::full(&rt).expect("failed to create JS context");
        let timer_registry = Rc::new(RefCell::new(timers::TimerRegistry::new()));
        ctx.with(|ctx| {
            ctx.store_userdata(JsRuntimeData { canvases, bridge })
                .expect("failed to store JS runtime data");
            let global = ctx.globals();
            Class::<WidgetCanvas>::define(&global).expect("failed to define WidgetCanvas");
            Class::<OffscreenCanvas>::define(&global).expect("failed to define OffscreenCanvas");
            Class::<CanvasRenderingContext2D>::define(&global)
                .expect("failed to define CanvasRenderingContext2D");
            Class::<ImageBitmap>::define(&global).expect("failed to define ImageBitmap");
            Class::<ImageData>::define(&global).expect("failed to define ImageData");
            Class::<TextMetrics>::define(&global).expect("failed to define TextMetrics");

            let log = Function::new(ctx.clone(), |message: String| {
                println!("[js] {message}");
            })
            .expect("failed to create console bridge");
            global
                .set("__widjet_log", log)
                .expect("failed to install console bridge");

            let set_timeout = {
                let timers = timer_registry.clone();
                Function::new(
                    ctx.clone(),
                    move |callback: Function,
                          delay_ms: f64,
                          repeat: bool|
                          -> rquickjs::Result<u32> {
                        let callback_ctx = callback.ctx().clone();
                        let callback = Persistent::save(&callback_ctx, callback);
                        Ok(timers.borrow_mut().set(callback, delay_ms, repeat))
                    },
                )
                .expect("failed to create setTimeout bridge")
            };
            global
                .set("__widjet_set_timeout", set_timeout)
                .expect("failed to install setTimeout bridge");

            let clear_timeout = {
                let timers = timer_registry.clone();
                Function::new(ctx.clone(), move |id: u32| -> bool {
                    timers.borrow_mut().clear(id)
                })
                .expect("failed to create clearTimeout bridge")
            };
            global
                .set("__widjet_clear_timeout", clear_timeout)
                .expect("failed to install clearTimeout bridge");

            let now = {
                let timers = timer_registry.clone();
                Function::new(ctx.clone(), move || -> f64 { timers.borrow().now_ms() })
                    .expect("failed to create performance.now bridge")
            };
            global
                .set("__widjet_now", now)
                .expect("failed to install performance.now bridge");

            ctx.eval::<rquickjs::Value, _>(RUNTIME_JS.as_bytes())
                .expect("failed to initialize JS runtime");
        });

        Self {
            timers: timer_registry,
            ctx,
            _rt: rt,
        }
    }

    #[allow(dead_code)]
    pub fn eval<T>(&self, code: &str) -> rquickjs::Result<T>
    where
        T: for<'js> rquickjs::FromJs<'js>,
    {
        self.ctx.with(|ctx| ctx.eval::<T, _>(code.as_bytes()))
    }

    /// Evaluate a user script, discarding the completion value.
    pub fn eval_script(&self, code: &str) -> rquickjs::Result<()> {
        self.ctx.with(|ctx| {
            ctx.eval::<rquickjs::Value, _>(code.as_bytes())?;
            Ok(())
        })
    }

    pub fn now_ms(&self) -> f64 {
        self.timers.borrow().now_ms()
    }

    pub fn next_timer_deadline_ms(&self) -> Option<f64> {
        self.timers.borrow().next_deadline_ms()
    }

    /// Run at most one due timer. The event loop calls this from a
    /// `TaskSource::Timer` task.
    pub fn run_next_due_timer(&self) -> bool {
        let now = self.timers.borrow().now_ms();
        let Some(timer) = self.timers.borrow_mut().take_due(now) else {
            return false;
        };

        let result = self.ctx.with(|ctx| {
            let callback = timer.callback.restore(&ctx)?;
            callback.call::<_, ()>(())
        });
        if let Err(error) = result {
            eprintln!("widjet: timer callback failed: {error}");
        }
        true
    }

    /// Drain the QuickJS job queue (Promise reactions, `queueMicrotask`, ...).
    pub fn perform_microtask_checkpoint(&self) {
        loop {
            match self._rt.execute_pending_job() {
                Ok(true) => {}
                Ok(false) => break,
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
                    eprintln!("widjet: unhandled exception in microtask: {message}");
                }
            }
        }
    }

    /// Whether the user script is waiting for another animation frame.
    pub fn has_animation_frame(&self) -> bool {
        self.ctx
            .with(|ctx| {
                let function: Function = ctx.globals().get("__widjet_has_frames")?;
                function.call::<_, bool>(())
            })
            .unwrap_or(false)
    }

    /// Run all pending `requestAnimationFrame` callbacks.
    pub fn run_animation_frame(&self, timestamp_ms: f64) {
        let result = self.ctx.with(|ctx| {
            let function: Function = ctx.globals().get("__widjet_frame")?;
            function.call::<_, u32>((timestamp_ms,))?;
            Ok::<_, rquickjs::Error>(())
        });

        if let Err(error) = result {
            eprintln!("widjet: animation frame failed: {error}");
        }
    }

    /// Dispatch a pointer event to a widget's JS listeners.
    #[allow(clippy::too_many_arguments)]
    pub fn dispatch_pointer_event(
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

    /// Notify a widget that the compositor assigned it a new size.
    pub fn dispatch_resize_event(&self, widget_id: WidgetId, width: u32, height: u32) {
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
        let result = self.ctx.with(|ctx| {
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
            eprintln!("widjet: event dispatch failed: {error}");
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use crate::{bridge::JsBridge, canvas::CanvasStore};

    use super::JsAgent;

    fn agent() -> JsAgent {
        JsAgent::new(
            Rc::new(RefCell::new(CanvasStore::new())),
            Rc::new(RefCell::new(JsBridge::default())),
        )
    }

    #[test]
    fn offscreen_canvas_fill_and_transfer() {
        let agent = agent();
        agent
            .eval::<()>(
                r##"
                globalThis.canvas = new OffscreenCanvas(8, 8);
                globalThis.ctx = globalThis.canvas.getContext("2d");
                globalThis.ctx.fillStyle = "#ff0000";
                globalThis.ctx.fillRect(0, 0, 4, 4);
                undefined;
                "##,
            )
            .unwrap();

        agent
            .eval::<()>(
                r##"
                globalThis.before = globalThis.ctx.getImageData(0, 0, 1, 1);
                globalThis.red = globalThis.before.data[0];
                globalThis.alpha = globalThis.before.data[3];
                globalThis.bitmap = globalThis.canvas.transferToImageBitmap();
                globalThis.bitmapWidth = globalThis.bitmap.width;
                globalThis.afterAlpha = globalThis.ctx.getImageData(0, 0, 1, 1).data[3];
                undefined;
                "##,
            )
            .unwrap();

        assert_eq!(agent.eval::<u8>("globalThis.red").unwrap(), 255);
        assert_eq!(agent.eval::<u8>("globalThis.alpha").unwrap(), 255);
        assert_eq!(agent.eval::<u32>("globalThis.bitmapWidth").unwrap(), 8);
        assert_eq!(agent.eval::<u8>("globalThis.afterAlpha").unwrap(), 0);
    }

    #[test]
    fn path_api_draws_rect_and_arc() {
        let agent = agent();
        agent
            .eval::<()>(
                r##"
                globalThis.pathCanvas = new OffscreenCanvas(32, 32);
                globalThis.pathCtx = globalThis.pathCanvas.getContext("2d");
                globalThis.pathCtx.fillStyle = "red";

                globalThis.pathCtx.beginPath();
                globalThis.pathCtx.rect(0, 0, 8, 8);
                globalThis.pathCtx.fill();

                globalThis.pathCtx.beginPath();
                globalThis.pathCtx.arc(16, 16, 8, 0, Math.PI * 2, false);
                globalThis.pathCtx.fill();

                globalThis.pathCtx.beginPath();
                globalThis.pathCtx.quadraticCurveTo(1, 1, 2, 2);
                globalThis.pathCtx.bezierCurveTo(3, 3, 4, 4, 5, 5);
                globalThis.pathCtx.roundRect(20, 20, 8, 8, 2);
                undefined;
                "##,
            )
            .unwrap();

        agent
            .eval::<()>(
                r##"
                globalThis.rectPixel = globalThis.pathCtx.getImageData(2, 2, 1, 1).data[0];
                globalThis.arcPixel = globalThis.pathCtx.getImageData(16, 16, 1, 1).data[0];
                undefined;
                "##,
            )
            .unwrap();

        assert_eq!(agent.eval::<u8>("globalThis.rectPixel").unwrap(), 255);
        assert_eq!(agent.eval::<u8>("globalThis.arcPixel").unwrap(), 255);
    }

    #[test]
    fn text_api_reports_metrics_and_does_not_throw() {
        let agent = agent();
        agent
            .eval::<()>(
                r##"
                globalThis.textCanvas = new OffscreenCanvas(64, 32);
                globalThis.textCtx = globalThis.textCanvas.getContext("2d");
                globalThis.textCtx.font = "16px sans-serif";
                globalThis.textCtx.fillStyle = "green";
                globalThis.textCtx.fillText("A", 4, 20);

                globalThis.metrics = globalThis.textCtx.measureText("A");
                globalThis.textWidth = globalThis.metrics.width;
                globalThis.textAscent = globalThis.metrics.fontBoundingBoxAscent;
                undefined;
                "##,
            )
            .unwrap();

        assert!(agent.eval::<f64>("globalThis.textWidth").unwrap() >= 0.0);
        assert!(agent.eval::<f64>("globalThis.textAscent").unwrap() >= 0.0);
    }

    #[test]
    fn widget_canvas_uses_deferred_bridge_and_owns_a_canvas() {
        use crate::bridge::BridgeCommand;

        let canvases = Rc::new(RefCell::new(CanvasStore::new()));
        let bridge = Rc::new(RefCell::new(JsBridge::default()));
        let agent = JsAgent::new(canvases.clone(), bridge.clone());

        agent
            .eval::<()>(
                r##"
                globalThis.widget = new WidgetCanvas();
                globalThis.widget.width = 10;
                globalThis.widget.height = 20;
                globalThis.widget.anchor = 1;
                globalThis.widget.marginTop = 40;
                globalThis.widget.exclusiveZone = 4;
                globalThis.widgetCtx = globalThis.widget.getContext("2d");
                globalThis.widgetCtx.fillStyle = "green";
                globalThis.widgetCtx.fillRect(0, 0, 1, 1);
                undefined;
                "##,
            )
            .unwrap();

        let commands = bridge.borrow().take_commands();
        assert!(matches!(
            commands.as_slice(),
            [BridgeCommand::CreateWidget { spec, .. }]
                if spec.width == 10
                    && spec.height == 20
                    && spec.anchor == 1
                    && spec.margin_top == 40
                    && spec.exclusive_zone == 4
        ));

        let canvas_id = canvases.borrow().find_for_widget(1).unwrap();
        assert_eq!(canvases.borrow().size(canvas_id).unwrap().width, 10);
        assert_eq!(canvases.borrow().size(canvas_id).unwrap().height, 20);

        let snapshot = canvases.borrow_mut().snapshot(canvas_id).unwrap();
        assert_eq!(snapshot.pixel(0, 0), Some([0, 128, 0, 255]));
    }

    #[test]
    fn draw_image_from_offscreen_and_bitmap() {
        let agent = agent();
        agent
            .eval::<()>(
                r##"
                globalThis.source = new OffscreenCanvas(4, 4);
                globalThis.sourceCtx = globalThis.source.getContext("2d");
                globalThis.sourceCtx.fillStyle = "blue";
                globalThis.sourceCtx.fillRect(0, 0, 4, 4);
                undefined;
                "##,
            )
            .unwrap();
        agent
            .eval::<()>(
                r##"
                globalThis.destination = new OffscreenCanvas(4, 4);
                globalThis.destinationCtx = globalThis.destination.getContext("2d");
                globalThis.destinationCtx.drawImage(globalThis.source, 0, 0);
                undefined;
                "##,
            )
            .unwrap();
        agent
            .eval::<()>(
                r##"
                globalThis.bitmap = globalThis.source.transferToImageBitmap();
                globalThis.bitmapDestination = new OffscreenCanvas(4, 4);
                globalThis.bitmapDestinationCtx = globalThis.bitmapDestination.getContext("2d");
                globalThis.bitmapDestinationCtx.drawImage(globalThis.bitmap, 0, 0);
                undefined;
                "##,
            )
            .unwrap();
        agent
            .eval::<()>(
                r##"
                globalThis.fromCanvas = globalThis.destinationCtx.getImageData(0, 0, 1, 1).data[2];
                globalThis.fromBitmap = globalThis.bitmapDestinationCtx.getImageData(0, 0, 1, 1).data[2];
                undefined;
                "##,
            )
            .unwrap();

        assert_eq!(agent.eval::<u8>("globalThis.fromCanvas").unwrap(), 255);
        assert_eq!(agent.eval::<u8>("globalThis.fromBitmap").unwrap(), 255);
    }

    #[test]
    fn runtime_dispatches_events_and_animation_frames() {
        let agent = agent();
        agent
            .eval_script(
                r##"
                globalThis.events = [];
                globalThis.widget = new WidgetCanvas();
                globalThis.widget.width = 100;
                globalThis.widget.height = 50;
                globalThis.widget.onclick = (event) => {
                  events.push(["onclick", event.x, event.y, event.button]);
                };
                globalThis.widget.addEventListener("contextmenu", (event) => {
                  events.push(["contextmenu", event.x, event.y, event.button]);
                });
                globalThis.frames = 0;
                requestAnimationFrame((timestamp) => {
                  frames += 1;
                  globalThis.lastFrame = timestamp;
                });
                undefined;
                "##,
            )
            .unwrap();

        assert!(agent.has_animation_frame());
        agent.run_animation_frame(16.0);
        assert!(!agent.has_animation_frame());
        assert_eq!(agent.eval::<u32>("globalThis.frames").unwrap(), 1);
        assert_eq!(agent.eval::<f64>("globalThis.lastFrame").unwrap(), 16.0);

        agent.dispatch_pointer_event(1, "click", 12.5, 7.0, 0, 0);
        agent.dispatch_pointer_event(1, "contextmenu", 20.0, 30.0, 2, 2);
        agent.dispatch_resize_event(1, 120, 60);

        assert_eq!(agent.eval::<u32>("globalThis.events.length").unwrap(), 2);
        assert_eq!(
            agent.eval::<String>("globalThis.events[0][0]").unwrap(),
            "onclick"
        );
        assert_eq!(agent.eval::<f64>("globalThis.events[0][1]").unwrap(), 12.5);
        assert_eq!(
            agent.eval::<String>("globalThis.events[1][0]").unwrap(),
            "contextmenu"
        );
        assert_eq!(agent.eval::<u32>("globalThis.events[1][3]").unwrap(), 2);
        assert_eq!(agent.eval::<u32>("globalThis.widget.width").unwrap(), 120);
        assert_eq!(agent.eval::<u32>("globalThis.widget.height").unwrap(), 60);
    }

    #[test]
    fn bundled_user_script_runs() {
        let agent = agent();
        agent
            .eval_script(include_str!("../../user.js"))
            .expect("user.js should run");
        assert!(agent.has_animation_frame());

        agent.run_animation_frame(16.0);
        agent.run_animation_frame(600.0);
        agent.dispatch_pointer_event(2, "click", 10.0, 20.0, 0, 0);
        agent.dispatch_pointer_event(2, "contextmenu", 10.0, 20.0, 2, 2);
        assert!(agent.has_animation_frame());
    }

    #[test]
    fn every_new_widget_canvas_is_independent() {
        let agent = agent();
        agent
            .eval_script(
                r##"
                globalThis.clicks = [];
                globalThis.widgets = [];
                for (let i = 0; i < 3; i += 1) {
                  const widget = new WidgetCanvas();
                  widget.width = 20;
                  widget.height = 10;
                  widget.getContext("2d").fillRect(0, 0, 1, 1);
                  widget.onclick = () => clicks.push(widget.widgetId);
                  widgets.push(widget);
                }
                undefined;
                "##,
            )
            .unwrap();

        for widget_id in 1..=3 {
            agent.dispatch_pointer_event(widget_id, "click", 1.0, 2.0, 0, 0);
        }

        assert_eq!(
            agent.eval::<String>("JSON.stringify(clicks)").unwrap(),
            "[1,2,3]"
        );
    }

    #[test]
    fn timers_and_microtask_checkpoints() {
        let agent = agent();
        agent
            .eval_script(
                r##"
                globalThis.order = [];
                Promise.resolve().then(() => order.push("promise"));
                queueMicrotask(() => order.push("microtask"));
                setTimeout(() => {
                  order.push("timeout");
                  Promise.resolve().then(() => order.push("timeout-microtask"));
                }, 0);
                undefined;
                "##,
            )
            .unwrap();

        assert_eq!(agent.eval::<u32>("globalThis.order.length").unwrap(), 0);
        agent.perform_microtask_checkpoint();
        assert_eq!(
            agent
                .eval::<String>("JSON.stringify(globalThis.order)")
                .unwrap(),
            r#"["promise","microtask"]"#
        );

        assert!(agent.next_timer_deadline_ms().is_some());
        assert!(agent.run_next_due_timer());
        agent.perform_microtask_checkpoint();
        assert_eq!(
            agent
                .eval::<String>("JSON.stringify(globalThis.order)")
                .unwrap(),
            r#"["promise","microtask","timeout","timeout-microtask"]"#
        );

        agent
            .eval_script(
                r##"
                globalThis.intervalCount = 0;
                const interval = setInterval(() => {
                  intervalCount += 1;
                  clearInterval(interval);
                }, 0);
                undefined;
                "##,
            )
            .unwrap();
        assert!(agent.run_next_due_timer());
        assert_eq!(agent.eval::<u32>("globalThis.intervalCount").unwrap(), 1);
        assert!(agent.next_timer_deadline_ms().is_none());
    }

    #[test]
    fn user2_collision_ends_and_restart_resumes_the_game() {
        let agent = agent();
        agent
            .eval_script(include_str!("../../user2.js"))
            .expect("user2.js should run");

        agent
            .eval_script(
                r##"
                invulnerable = 0;
                lastTimestamp = 0;
                player.x = 200;
                player.y = 400;
                player.targetX = 200;
                player.targetY = 400;
                bullets = [{ x: 200, y: 400, vx: 0, vy: 0, r: 6 }];
                undefined;
                "##,
            )
            .unwrap();

        agent.run_animation_frame(0.0);
        assert!(agent.eval::<bool>("gameOver").unwrap());
        assert!(
            agent.eval::<f64>("bullets.length").unwrap() >= 1.0,
            "the collision bullet should still be in the frozen field"
        );

        agent.dispatch_pointer_event(1, "pointerdown", 270.0, 700.0, 0, 0);
        assert!(!agent.eval::<bool>("gameOver").unwrap());
        assert_eq!(agent.eval::<f64>("bullets.length").unwrap(), 0.0);

        agent.eval_script("gameOver = true; undefined;").unwrap();
        let exit_x = agent
            .eval::<f64>("EXIT_BUTTON.x + EXIT_BUTTON.w / 2")
            .unwrap();
        let exit_y = agent
            .eval::<f64>("EXIT_BUTTON.y + EXIT_BUTTON.h / 2")
            .unwrap();
        agent.dispatch_pointer_event(1, "pointerdown", exit_x, exit_y, 0, 0);
        assert!(agent.eval::<bool>("exited").unwrap());
    }
}
