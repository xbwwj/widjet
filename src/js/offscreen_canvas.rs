use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

use rquickjs::{Class, Ctx, Exception, JsLifetime, class::Trace};

use crate::{
    bridge::{BridgeCommand, JsBridge, WidgetPatch},
    canvas::{CanvasId, CanvasOwner, SharedCanvasStore, Size, WidgetId},
};

use super::{JsRuntimeData, context_2d::CanvasRenderingContext2D, image_bitmap::ImageBitmap};

/// An offscreen canvas with its own `CanvasId` and pixel backend.
#[derive(JsLifetime)]
#[rquickjs::class(rename_all = "camelCase")]
pub struct OffscreenCanvas<'js> {
    canvas_id: CanvasId,
    width: Cell<u32>,
    height: Cell<u32>,
    placeholder: Option<WidgetId>,
    canvases: SharedCanvasStore,
    bridge: Rc<RefCell<JsBridge>>,
    context: Option<Class<'js, CanvasRenderingContext2D>>,
}

impl<'js> OffscreenCanvas<'js> {
    pub fn create_with_placeholder(
        ctx: &Ctx<'js>,
        width: u32,
        height: u32,
        placeholder: Option<WidgetId>,
    ) -> rquickjs::Result<Class<'js, Self>> {
        let Some(data) = ctx.userdata::<JsRuntimeData>() else {
            return Err(Exception::throw_message(
                ctx,
                "OffscreenCanvas used outside a widjet runtime",
            ));
        };
        let canvases = data.canvases.clone();
        let bridge = data.bridge.clone();
        let size = Size::new(width, height);
        let canvas_id = canvases
            .borrow_mut()
            .create(size, CanvasOwner::Offscreen { placeholder });

        if let Some(widget_id) = placeholder {
            bridge.borrow().push(BridgeCommand::UpdateWidget {
                widget_id,
                patch: WidgetPatch::SetSize { width, height },
            });
        }

        Class::instance(
            ctx.clone(),
            OffscreenCanvas {
                canvas_id,
                width: Cell::new(width),
                height: Cell::new(height),
                placeholder,
                canvases,
                bridge,
                context: None,
            },
        )
    }

    pub fn canvas_id(&self) -> CanvasId {
        self.canvas_id
    }

    fn ensure_context(
        &mut self,
        ctx: &Ctx<'js>,
    ) -> rquickjs::Result<Class<'js, CanvasRenderingContext2D>> {
        if let Some(context) = &self.context {
            return Ok(context.clone());
        }
        let context = CanvasRenderingContext2D::create(ctx, self.canvases.clone(), self.canvas_id)?;
        self.context = Some(context.clone());
        Ok(context)
    }

    fn resize(&self, width: u32, height: u32) {
        self.canvases
            .borrow_mut()
            .recreate(self.canvas_id, Size::new(width, height));
        if let Some(widget_id) = self.placeholder {
            self.bridge.borrow().push(BridgeCommand::UpdateWidget {
                widget_id,
                patch: WidgetPatch::SetSize { width, height },
            });
        }
    }
}

#[rquickjs::methods(rename_all = "camelCase")]
impl<'js> OffscreenCanvas<'js> {
    #[qjs(constructor)]
    fn new(ctx: Ctx<'js>, width: u32, height: u32) -> rquickjs::Result<Self> {
        let Some(data) = ctx.userdata::<JsRuntimeData>() else {
            return Err(Exception::throw_message(
                &ctx,
                "OffscreenCanvas used outside a widjet runtime",
            ));
        };
        let canvases = data.canvases.clone();
        let bridge = data.bridge.clone();
        let canvas_id = canvases.borrow_mut().create(
            Size::new(width, height),
            CanvasOwner::Offscreen { placeholder: None },
        );
        Ok(Self {
            canvas_id,
            width: Cell::new(width),
            height: Cell::new(height),
            placeholder: None,
            canvases,
            bridge,
            context: None,
        })
    }

    #[qjs(get, rename = "width")]
    fn width(&self) -> u32 {
        self.width.get()
    }

    #[qjs(set, rename = "width")]
    fn set_width(&self, width: u32) {
        self.width.set(width);
        self.resize(width, self.height.get());
    }

    #[qjs(get, rename = "height")]
    fn height(&self) -> u32 {
        self.height.get()
    }

    #[qjs(set, rename = "height")]
    fn set_height(&self, height: u32) {
        self.height.set(height);
        self.resize(self.width.get(), height);
    }

    fn get_context(
        &mut self,
        ctx: Ctx<'js>,
        ty: String,
    ) -> rquickjs::Result<Option<Class<'js, CanvasRenderingContext2D>>> {
        if ty != "2d" {
            return Ok(None);
        }
        Ok(Some(self.ensure_context(&ctx)?))
    }

    fn transfer_to_image_bitmap(
        &mut self,
        ctx: Ctx<'js>,
    ) -> rquickjs::Result<Class<'js, ImageBitmap>> {
        let Some(snapshot) = self.canvases.borrow_mut().snapshot(self.canvas_id) else {
            return Err(Exception::throw_message(&ctx, "canvas no longer exists"));
        };
        let bitmap = ImageBitmap::create(&ctx, snapshot)?;

        // Step 4: reset the bitmap while preserving the context state.
        self.canvases.borrow_mut().clear(self.canvas_id);
        Ok(bitmap)
    }
}

impl<'js> Drop for OffscreenCanvas<'js> {
    fn drop(&mut self) {
        self.canvases.borrow_mut().destroy(self.canvas_id);
    }
}

impl<'js> Trace<'js> for OffscreenCanvas<'js> {
    fn trace<'a>(&self, tracer: rquickjs::class::Tracer<'a, 'js>) {
        self.context.trace(tracer);
    }
}
