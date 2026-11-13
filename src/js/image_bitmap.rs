use std::{cell::Cell, sync::Arc};

use rquickjs::{Class, Ctx, JsLifetime, class::Trace};

use crate::canvas::Snapshot;

#[derive(JsLifetime)]
#[rquickjs::class(rename_all = "camelCase")]
pub struct ImageBitmap {
    snapshot: Arc<Snapshot>,
    closed: Cell<bool>,
}

impl ImageBitmap {
    pub fn create<'js>(
        ctx: &Ctx<'js>,
        snapshot: Arc<Snapshot>,
    ) -> rquickjs::Result<Class<'js, Self>> {
        Class::instance(
            ctx.clone(),
            Self {
                snapshot,
                closed: Cell::new(false),
            },
        )
    }

    pub fn snapshot(&self) -> Option<Arc<Snapshot>> {
        if self.closed.get() {
            None
        } else {
            Some(self.snapshot.clone())
        }
    }
}

#[rquickjs::methods(rename_all = "camelCase")]
impl ImageBitmap {
    #[qjs(get, rename = "width")]
    fn width(&self) -> u32 {
        self.snapshot.size.width
    }

    #[qjs(get, rename = "height")]
    fn height(&self) -> u32 {
        self.snapshot.size.height
    }

    fn close(&self) {
        self.closed.set(true);
    }
}

impl<'js> Trace<'js> for ImageBitmap {
    fn trace<'a>(&self, _tracer: rquickjs::class::Tracer<'a, 'js>) {}
}
