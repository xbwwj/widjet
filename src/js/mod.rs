mod context_2d;
mod image_bitmap;
mod image_data;
mod offscreen_canvas;
mod path;
mod text_metrics;
pub(crate) mod timers;
mod widget_canvas;

use std::{cell::RefCell, rc::Rc};

use rquickjs::{Class, Context, Function, JsLifetime, Persistent, class::Trace};

const RUNTIME_JS: &str = include_str!("runtime.js");

pub use context_2d::CanvasRenderingContext2D;
pub use image_bitmap::ImageBitmap;
pub use image_data::ImageData;
pub use offscreen_canvas::OffscreenCanvas;
pub use text_metrics::TextMetrics;
pub(crate) use timers::TimerRegistry;
pub use widget_canvas::WidgetCanvas;

use crate::canvas::{CanvasStore, WidgetStore};

/// Values exposed to native JS classes. The shared stores contain only data;
/// no platform host, native handle, or command queue is reachable from JS.
#[derive(JsLifetime)]
pub struct JsRuntimeData {
    pub canvases: Rc<RefCell<CanvasStore>>,
    pub widgets: Rc<RefCell<WidgetStore>>,
}

impl<'js> Trace<'js> for JsRuntimeData {
    fn trace<'a>(&self, _tracer: rquickjs::class::Tracer<'a, 'js>) {}
}

/// Install the Widjet classes and host functions into ScriptThread's context.
/// Runtime, Context and TimerRegistry remain owned directly by ScriptThread.
pub(crate) fn install(
    context: &Context,
    timers: Rc<RefCell<TimerRegistry>>,
    canvases: Rc<RefCell<CanvasStore>>,
    widgets: Rc<RefCell<WidgetStore>>,
) {
    context.with(|ctx| {
        ctx.store_userdata(JsRuntimeData { canvases, widgets })
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
        .expect("failed to create console host function");
        global
            .set("__widjet_log", log)
            .expect("failed to install console host function");

        let set_timeout = {
            let timers = timers.clone();
            Function::new(
                ctx.clone(),
                move |callback: Function, delay_ms: f64, repeat: bool| -> rquickjs::Result<u32> {
                    let callback_ctx = callback.ctx().clone();
                    let callback = Persistent::save(&callback_ctx, callback);
                    Ok(timers.borrow_mut().set(callback, delay_ms, repeat))
                },
            )
            .expect("failed to create setTimeout host function")
        };
        global
            .set("__widjet_set_timeout", set_timeout)
            .expect("failed to install setTimeout host function");

        let clear_timeout = {
            let timers = timers.clone();
            Function::new(ctx.clone(), move |id: u32| -> bool {
                timers.borrow_mut().clear(id)
            })
            .expect("failed to create clearTimeout host function")
        };
        global
            .set("__widjet_clear_timeout", clear_timeout)
            .expect("failed to install clearTimeout host function");

        let now = Function::new(ctx.clone(), move || -> f64 { timers.borrow().now_ms() })
            .expect("failed to create performance.now host function");
        global
            .set("__widjet_now", now)
            .expect("failed to install performance.now host function");

        ctx.eval::<rquickjs::Value, _>(RUNTIME_JS.as_bytes())
            .expect("failed to initialize JS runtime");
    });
}
