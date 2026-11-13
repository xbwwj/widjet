use std::sync::Arc;

use parley::{
    Alignment, AlignmentOptions, FontContext, FontFamily, FontFamilyName, FontWeight, Layout,
    LayoutContext, PositionedLayoutItem, StyleProperty,
};
use vello_cpu::{
    Pixmap, RasterizerSettings, RenderContext, Resources, TargetInit,
    kurbo::{Affine, BezPath, Rect},
    peniko::{Color, Fill},
};

use super::{
    CanvasCommand, FillRule, ImageSource, Size, Snapshot, TextAlign, TextBaseline, TextMetrics,
};

/// A canvas pixel backend.
///
/// This is the equivalent of Servo's `CanvasData<DrawTarget>`: it owns the
/// drawing surface, but knows nothing about JS objects or platform surfaces.
pub trait CanvasTarget {
    fn recreate(&mut self, size: Size);
    fn clear(&mut self);
    fn execute(&mut self, command: CanvasCommand);
    fn snapshot(&mut self) -> Arc<Snapshot>;
    fn measure_text(
        &mut self,
        text: &str,
        font_size: f32,
        font_family: &str,
        font_weight: f32,
        max_width: Option<f64>,
    ) -> TextMetrics;
}

pub struct VelloCpuTarget {
    size: Size,
    pixmap: Pixmap,
    resources: Resources,
    clips: Vec<(BezPath, Fill)>,
    font_cx: Option<FontContext>,
    layout_cx: Option<LayoutContext<()>>,
}

impl VelloCpuTarget {
    pub fn new(size: Size) -> Self {
        let (width, height) = size.backing();
        Self {
            size,
            pixmap: Pixmap::new(width, height),
            resources: Resources::new(),
            clips: Vec::new(),
            font_cx: None,
            layout_cx: None,
        }
    }

    fn render_with_transform(
        &mut self,
        transform: Affine,
        draw: impl FnOnce(&mut RenderContext, &mut Resources),
    ) {
        if self.size.is_empty() {
            return;
        }
        let (width, height) = self.size.backing();
        let mut context = RenderContext::new(width, height);

        context.set_transform(Affine::IDENTITY);
        for (path, fill_rule) in &self.clips {
            context.set_fill_rule(*fill_rule);
            context.push_clip_path(path);
        }

        context.set_transform(transform);
        draw(&mut context, &mut self.resources);
        context.flush();

        for _ in 0..self.clips.len() {
            context.pop_clip();
        }

        context.render_with(
            self.pixmap.as_mut(),
            &mut self.resources,
            RasterizerSettings {
                target_init: TargetInit::SrcOver,
                ..Default::default()
            },
        );
    }

    fn apply_global_alpha(&self, color: Color) -> Color {
        // The context state already folds globalAlpha into its color before
        // constructing commands. Keeping this helper makes that contract
        // explicit for future command sources.
        color
    }

    fn clear_rect(&mut self, rect: Rect) {
        if self.size.is_empty() {
            return;
        }
        let (width, height) = self.size.backing();
        let rect = rect.intersect(Rect::new(0.0, 0.0, f64::from(width), f64::from(height)));
        let x0 = rect.x0.floor().max(0.0) as u16;
        let y0 = rect.y0.floor().max(0.0) as u16;
        let x1 = rect.x1.ceil().clamp(0.0, f64::from(width)) as u16;
        let y1 = rect.y1.ceil().clamp(0.0, f64::from(height)) as u16;

        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let stride = usize::from(width) * 4;
        let data = self.pixmap.data_as_u8_slice_mut();
        for y in y0..y1 {
            let start = usize::from(y) * stride + usize::from(x0) * 4;
            let end = usize::from(y) * stride + usize::from(x1) * 4;
            data[start..end].fill(0);
        }
    }

    fn draw_snapshot(&mut self, snapshot: &Snapshot, source: Rect, dest: Rect, smoothing: bool) {
        if self.size.is_empty() || snapshot.size.is_empty() {
            return;
        }
        let (width, height) = self.size.backing();
        let dest = dest.intersect(Rect::new(0.0, 0.0, f64::from(width), f64::from(height)));
        let x0 = dest.x0.floor().max(0.0) as u16;
        let y0 = dest.y0.floor().max(0.0) as u16;
        let x1 = dest.x1.ceil().clamp(0.0, f64::from(width)) as u16;
        let y1 = dest.y1.ceil().clamp(0.0, f64::from(height)) as u16;
        if x0 >= x1 || y0 >= y1 {
            return;
        }

        let source_width = source.width().max(1.0);
        let source_height = source.height().max(1.0);
        let dest_width = dest.width().max(1.0);
        let dest_height = dest.height().max(1.0);

        let dst_stride = usize::from(width) * 4;
        let src_stride = snapshot.size.width as usize * 4;

        for y in y0..y1 {
            for x in x0..x1 {
                let u = ((f64::from(x) + 0.5 - dest.x0) / dest_width).clamp(0.0, 0.999_999);
                let v = ((f64::from(y) + 0.5 - dest.y0) / dest_height).clamp(0.0, 0.999_999);
                let source_x = ((source.x0 + u * source_width) as u32)
                    .min(snapshot.size.width.saturating_sub(1));
                let source_y = ((source.y0 + v * source_height) as u32)
                    .min(snapshot.size.height.saturating_sub(1));
                if smoothing {
                    // Nearest-neighbour is used until the full image sampling
                    // options are wired through. The flag is kept because it is
                    // part of the public command shape.
                }
                let source_offset = source_y as usize * src_stride + source_x as usize * 4;
                let destination_offset = y as usize * dst_stride + x as usize * 4;
                let source_pixel = [
                    snapshot.data[source_offset],
                    snapshot.data[source_offset + 1],
                    snapshot.data[source_offset + 2],
                    snapshot.data[source_offset + 3],
                ];
                let destination = &mut self.pixmap.data_as_u8_slice_mut()
                    [destination_offset..destination_offset + 4];
                let alpha = u16::from(source_pixel[3]);
                let inverse = 255 - alpha;
                for channel in 0..3 {
                    destination[channel] = (u16::from(source_pixel[channel])
                        + u16::from(destination[channel]) * inverse / 255)
                        .min(255) as u8;
                }
                destination[3] = (alpha + u16::from(destination[3]) * inverse / 255).min(255) as u8;
            }
        }
    }

    fn put_snapshot(&mut self, x: i32, y: i32, snapshot: &Snapshot) {
        if self.size.is_empty() || snapshot.size.is_empty() {
            return;
        }
        let (width, height) = self.size.backing();
        let dst_stride = usize::from(width) * 4;
        let src_stride = snapshot.size.width as usize * 4;
        for row in 0..snapshot.size.height {
            let destination_y = y + row as i32;
            if destination_y < 0 || destination_y >= i32::from(height) {
                continue;
            }
            for column in 0..snapshot.size.width {
                let destination_x = x + column as i32;
                if destination_x < 0 || destination_x >= i32::from(width) {
                    continue;
                }
                let source_offset = row as usize * src_stride + column as usize * 4;
                let destination_offset =
                    destination_y as usize * dst_stride + destination_x as usize * 4;
                let source = &snapshot.data[source_offset..source_offset + 4];
                self.pixmap.data_as_u8_slice_mut()[destination_offset..destination_offset + 4]
                    .copy_from_slice(source);
            }
        }
    }
}

impl VelloCpuTarget {
    fn layout_text(
        &mut self,
        text: &str,
        font_size: f32,
        font_family: &str,
        font_weight: f32,
        max_width: Option<f64>,
    ) -> Layout<()> {
        let font_cx = self.font_cx.get_or_insert_with(FontContext::new);
        let layout_cx = self.layout_cx.get_or_insert_with(LayoutContext::new);
        let mut builder = layout_cx.ranged_builder(font_cx, text, 1.0, true);
        builder.push_default(StyleProperty::FontFamily(FontFamily::from(
            FontFamilyName::named(font_family),
        )));
        builder.push_default(StyleProperty::FontSize(font_size.max(1.0)));
        builder.push_default(StyleProperty::FontWeight(FontWeight::new(
            font_weight.clamp(1.0, 1000.0),
        )));

        let mut layout: Layout<()> = builder.build(text);
        layout.break_all_lines(max_width.map(|width| width as f32));
        layout.align(Alignment::Start, AlignmentOptions::default());
        layout
    }

    fn text_metrics(&mut self, layout: &Layout<()>, font_size: f32) -> TextMetrics {
        let baseline = layout.first_baseline().unwrap_or(font_size) as f64;
        let height = layout.height() as f64;
        let width = layout.width() as f64;
        TextMetrics {
            width,
            actual_bounding_box_left: 0.0,
            actual_bounding_box_right: width,
            font_bounding_box_ascent: baseline,
            font_bounding_box_descent: (height - baseline).max(0.0),
            actual_bounding_box_ascent: baseline,
            actual_bounding_box_descent: (height - baseline).max(0.0),
            em_height_ascent: baseline,
            em_height_descent: (height - baseline).max(0.0),
            hanging_baseline: baseline * 0.8,
            alphabetic_baseline: baseline,
            ideographic_baseline: (height - (height - baseline) * 0.5).max(0.0),
        }
    }

    fn draw_text_layout(
        context: &mut RenderContext,
        resources: &mut Resources,
        layout: &Layout<()>,
        x: f64,
        y: f64,
        stroke: Option<vello_cpu::kurbo::Stroke>,
    ) {
        if let Some(stroke) = stroke.clone() {
            context.set_stroke(stroke);
        }

        for line in layout.lines() {
            for item in line.items() {
                let PositionedLayoutItem::GlyphRun(glyph_run) = item else {
                    continue;
                };
                let run = glyph_run.run();
                let normalized_coords: Vec<i16> = run
                    .normalized_coords()
                    .iter()
                    .map(|coord| (*coord).to_bits())
                    .collect();
                let glyphs = glyph_run.positioned_glyphs().map(|glyph| vello_cpu::Glyph {
                    id: glyph.id,
                    x: glyph.x + x as f32,
                    y: glyph.y + y as f32,
                });
                let builder = context
                    .glyph_run(resources, run.font())
                    .font_size(run.font_size())
                    .normalized_coords(&normalized_coords);
                let _ = if stroke.is_some() {
                    builder.stroke_glyphs(glyphs)
                } else {
                    builder.fill_glyphs(glyphs)
                };
            }
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn render_text(
        &mut self,
        text: &str,
        x: f64,
        y: f64,
        max_width: Option<f64>,
        font_size: f32,
        font_family: &str,
        font_weight: f32,
        text_align: TextAlign,
        text_baseline: TextBaseline,
        style: Color,
        stroke: Option<vello_cpu::kurbo::Stroke>,
        transform: Affine,
    ) {
        let layout = self.layout_text(text, font_size, font_family, font_weight, max_width);
        let width = f64::from(layout.width());
        let baseline = f64::from(layout.first_baseline().unwrap_or(font_size));
        let height = f64::from(layout.height());

        let x_offset = match text_align {
            TextAlign::Start | TextAlign::Left => 0.0,
            TextAlign::Center => -width / 2.0,
            TextAlign::End | TextAlign::Right => -width,
        };
        let y_offset = match text_baseline {
            TextBaseline::Top => 0.0,
            TextBaseline::Hanging => -baseline * 0.8,
            TextBaseline::Middle => -height / 2.0,
            TextBaseline::Alphabetic => -baseline,
            TextBaseline::Ideographic => -height,
            TextBaseline::Bottom => -height,
        };

        self.render_with_transform(transform, move |context, resources| {
            context.set_paint(style);
            Self::draw_text_layout(
                context,
                resources,
                &layout,
                x + x_offset,
                y + y_offset,
                stroke,
            );
        });
    }
}

impl CanvasTarget for VelloCpuTarget {
    fn recreate(&mut self, size: Size) {
        self.size = size;
        let (width, height) = size.backing();
        self.pixmap = Pixmap::new(width, height);
        self.clips.clear();
    }

    fn clear(&mut self) {
        self.pixmap.data_as_u8_slice_mut().fill(0);
    }

    fn execute(&mut self, command: CanvasCommand) {
        match command {
            CanvasCommand::Recreate(size) => self.recreate(size),
            CanvasCommand::ClearRect { rect, transform } => {
                self.clear_rect(transform.transform_rect_bbox(rect));
            }
            CanvasCommand::FillRect {
                rect,
                style,
                transform,
            } => {
                let style = self.apply_global_alpha(style);
                self.render_with_transform(transform, |context, _resources| {
                    context.set_paint(style);
                    context.fill_rect(&rect);
                });
            }
            CanvasCommand::StrokeRect {
                rect,
                style,
                stroke,
                transform,
            } => {
                let style = self.apply_global_alpha(style);
                self.render_with_transform(transform, |context, _resources| {
                    context.set_paint(style);
                    context.set_stroke(stroke);
                    context.stroke_rect(&rect);
                });
            }
            CanvasCommand::FillPath {
                path,
                fill_rule,
                style,
                transform,
            } => {
                let style = self.apply_global_alpha(style);
                self.render_with_transform(transform, |context, _resources| {
                    context.set_fill_rule(match fill_rule {
                        FillRule::NonZero => Fill::NonZero,
                        FillRule::EvenOdd => Fill::EvenOdd,
                    });
                    context.set_paint(style);
                    context.fill_path(&path);
                });
            }
            CanvasCommand::StrokePath {
                path,
                style,
                stroke,
                transform,
            } => {
                let style = self.apply_global_alpha(style);
                self.render_with_transform(transform, |context, _resources| {
                    context.set_paint(style);
                    context.set_stroke(stroke);
                    context.stroke_path(&path);
                });
            }
            CanvasCommand::ClipPath {
                mut path,
                fill_rule,
                transform,
            } => {
                path.apply_affine(transform);
                self.clips.push((
                    path,
                    match fill_rule {
                        FillRule::NonZero => Fill::NonZero,
                        FillRule::EvenOdd => Fill::EvenOdd,
                    },
                ));
            }
            CanvasCommand::PopClips(count) => {
                for _ in 0..count {
                    self.clips.pop();
                }
            }
            CanvasCommand::DrawImage {
                source,
                source_rect,
                dest_rect,
                smoothing,
                transform,
            } => {
                if let ImageSource::Bitmap(snapshot) = source {
                    let dest_rect = transform.transform_rect_bbox(dest_rect);
                    self.draw_snapshot(&snapshot, source_rect, dest_rect, smoothing);
                }
            }
            CanvasCommand::PutImageData { x, y, snapshot } => {
                self.put_snapshot(x, y, &snapshot);
            }
            CanvasCommand::FillText {
                text,
                x,
                y,
                max_width,
                font_size,
                font_family,
                font_weight,
                text_align,
                text_baseline,
                style,
                transform,
            } => {
                self.render_text(
                    &text,
                    x,
                    y,
                    max_width,
                    font_size,
                    &font_family,
                    font_weight,
                    text_align,
                    text_baseline,
                    style,
                    None,
                    transform,
                );
            }
            CanvasCommand::StrokeText {
                text,
                x,
                y,
                max_width,
                font_size,
                font_family,
                font_weight,
                text_align,
                text_baseline,
                style,
                stroke,
                transform,
            } => {
                self.render_text(
                    &text,
                    x,
                    y,
                    max_width,
                    font_size,
                    &font_family,
                    font_weight,
                    text_align,
                    text_baseline,
                    style,
                    Some(stroke),
                    transform,
                );
            }
        }
    }

    fn snapshot(&mut self) -> Arc<Snapshot> {
        let data = if self.size.is_empty() {
            Vec::new()
        } else {
            self.pixmap.data_as_u8_slice().to_vec()
        };
        Arc::new(Snapshot::new(self.size, data))
    }

    fn measure_text(
        &mut self,
        text: &str,
        font_size: f32,
        font_family: &str,
        font_weight: f32,
        max_width: Option<f64>,
    ) -> TextMetrics {
        let layout = self.layout_text(text, font_size, font_family, font_weight, max_width);
        self.text_metrics(&layout, font_size)
    }
}
