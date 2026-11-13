use std::{cell::RefCell, rc::Rc};

use rquickjs::{Class, Ctx, Exception, JsLifetime, class::Trace};

use crate::canvas::{CanvasId, CanvasOwner, CanvasStore, Size, WidgetId, WidgetSpec, WidgetStore};

use super::{
    JsRuntimeData, context_2d::CanvasRenderingContext2D, offscreen_canvas::OffscreenCanvas,
};

/// JS facade for one widget surface. All WidgetId-keyed state lives in the
/// shared WidgetStore; this object retains only its identity and canvas refs.
#[derive(JsLifetime)]
#[rquickjs::class(rename_all = "camelCase")]
pub struct WidgetCanvas<'js> {
    widget_id: WidgetId,
    canvases: Rc<RefCell<CanvasStore>>,
    widgets: Rc<RefCell<WidgetStore>>,
    canvas_id: Option<CanvasId>,
    placeholder: Option<CanvasId>,
    context: Option<Class<'js, CanvasRenderingContext2D>>,
}

impl<'js> WidgetCanvas<'js> {
    fn size(&self) -> Size {
        self.widgets
            .borrow()
            .get(self.widget_id)
            .map(|state| state.size)
            .unwrap_or_default()
    }

    fn spec(&self) -> WidgetSpec {
        self.widgets
            .borrow()
            .get(self.widget_id)
            .map(|state| state.spec)
            .unwrap_or_default()
    }

    fn update_spec(&self, update: impl FnOnce(&mut WidgetSpec)) {
        self.widgets
            .borrow_mut()
            .update_spec(self.widget_id, update);
    }

    /// Mark the model entry as requiring a native surface. ScriptThread will
    /// materialize it on the next model-reconciliation step.
    fn ensure_widget(&self) {
        self.widgets.borrow_mut().mark_materialized(self.widget_id);
    }

    fn set_size(&self, size: Size) {
        self.widgets.borrow_mut().set_size(self.widget_id, size);
        if let Some(canvas_id) = self.canvas_id {
            self.canvases.borrow_mut().recreate(canvas_id, size);
        }
    }

    fn ensure_canvas(&mut self) -> CanvasId {
        if let Some(canvas_id) = self.canvas_id {
            return canvas_id;
        }
        self.ensure_widget();
        let size = self.size();
        let canvas_id = self.canvases.borrow_mut().create(
            size,
            CanvasOwner::Onscreen {
                widget: self.widget_id,
            },
        );
        self.canvas_id = Some(canvas_id);
        canvas_id
    }

    /// The canvas behind this widget for use as a `drawImage` source.
    /// After `transferControlToOffscreen`, this is the offscreen placeholder.
    pub fn canvas_id(&self) -> Option<CanvasId> {
        self.canvas_id.or(self.placeholder)
    }

    pub fn widget_id(&self) -> WidgetId {
        self.widget_id
    }
}

#[rquickjs::methods(rename_all = "camelCase")]
impl<'js> WidgetCanvas<'js> {
    #[qjs(constructor)]
    fn new(ctx: Ctx<'js>) -> rquickjs::Result<Self> {
        let Some(data) = ctx.userdata::<JsRuntimeData>() else {
            return Err(Exception::throw_message(
                &ctx,
                "WidgetCanvas used outside a widjet runtime",
            ));
        };
        let canvases = data.canvases.clone();
        let widgets = data.widgets.clone();
        let widget_id = {
            let mut store = widgets.borrow_mut();
            let id = store.allocate_id();
            // Register immediately so the WidgetStore is the only authoritative
            // WidgetId registry. `materialized=false` keeps surface creation lazy.
            store.register(id, WidgetSpec::default());
            id
        };

        Ok(Self {
            widget_id,
            canvases,
            widgets,
            canvas_id: None,
            placeholder: None,
            context: None,
        })
    }

    #[qjs(get, rename = "widgetId")]
    fn js_widget_id(&self) -> WidgetId {
        self.widget_id
    }

    /// Materialize this surface when its first surface-bound rAF is requested.
    #[qjs(rename = "__ensureWidgetForAnimationFrame")]
    fn ensure_widget_for_animation_frame(&self) {
        self.ensure_widget();
    }

    /// Synchronize the JS-visible size after the compositor configure event.
    /// The requested WidgetSpec is intentionally left unchanged here.
    #[qjs(rename = "__setSize")]
    fn set_platform_size(&self, width: u32, height: u32) {
        let size = Size::new(width, height);
        self.widgets
            .borrow_mut()
            .update_runtime(self.widget_id, |state| state.size = size);
    }

    #[qjs(get, rename = "width")]
    fn width(&self) -> u32 {
        self.size().width
    }

    #[qjs(set, rename = "width")]
    fn set_width(&self, width: u32) {
        if self.placeholder.is_some() {
            return;
        }
        let mut size = self.size();
        size.width = width;
        self.set_size(size);
    }

    #[qjs(get, rename = "height")]
    fn height(&self) -> u32 {
        self.size().height
    }

    #[qjs(set, rename = "height")]
    fn set_height(&self, height: u32) {
        if self.placeholder.is_some() {
            return;
        }
        let mut size = self.size();
        size.height = height;
        self.set_size(size);
    }

    #[qjs(get, rename = "anchor")]
    fn anchor(&self) -> u32 {
        self.spec().anchor
    }

    #[qjs(set, rename = "anchor")]
    fn set_anchor(&self, value: u32) {
        self.update_spec(|spec| spec.anchor = value);
    }

    #[qjs(get, rename = "exclusiveZone")]
    fn exclusive_zone(&self) -> i32 {
        self.spec().exclusive_zone
    }

    #[qjs(set, rename = "exclusiveZone")]
    fn set_exclusive_zone(&self, value: i32) {
        self.update_spec(|spec| spec.exclusive_zone = value);
    }

    #[qjs(get, rename = "marginTop")]
    fn margin_top(&self) -> i32 {
        self.spec().margin_top
    }

    #[qjs(set, rename = "marginTop")]
    fn set_margin_top(&self, value: i32) {
        self.update_spec(|spec| spec.margin_top = value);
    }

    #[qjs(get, rename = "marginRight")]
    fn margin_right(&self) -> i32 {
        self.spec().margin_right
    }

    #[qjs(set, rename = "marginRight")]
    fn set_margin_right(&self, value: i32) {
        self.update_spec(|spec| spec.margin_right = value);
    }

    #[qjs(get, rename = "marginBottom")]
    fn margin_bottom(&self) -> i32 {
        self.spec().margin_bottom
    }

    #[qjs(set, rename = "marginBottom")]
    fn set_margin_bottom(&self, value: i32) {
        self.update_spec(|spec| spec.margin_bottom = value);
    }

    #[qjs(get, rename = "marginLeft")]
    fn margin_left(&self) -> i32 {
        self.spec().margin_left
    }

    #[qjs(set, rename = "marginLeft")]
    fn set_margin_left(&self, value: i32) {
        self.update_spec(|spec| spec.margin_left = value);
    }

    fn get_context(
        &mut self,
        ctx: Ctx<'js>,
        ty: String,
    ) -> rquickjs::Result<Option<Class<'js, CanvasRenderingContext2D>>> {
        if ty != "2d" || self.placeholder.is_some() {
            return Ok(None);
        }
        if let Some(context) = &self.context {
            return Ok(Some(context.clone()));
        }

        let canvas_id = self.ensure_canvas();
        let context = CanvasRenderingContext2D::create(&ctx, self.canvases.clone(), canvas_id)?;
        self.context = Some(context.clone());
        Ok(Some(context))
    }

    fn transfer_control_to_offscreen(
        &mut self,
        ctx: Ctx<'js>,
    ) -> rquickjs::Result<Class<'js, OffscreenCanvas<'js>>> {
        if self.context.is_some() || self.placeholder.is_some() {
            return Err(Exception::throw_message(
                &ctx,
                "WidgetCanvas already has a rendering context",
            ));
        }

        self.ensure_widget();
        let size = self.size();
        let offscreen = OffscreenCanvas::create_with_placeholder(
            &ctx,
            size.width,
            size.height,
            Some(self.widget_id),
        )?;
        self.placeholder = Some(offscreen.borrow().canvas_id());
        Ok(offscreen)
    }

    fn destroy(&mut self) {
        if let Some(canvas_id) = self.canvas_id.take() {
            self.canvases.borrow_mut().destroy(canvas_id);
        }
        if let Some(canvas_id) = self.placeholder.take() {
            self.canvases.borrow_mut().destroy(canvas_id);
        }
        self.context = None;
        self.widgets.borrow_mut().mark_removed(self.widget_id);
    }
}

impl<'js> Trace<'js> for WidgetCanvas<'js> {
    fn trace<'a>(&self, tracer: rquickjs::class::Tracer<'a, 'js>) {
        self.context.trace(tracer);
    }
}
