use std::{cell::Cell, rc::Rc};

use rquickjs::{Class, Ctx, Exception, JsLifetime, class::Trace};

use crate::{
    bridge::{BridgeCommand, JsBridge, WidgetPatch, WidgetSpec},
    canvas::{CanvasId, CanvasOwner, SharedCanvasStore, Size, WidgetId},
};

use super::{
    JsRuntimeData, context_2d::CanvasRenderingContext2D, offscreen_canvas::OffscreenCanvas,
};

/// The onscreen canvas / widget surface exposed to JS.
///
/// It is a JS facade only: no `wl_surface`, no `wl_shm_pool`, no app `State`.
/// Platform work is deferred through [`JsBridge`].
#[derive(JsLifetime)]
#[rquickjs::class(rename_all = "camelCase")]
pub struct WidgetCanvas<'js> {
    widget_id: WidgetId,
    width: Cell<u32>,
    height: Cell<u32>,
    anchor: Cell<u32>,
    exclusive_zone: Cell<i32>,
    margins: Cell<(i32, i32, i32, i32)>,
    canvases: SharedCanvasStore,
    bridge: Rc<std::cell::RefCell<JsBridge>>,
    created: Cell<bool>,
    canvas_id: Option<CanvasId>,
    placeholder: Option<CanvasId>,
    context: Option<Class<'js, CanvasRenderingContext2D>>,
}

impl<'js> WidgetCanvas<'js> {
    fn send_patch(&self, patch: WidgetPatch) {
        self.bridge.borrow().push(BridgeCommand::UpdateWidget {
            widget_id: self.widget_id,
            patch,
        });
    }

    /// Create the platform surface lazily with the final initial size.
    ///
    /// Scripts commonly set `width`/`height` before calling `getContext()`.
    /// Deferring widget creation until the first canvas operation avoids a
    /// transient fullscreen layer surface configured from the default 0x0
    /// size.
    fn ensure_widget(&self) {
        if self.created.replace(true) {
            return;
        }

        let (top, right, bottom, left) = self.margins.get();
        self.bridge.borrow().push(BridgeCommand::CreateWidget {
            widget_id: self.widget_id,
            spec: WidgetSpec {
                width: self.width.get(),
                height: self.height.get(),
                anchor: self.anchor.get(),
                exclusive_zone: self.exclusive_zone.get(),
                margin_top: top,
                margin_right: right,
                margin_bottom: bottom,
                margin_left: left,
            },
        });
    }

    fn ensure_canvas(&mut self) -> CanvasId {
        if let Some(canvas_id) = self.canvas_id {
            return canvas_id;
        }
        self.ensure_widget();
        let canvas_id = self.canvases.borrow_mut().create(
            Size::new(self.width.get(), self.height.get()),
            CanvasOwner::Onscreen {
                widget: self.widget_id,
            },
        );
        self.canvas_id = Some(canvas_id);
        canvas_id
    }

    /// The canvas behind this widget for use as a `drawImage` source.
    ///
    /// After `transferControlToOffscreen`, this is the offscreen canvas.
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
        let widget_id = data.bridge.borrow().alloc_widget_id();
        let canvases = data.canvases.clone();
        let bridge = data.bridge.clone();
        let spec = WidgetSpec::default();

        Ok(Self {
            widget_id,
            width: Cell::new(spec.width),
            height: Cell::new(spec.height),
            anchor: Cell::new(spec.anchor),
            exclusive_zone: Cell::new(spec.exclusive_zone),
            margins: Cell::new((
                spec.margin_top,
                spec.margin_right,
                spec.margin_bottom,
                spec.margin_left,
            )),
            canvases,
            bridge,
            created: Cell::new(false),
            canvas_id: None,
            placeholder: None,
            context: None,
        })
    }

    #[qjs(get, rename = "widgetId")]
    fn js_widget_id(&self) -> WidgetId {
        self.widget_id
    }

    /// Update the cached size after a compositor configure event.
    ///
    /// This must not issue another resize request; the caller is reacting to
    /// the compositor, not driving it.
    #[qjs(rename = "__setSize")]
    fn set_platform_size(&self, width: u32, height: u32) {
        self.width.set(width);
        self.height.set(height);
    }

    #[qjs(get, rename = "width")]
    fn width(&self) -> u32 {
        self.width.get()
    }

    #[qjs(set, rename = "width")]
    fn set_width(&self, width: u32) {
        if self.placeholder.is_some() {
            return;
        }
        self.width.set(width);
        if let Some(canvas_id) = self.canvas_id {
            self.canvases
                .borrow_mut()
                .recreate(canvas_id, Size::new(width, self.height.get()));
        }
        if self.created.get() {
            self.send_patch(WidgetPatch::SetSize {
                width,
                height: self.height.get(),
            });
        }
    }

    #[qjs(get, rename = "height")]
    fn height(&self) -> u32 {
        self.height.get()
    }

    #[qjs(set, rename = "height")]
    fn set_height(&self, height: u32) {
        if self.placeholder.is_some() {
            return;
        }
        self.height.set(height);
        if let Some(canvas_id) = self.canvas_id {
            self.canvases
                .borrow_mut()
                .recreate(canvas_id, Size::new(self.width.get(), height));
        }
        if self.created.get() {
            self.send_patch(WidgetPatch::SetSize {
                width: self.width.get(),
                height,
            });
        }
    }

    #[qjs(get, rename = "anchor")]
    fn anchor(&self) -> u32 {
        self.anchor.get()
    }

    #[qjs(set, rename = "anchor")]
    fn set_anchor(&self, value: u32) {
        self.anchor.set(value);
        if self.created.get() {
            self.send_patch(WidgetPatch::SetAnchor(value));
        }
    }

    #[qjs(get, rename = "exclusiveZone")]
    fn exclusive_zone(&self) -> i32 {
        self.exclusive_zone.get()
    }

    #[qjs(set, rename = "exclusiveZone")]
    fn set_exclusive_zone(&self, value: i32) {
        self.exclusive_zone.set(value);
        if self.created.get() {
            self.send_patch(WidgetPatch::SetExclusiveZone(value));
        }
    }

    #[qjs(get, rename = "marginTop")]
    fn margin_top(&self) -> i32 {
        self.margins.get().0
    }

    #[qjs(set, rename = "marginTop")]
    fn set_margin_top(&self, value: i32) {
        let (_, right, bottom, left) = self.margins.get();
        self.margins.set((value, right, bottom, left));
        self.send_margins();
    }

    #[qjs(get, rename = "marginRight")]
    fn margin_right(&self) -> i32 {
        self.margins.get().1
    }

    #[qjs(set, rename = "marginRight")]
    fn set_margin_right(&self, value: i32) {
        let (top, _, bottom, left) = self.margins.get();
        self.margins.set((top, value, bottom, left));
        self.send_margins();
    }

    #[qjs(get, rename = "marginBottom")]
    fn margin_bottom(&self) -> i32 {
        self.margins.get().2
    }

    #[qjs(set, rename = "marginBottom")]
    fn set_margin_bottom(&self, value: i32) {
        let (top, right, _, left) = self.margins.get();
        self.margins.set((top, right, value, left));
        self.send_margins();
    }

    #[qjs(get, rename = "marginLeft")]
    fn margin_left(&self) -> i32 {
        self.margins.get().3
    }

    #[qjs(set, rename = "marginLeft")]
    fn set_margin_left(&self, value: i32) {
        let (top, right, bottom, _) = self.margins.get();
        self.margins.set((top, right, bottom, value));
        self.send_margins();
    }

    fn get_context(
        &mut self,
        ctx: Ctx<'js>,
        ty: String,
    ) -> rquickjs::Result<Option<Class<'js, CanvasRenderingContext2D>>> {
        if ty != "2d" {
            return Ok(None);
        }
        if self.placeholder.is_some() {
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
        let offscreen = OffscreenCanvas::create_with_placeholder(
            &ctx,
            self.width.get(),
            self.height.get(),
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
        self.bridge.borrow().push(BridgeCommand::DestroyWidget {
            widget_id: self.widget_id,
        });
    }
}

impl<'js> WidgetCanvas<'js> {
    fn send_margins(&self) {
        if !self.created.get() {
            return;
        }
        let (top, right, bottom, left) = self.margins.get();
        self.send_patch(WidgetPatch::SetMargin {
            top,
            right,
            bottom,
            left,
        });
    }
}

impl<'js> Trace<'js> for WidgetCanvas<'js> {
    fn trace<'a>(&self, tracer: rquickjs::class::Tracer<'a, 'js>) {
        self.context.trace(tracer);
    }
}
