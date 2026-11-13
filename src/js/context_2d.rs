use std::{cell::RefCell, sync::Arc};

use rquickjs::{Class, Ctx, JsLifetime, Value, class::Trace, function::Opt};
use vello_cpu::kurbo::{Affine, BezPath, Rect};

use crate::canvas::{
    CanvasCommand, CanvasId, FillRule, ImageSource, SharedCanvasStore, Size, Snapshot,
    state::{CanvasContextState, parse_color, parse_font, parse_text_align, parse_text_baseline},
};

use super::{
    image_bitmap::ImageBitmap, image_data::ImageData, offscreen_canvas::OffscreenCanvas, path,
    text_metrics::TextMetrics, widget_canvas::WidgetCanvas,
};

#[derive(JsLifetime)]
#[rquickjs::class(rename_all = "camelCase")]
pub struct CanvasRenderingContext2D {
    canvases: SharedCanvasStore,
    canvas_id: CanvasId,
    state: RefCell<CanvasContextState>,
    saved_states: RefCell<Vec<CanvasContextState>>,
    current_path: RefCell<BezPath>,
}

impl CanvasRenderingContext2D {
    pub fn create<'js>(
        ctx: &Ctx<'js>,
        canvases: SharedCanvasStore,
        canvas_id: CanvasId,
    ) -> rquickjs::Result<Class<'js, Self>> {
        Class::instance(
            ctx.clone(),
            Self {
                canvases,
                canvas_id,
                state: RefCell::new(CanvasContextState::default()),
                saved_states: RefCell::new(Vec::new()),
                current_path: RefCell::new(BezPath::new()),
            },
        )
    }

    fn rect_from_xywh(x: f64, y: f64, width: f64, height: f64) -> Option<Rect> {
        if ![x, y, width, height].iter().all(|value| value.is_finite()) {
            return None;
        }
        if width == 0.0 || height == 0.0 {
            return None;
        }
        Some(Rect::new(
            x.min(x + width),
            y.min(y + height),
            x.max(x + width),
            y.max(y + height),
        ))
    }

    fn execute(&self, command: CanvasCommand) {
        self.canvases.borrow_mut().execute(self.canvas_id, command);
    }

    fn fill_color(&self) -> vello_cpu::peniko::Color {
        let state = self.state.borrow();
        state.effective_color(state.fill_style)
    }

    fn stroke_color(&self) -> vello_cpu::peniko::Color {
        let state = self.state.borrow();
        state.effective_color(state.stroke_style)
    }

    fn transform(&self) -> Affine {
        self.state.borrow().transform
    }
}

#[rquickjs::methods(rename_all = "camelCase")]
impl CanvasRenderingContext2D {
    #[qjs(get, rename = "fillStyle")]
    fn fill_style(&self) -> String {
        self.state.borrow().fill_style_css.clone()
    }

    #[qjs(set, rename = "fillStyle")]
    fn set_fill_style(&self, value: String) {
        let Some(color) = parse_color(&value) else {
            return;
        };
        let mut state = self.state.borrow_mut();
        state.fill_style = color;
        state.fill_style_css = value;
    }

    #[qjs(get, rename = "strokeStyle")]
    fn stroke_style(&self) -> String {
        self.state.borrow().stroke_style_css.clone()
    }

    #[qjs(set, rename = "strokeStyle")]
    fn set_stroke_style(&self, value: String) {
        let Some(color) = parse_color(&value) else {
            return;
        };
        let mut state = self.state.borrow_mut();
        state.stroke_style = color;
        state.stroke_style_css = value;
    }

    #[qjs(get, rename = "globalAlpha")]
    fn global_alpha(&self) -> f32 {
        self.state.borrow().global_alpha
    }

    #[qjs(set, rename = "globalAlpha")]
    fn set_global_alpha(&self, value: f32) {
        if value.is_finite() && (0.0..=1.0).contains(&value) {
            self.state.borrow_mut().global_alpha = value;
        }
    }

    #[qjs(get, rename = "lineWidth")]
    fn line_width(&self) -> f32 {
        self.state.borrow().line_width
    }

    #[qjs(set, rename = "lineWidth")]
    fn set_line_width(&self, value: f32) {
        if value.is_finite() && value > 0.0 {
            self.state.borrow_mut().line_width = value;
        }
    }

    #[qjs(get, rename = "font")]
    fn font(&self) -> String {
        self.state.borrow().font.clone()
    }

    #[qjs(set, rename = "font")]
    fn set_font(&self, value: String) {
        let Some(font) = parse_font(&value) else {
            return;
        };
        let mut state = self.state.borrow_mut();
        state.font = value;
        state.font_size = font.size;
        state.font_family = font.family;
        state.font_weight = font.weight;
    }

    #[qjs(get, rename = "textAlign")]
    fn text_align(&self) -> String {
        match self.state.borrow().text_align {
            crate::canvas::TextAlign::Start => "start",
            crate::canvas::TextAlign::End => "end",
            crate::canvas::TextAlign::Left => "left",
            crate::canvas::TextAlign::Right => "right",
            crate::canvas::TextAlign::Center => "center",
        }
        .to_string()
    }

    #[qjs(set, rename = "textAlign")]
    fn set_text_align(&self, value: String) {
        if let Some(align) = parse_text_align(&value) {
            self.state.borrow_mut().text_align = align;
        }
    }

    #[qjs(get, rename = "textBaseline")]
    fn text_baseline(&self) -> String {
        match self.state.borrow().text_baseline {
            crate::canvas::TextBaseline::Top => "top",
            crate::canvas::TextBaseline::Hanging => "hanging",
            crate::canvas::TextBaseline::Middle => "middle",
            crate::canvas::TextBaseline::Alphabetic => "alphabetic",
            crate::canvas::TextBaseline::Ideographic => "ideographic",
            crate::canvas::TextBaseline::Bottom => "bottom",
        }
        .to_string()
    }

    #[qjs(set, rename = "textBaseline")]
    fn set_text_baseline(&self, value: String) {
        if let Some(baseline) = parse_text_baseline(&value) {
            self.state.borrow_mut().text_baseline = baseline;
        }
    }

    #[qjs(get, rename = "lineCap")]
    fn line_cap(&self) -> String {
        self.state.borrow().line_cap.clone()
    }

    #[qjs(set, rename = "lineCap")]
    fn set_line_cap(&self, value: String) {
        if matches!(value.as_str(), "butt" | "round" | "square") {
            self.state.borrow_mut().line_cap = value;
        }
    }

    #[qjs(get, rename = "lineJoin")]
    fn line_join(&self) -> String {
        self.state.borrow().line_join.clone()
    }

    #[qjs(set, rename = "lineJoin")]
    fn set_line_join(&self, value: String) {
        if matches!(value.as_str(), "round" | "bevel" | "miter") {
            self.state.borrow_mut().line_join = value;
        }
    }

    #[qjs(get, rename = "miterLimit")]
    fn miter_limit(&self) -> f32 {
        self.state.borrow().miter_limit
    }

    #[qjs(set, rename = "miterLimit")]
    fn set_miter_limit(&self, value: f32) {
        if value.is_finite() && value > 0.0 {
            self.state.borrow_mut().miter_limit = value;
        }
    }

    #[qjs(get, rename = "lineDashOffset")]
    fn line_dash_offset(&self) -> f32 {
        self.state.borrow().line_dash_offset
    }

    #[qjs(set, rename = "lineDashOffset")]
    fn set_line_dash_offset(&self, value: f32) {
        if value.is_finite() {
            self.state.borrow_mut().line_dash_offset = value;
        }
    }

    fn set_line_dash(&self, segments: Vec<f64>) {
        if segments
            .iter()
            .all(|segment| segment.is_finite() && *segment >= 0.0)
        {
            self.state.borrow_mut().line_dash = segments;
        }
    }

    fn get_line_dash(&self) -> Vec<f64> {
        self.state.borrow().line_dash.clone()
    }

    fn save(&self) {
        self.saved_states
            .borrow_mut()
            .push(self.state.borrow().clone());
    }

    fn restore(&self) {
        let Some(saved) = self.saved_states.borrow_mut().pop() else {
            return;
        };
        let clips = self.state.borrow().clips_pushed;
        if clips != 0 {
            self.execute(CanvasCommand::PopClips(
                clips.saturating_sub(saved.clips_pushed),
            ));
        }
        *self.state.borrow_mut() = saved;
    }

    fn reset(&self) {
        let clips = self.state.borrow().clips_pushed;
        if clips != 0 {
            self.execute(CanvasCommand::PopClips(clips));
        }
        *self.state.borrow_mut() = CanvasContextState::default();
        self.saved_states.borrow_mut().clear();
        *self.current_path.borrow_mut() = BezPath::new();
        self.canvases.borrow_mut().clear(self.canvas_id);
    }

    fn begin_path(&self) {
        *self.current_path.borrow_mut() = BezPath::new();
    }

    fn close_path(&self) {
        path::close_path(&mut self.current_path.borrow_mut());
    }

    fn move_to(&self, x: f64, y: f64) {
        path::move_to(&mut self.current_path.borrow_mut(), x, y);
    }

    fn line_to(&self, x: f64, y: f64) {
        path::line_to(&mut self.current_path.borrow_mut(), x, y);
    }

    fn quadratic_curve_to(&self, cpx: f64, cpy: f64, x: f64, y: f64) {
        path::quadratic_curve_to(&mut self.current_path.borrow_mut(), cpx, cpy, x, y);
    }

    fn bezier_curve_to(&self, cp1x: f64, cp1y: f64, cp2x: f64, cp2y: f64, x: f64, y: f64) {
        path::bezier_curve_to(
            &mut self.current_path.borrow_mut(),
            cp1x,
            cp1y,
            cp2x,
            cp2y,
            x,
            y,
        );
    }

    fn arc_to(&self, x1: f64, y1: f64, x2: f64, y2: f64, radius: f64) {
        path::arc_to(&mut self.current_path.borrow_mut(), x1, y1, x2, y2, radius);
    }

    fn rect(&self, x: f64, y: f64, width: f64, height: f64) {
        path::rect(&mut self.current_path.borrow_mut(), x, y, width, height);
    }

    fn round_rect(&self, x: f64, y: f64, width: f64, height: f64, radii: Opt<f64>) {
        let radius = radii.0.unwrap_or(0.0);
        path::round_rect(
            &mut self.current_path.borrow_mut(),
            x,
            y,
            width,
            height,
            radius,
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn arc(
        &self,
        x: f64,
        y: f64,
        radius: f64,
        start_angle: f64,
        end_angle: f64,
        counterclockwise: Opt<bool>,
    ) {
        path::arc(
            &mut self.current_path.borrow_mut(),
            x,
            y,
            radius,
            start_angle,
            end_angle,
            counterclockwise.0.unwrap_or(false),
        );
    }

    #[allow(clippy::too_many_arguments)]
    fn ellipse(
        &self,
        x: f64,
        y: f64,
        radius_x: f64,
        radius_y: f64,
        rotation: f64,
        start_angle: f64,
        end_angle: f64,
        counterclockwise: Opt<bool>,
    ) {
        path::ellipse(
            &mut self.current_path.borrow_mut(),
            x,
            y,
            radius_x,
            radius_y,
            rotation,
            start_angle,
            end_angle,
            counterclockwise.0.unwrap_or(false),
        );
    }

    fn fill_rect(&self, x: f64, y: f64, width: f64, height: f64) {
        let Some(rect) = Self::rect_from_xywh(x, y, width, height) else {
            return;
        };
        let style = self.fill_color();
        let transform = self.transform();
        self.execute(CanvasCommand::FillRect {
            rect,
            style,
            transform,
        });
    }

    fn clear_rect(&self, x: f64, y: f64, width: f64, height: f64) {
        let Some(rect) = Self::rect_from_xywh(x, y, width, height) else {
            return;
        };
        let transform = self.transform();
        self.execute(CanvasCommand::ClearRect { rect, transform });
    }

    fn stroke_rect(&self, x: f64, y: f64, width: f64, height: f64) {
        let Some(rect) = Self::rect_from_xywh(x, y, width, height) else {
            return;
        };
        let style = self.stroke_color();
        let stroke = self.state.borrow().stroke();
        let transform = self.transform();
        self.execute(CanvasCommand::StrokeRect {
            rect,
            style,
            stroke,
            transform,
        });
    }

    fn fill(&self, fill_rule: Opt<String>) {
        let path = self.current_path.borrow().clone();
        let style = self.fill_color();
        let fill_rule = match fill_rule.0.as_deref() {
            Some("evenodd") => FillRule::EvenOdd,
            _ => FillRule::NonZero,
        };
        let transform = self.transform();
        self.execute(CanvasCommand::FillPath {
            path,
            fill_rule,
            style,
            transform,
        });
    }

    fn stroke(&self) {
        let path = self.current_path.borrow().clone();
        let style = self.stroke_color();
        let stroke = self.state.borrow().stroke();
        let transform = self.transform();
        self.execute(CanvasCommand::StrokePath {
            path,
            style,
            stroke,
            transform,
        });
    }

    fn clip(&self, fill_rule: Opt<String>) {
        let path = self.current_path.borrow().clone();
        let fill_rule = match fill_rule.0.as_deref() {
            Some("evenodd") => FillRule::EvenOdd,
            _ => FillRule::NonZero,
        };
        let transform = self.transform();
        self.state.borrow_mut().clips_pushed += 1;
        self.execute(CanvasCommand::ClipPath {
            path,
            fill_rule,
            transform,
        });
    }

    fn translate(&self, x: f64, y: f64) {
        if x.is_finite() && y.is_finite() {
            let mut state = self.state.borrow_mut();
            state.transform = state.transform * Affine::translate((x, y));
        }
    }

    fn scale(&self, x: f64, y: f64) {
        if x.is_finite() && y.is_finite() {
            let mut state = self.state.borrow_mut();
            state.transform = state.transform * Affine::scale_non_uniform(x, y);
        }
    }

    fn rotate(&self, angle: f64) {
        if angle.is_finite() {
            let mut state = self.state.borrow_mut();
            state.transform = state.transform * Affine::rotate(angle);
        }
    }

    fn set_transform(&self, a: f64, b: f64, c: f64, d: f64, e: f64, f: f64) {
        if [a, b, c, d, e, f].iter().all(|value| value.is_finite()) {
            self.state.borrow_mut().transform = Affine::new([a, b, c, d, e, f]);
        }
    }

    fn reset_transform(&self) {
        self.state.borrow_mut().transform = Affine::IDENTITY;
    }

    fn draw_image<'js>(
        &self,
        _ctx: Ctx<'js>,
        source: Value<'js>,
        dx: f64,
        dy: f64,
        dw: Opt<f64>,
        dh: Opt<f64>,
    ) -> rquickjs::Result<()> {
        if !dx.is_finite() || !dy.is_finite() {
            return Ok(());
        }

        let Some(object) = source.as_object() else {
            return Ok(());
        };

        let (image_source, source_size) = if object.instance_of::<ImageBitmap>() {
            let bitmap = Class::<ImageBitmap>::from_object(object)
                .expect("instance_of and from_object disagree");
            let snapshot = bitmap.borrow().snapshot();
            let Some(snapshot) = snapshot else {
                return Ok(());
            };
            let size = snapshot.size;
            (ImageSource::Bitmap(snapshot), size)
        } else if object.instance_of::<WidgetCanvas>() {
            let canvas = Class::<WidgetCanvas>::from_object(object)
                .expect("instance_of and from_object disagree");
            let Some(canvas_id) = canvas.borrow().canvas_id() else {
                return Ok(());
            };
            let Some(size) = self.canvases.borrow().size(canvas_id) else {
                return Ok(());
            };
            (ImageSource::Canvas(canvas_id), size)
        } else if object.instance_of::<OffscreenCanvas>() {
            let canvas = Class::<OffscreenCanvas>::from_object(object)
                .expect("instance_of and from_object disagree");
            let canvas_id = canvas.borrow().canvas_id();
            let Some(size) = self.canvases.borrow().size(canvas_id) else {
                return Ok(());
            };
            (ImageSource::Canvas(canvas_id), size)
        } else {
            return Ok(());
        };

        if source_size.is_empty() {
            return Ok(());
        }

        let source_width = f64::from(source_size.width);
        let source_height = f64::from(source_size.height);
        let dest_width =
            dw.0.filter(|value| value.is_finite())
                .unwrap_or(source_width);
        let dest_height =
            dh.0.filter(|value| value.is_finite())
                .unwrap_or(source_height);
        if dest_width == 0.0 || dest_height == 0.0 {
            return Ok(());
        }

        self.execute(CanvasCommand::DrawImage {
            source: image_source,
            source_rect: Rect::new(0.0, 0.0, source_width, source_height),
            dest_rect: Rect::new(dx, dy, dx + dest_width, dy + dest_height),
            smoothing: true,
            transform: self.transform(),
        });
        Ok(())
    }

    fn fill_text(&self, text: String, x: f64, y: f64, max_width: Opt<f64>) {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        let (style, transform, font_size, font_family, font_weight, text_align, text_baseline) = {
            let state = self.state.borrow();
            (
                state.effective_color(state.fill_style),
                state.transform,
                state.font_size,
                state.font_family.clone(),
                state.font_weight,
                state.text_align,
                state.text_baseline,
            )
        };
        self.execute(CanvasCommand::FillText {
            text,
            x,
            y,
            max_width: max_width
                .0
                .filter(|value| value.is_finite() && *value > 0.0),
            font_size,
            font_family,
            font_weight,
            text_align,
            text_baseline,
            style,
            transform,
        });
    }

    fn stroke_text(&self, text: String, x: f64, y: f64, max_width: Opt<f64>) {
        if !x.is_finite() || !y.is_finite() {
            return;
        }
        let (
            style,
            stroke,
            transform,
            font_size,
            font_family,
            font_weight,
            text_align,
            text_baseline,
        ) = {
            let state = self.state.borrow();
            (
                state.effective_color(state.stroke_style),
                state.stroke(),
                state.transform,
                state.font_size,
                state.font_family.clone(),
                state.font_weight,
                state.text_align,
                state.text_baseline,
            )
        };
        self.execute(CanvasCommand::StrokeText {
            text,
            x,
            y,
            max_width: max_width
                .0
                .filter(|value| value.is_finite() && *value > 0.0),
            font_size,
            font_family,
            font_weight,
            text_align,
            text_baseline,
            style,
            stroke,
            transform,
        });
    }

    fn measure_text<'js>(
        &self,
        ctx: Ctx<'js>,
        text: String,
    ) -> rquickjs::Result<Class<'js, TextMetrics>> {
        let (font_size, font_family, font_weight) = {
            let state = self.state.borrow();
            (
                state.font_size,
                state.font_family.clone(),
                state.font_weight,
            )
        };
        let metrics = self.canvases.borrow_mut().measure_text(
            self.canvas_id,
            &text,
            font_size,
            &font_family,
            font_weight,
            None,
        );
        TextMetrics::create(&ctx, metrics)
    }

    fn get_image_data<'js>(
        &self,
        ctx: Ctx<'js>,
        sx: i32,
        sy: i32,
        sw: i32,
        sh: i32,
    ) -> rquickjs::Result<Class<'js, ImageData>> {
        let snapshot = self
            .canvases
            .borrow_mut()
            .get_image_data(self.canvas_id, sx, sy, sw, sh)
            .unwrap_or_else(|| Snapshot::empty(Size::new(sw.max(0) as u32, sh.max(0) as u32)));
        ImageData::create(&ctx, snapshot)
    }

    fn put_image_data(&self, image: Class<'_, ImageData>, dx: i32, dy: i32) {
        let image = image.borrow();
        let snapshot = Arc::new(image.to_snapshot());
        self.canvases
            .borrow_mut()
            .put_image_data(self.canvas_id, dx, dy, snapshot);
    }
}

impl<'js> Trace<'js> for CanvasRenderingContext2D {
    fn trace<'a>(&self, _tracer: rquickjs::class::Tracer<'a, 'js>) {}
}
