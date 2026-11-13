use rquickjs::{Class, Ctx, JsLifetime, class::Trace};

use crate::canvas::TextMetrics as CanvasTextMetrics;

#[derive(JsLifetime)]
#[rquickjs::class(rename_all = "camelCase")]
pub struct TextMetrics {
    metrics: CanvasTextMetrics,
}

impl TextMetrics {
    pub fn create<'js>(
        ctx: &Ctx<'js>,
        metrics: CanvasTextMetrics,
    ) -> rquickjs::Result<Class<'js, Self>> {
        Class::instance(ctx.clone(), Self { metrics })
    }
}

#[rquickjs::methods(rename_all = "camelCase")]
impl TextMetrics {
    #[qjs(get, rename = "width")]
    fn width(&self) -> f64 {
        self.metrics.width
    }

    #[qjs(get, rename = "actualBoundingBoxLeft")]
    fn actual_bounding_box_left(&self) -> f64 {
        self.metrics.actual_bounding_box_left
    }

    #[qjs(get, rename = "actualBoundingBoxRight")]
    fn actual_bounding_box_right(&self) -> f64 {
        self.metrics.actual_bounding_box_right
    }

    #[qjs(get, rename = "fontBoundingBoxAscent")]
    fn font_bounding_box_ascent(&self) -> f64 {
        self.metrics.font_bounding_box_ascent
    }

    #[qjs(get, rename = "fontBoundingBoxDescent")]
    fn font_bounding_box_descent(&self) -> f64 {
        self.metrics.font_bounding_box_descent
    }

    #[qjs(get, rename = "actualBoundingBoxAscent")]
    fn actual_bounding_box_ascent(&self) -> f64 {
        self.metrics.actual_bounding_box_ascent
    }

    #[qjs(get, rename = "actualBoundingBoxDescent")]
    fn actual_bounding_box_descent(&self) -> f64 {
        self.metrics.actual_bounding_box_descent
    }

    #[qjs(get, rename = "emHeightAscent")]
    fn em_height_ascent(&self) -> f64 {
        self.metrics.em_height_ascent
    }

    #[qjs(get, rename = "emHeightDescent")]
    fn em_height_descent(&self) -> f64 {
        self.metrics.em_height_descent
    }

    #[qjs(get, rename = "hangingBaseline")]
    fn hanging_baseline(&self) -> f64 {
        self.metrics.hanging_baseline
    }

    #[qjs(get, rename = "alphabeticBaseline")]
    fn alphabetic_baseline(&self) -> f64 {
        self.metrics.alphabetic_baseline
    }

    #[qjs(get, rename = "ideographicBaseline")]
    fn ideographic_baseline(&self) -> f64 {
        self.metrics.ideographic_baseline
    }
}

impl<'js> Trace<'js> for TextMetrics {
    fn trace<'a>(&self, _tracer: rquickjs::class::Tracer<'a, 'js>) {}
}
