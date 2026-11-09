use std::{cell::RefCell, rc::Rc};

use rquickjs::{Class, Context, Ctx, JsLifetime, Runtime, class::Trace};
use wayland_protocols_wlr::layer_shell::v1::client::{
    zwlr_layer_shell_v1::Layer, zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
};

use crate::app::State;

pub struct JsAgent {
    rt: Runtime,
    ctx: Context,
}

impl JsAgent {
    pub fn new(state: Rc<RefCell<State>>) -> Self {
        let rt = Runtime::new().unwrap();
        let ctx = Context::full(&rt).unwrap();

        ctx.with(|ctx| {
            ctx.store_userdata(state).unwrap();

            let global = ctx.globals();
            Class::<WidgetCanvas>::define(&global).unwrap();
            Class::<CanvasRenderingContext2D>::define(&global).unwrap();
        });

        Self { rt, ctx }
    }
}

#[derive(JsLifetime)]
#[rquickjs::class(rename_all = "camelCase")]
struct WidgetCanvas {
    width: u32,
    height: u32,
    // TODO: anchor
    #[qjs(get, set)]
    exclusive_zone: i32,
    #[qjs(get, set)]
    margin_top: i32,
    #[qjs(get, set)]
    margin_right: i32,
    #[qjs(get, set)]
    margin_bottom: i32,
    #[qjs(get, set)]
    margin_left: i32,
    // TODO: keyboard_interactivity
    // TODO: layer
    // TODO: exclusive edge
    // TODO: on configure
    // TODO: on click
    // TODO: on contextmenu

    // TODO: it seems that we should store separate width height to avoid
    // sending duplicate `set_size` to wayland server.
    wlr_layer_surface: Rc<ZwlrLayerSurfaceV1>,
}

#[rquickjs::methods(rename_all = "camelCase")]
impl WidgetCanvas {
    #[qjs(constructor)]
    // TODO: parameters
    fn new(ctx: Ctx<'_>) -> Self {
        let state = ctx.userdata::<Rc<RefCell<State>>>().unwrap();
        let state = state.borrow_mut();

        let wl_surface = state
            .wl_compositor
            .as_ref()
            .unwrap()
            .create_surface(&state.qh, ());

        let wlr_layer_surface = state.wlr_layer_shell.as_ref().unwrap().get_layer_surface(
            &wl_surface,
            None,
            Layer::Top,
            Default::default(),
            &state.qh,
            // TODO: should be counting
            0,
        );
        // TODO: set initial size/anchor/... here

        wl_surface.commit();

        // TODO: should ack_configure async-ly later.
        // NOTE: ack configure should not be exposed to user script
        // but a resize event can fire on widget canvas.

        let wlr_layer_surface = Rc::new(wlr_layer_surface);
        // TODO: wlr layer surface should also be store in state for rendering stage.

        Self {
            width: 0,
            height: 0,
            exclusive_zone: 0,
            margin_top: 0,
            margin_right: 0,
            margin_bottom: 0,
            margin_left: 0,
            wlr_layer_surface,
        }
    }

    #[qjs(get, rename = "width")]
    fn width(&self) -> u32 {
        self.width
    }

    #[qjs(set, rename = "width")]
    fn set_width(&mut self, width: u32) {
        if self.width != width {
            self.width = width;
            self.send_size();
        }
    }

    #[qjs(get, rename = "height")]
    fn height(&self) -> u32 {
        self.height
    }

    #[qjs(set, rename = "height")]
    fn set_height(&mut self, height: u32) {
        if self.height != height {
            self.height = height;
            self.send_size();
        }
    }

    fn destroy(&self) {
        self.wlr_layer_surface.destroy();
    }

    fn add_event_listener(&self, _event: String) {
        todo!()
    }

    // TODO: getPopup
}

impl WidgetCanvas {
    // PERF: currently we send set_size each time width/height is changed,
    // this hould be buffered and apply only in render cycle.
    fn send_size(&self) {
        self.wlr_layer_surface.set_size(self.width, self.height);
    }
}

impl<'js> Trace<'js> for WidgetCanvas {
    fn trace<'a>(&self, _tracer: rquickjs::class::Tracer<'a, 'js>) {
        // nothing to trace
    }
}

#[derive(Trace, JsLifetime)]
#[rquickjs::class]
struct CanvasRenderingContext2D {}
