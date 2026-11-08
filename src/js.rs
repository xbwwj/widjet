use rquickjs::{JsLifetime, class::Trace};

#[derive(Trace, JsLifetime)]
#[rquickjs::class(rename_all = "camelCase")]
struct WidgetCanvas {
    // TODO: keep Rc<RefCell<WlrLayerSurface>> or a wrapper
    #[qjs(get, set)]
    width: u32,
    #[qjs(get, set)]
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
}

#[rquickjs::methods(rename_all = "camelCase")]
impl WidgetCanvas {
    #[qjs(constructor)]
    // TODO: parameters
    fn new() -> Self {
        todo!()
    }

    fn get_popup(&self) {
        todo!()
    }

    fn destroy(&self) {
        todo!()
    }

    fn add_event_listener(&self, _event: String) {
        todo!()
    }
}

#[derive(Trace, JsLifetime)]
#[rquickjs::class]
struct CanvasRenderingContext2D {}
