use rquickjs::{Class, Ctx, JsLifetime, TypedArray, class::Trace};

use crate::canvas::{Size, Snapshot};

#[derive(JsLifetime)]
#[rquickjs::class(rename_all = "camelCase")]
pub struct ImageData {
    width: u32,
    height: u32,
    /// Unpremultiplied RGBA8, as required by the Canvas 2D API.
    data: Vec<u8>,
}

impl ImageData {
    pub fn create<'js>(ctx: &Ctx<'js>, snapshot: Snapshot) -> rquickjs::Result<Class<'js, Self>> {
        Class::instance(
            ctx.clone(),
            Self {
                width: snapshot.size.width,
                height: snapshot.size.height,
                data: snapshot.to_unpremultiplied(),
            },
        )
    }

    pub fn from_parts(width: u32, height: u32, data: Vec<u8>) -> Self {
        Self {
            width,
            height,
            data,
        }
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Convert the unpremultiplied JS-facing data back to the premultiplied
    /// snapshot format used by the canvas backend.
    pub fn to_snapshot(&self) -> Snapshot {
        let mut data = Vec::with_capacity(self.data.len());
        for pixel in self.data.chunks_exact(4) {
            let alpha = u16::from(pixel[3]);
            let premultiply =
                |component: u8| ((u16::from(component) * alpha + 127) / 255).min(255) as u8;
            data.extend_from_slice(&[
                premultiply(pixel[0]),
                premultiply(pixel[1]),
                premultiply(pixel[2]),
                pixel[3],
            ]);
        }
        Snapshot::new(Size::new(self.width, self.height), data)
    }
}

#[rquickjs::methods(rename_all = "camelCase")]
impl ImageData {
    #[qjs(get, rename = "width")]
    fn get_width(&self) -> u32 {
        self.width
    }

    #[qjs(get, rename = "height")]
    fn get_height(&self) -> u32 {
        self.height
    }

    #[qjs(get, rename = "data")]
    fn get_data<'js>(&self, ctx: Ctx<'js>) -> rquickjs::Result<TypedArray<'js, u8>> {
        TypedArray::new_copy(ctx, self.data.as_slice())
    }
}

impl<'js> Trace<'js> for ImageData {
    fn trace<'a>(&self, _tracer: rquickjs::class::Tracer<'a, 'js>) {}
}
