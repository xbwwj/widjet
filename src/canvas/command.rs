use std::sync::Arc;

use vello_cpu::{
    kurbo::{Affine, BezPath, Rect, Stroke},
    peniko::Color,
};

use super::{CanvasId, Size, Snapshot, TextAlign, TextBaseline};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FillRule {
    NonZero,
    EvenOdd,
}

#[derive(Clone, Debug)]
pub enum ImageSource {
    Bitmap(Arc<Snapshot>),
    Canvas(CanvasId),
}

/// A fully resolved canvas operation.
///
/// Drawing state (fill style, transform, line width, ...) lives in the JS-side
/// `CanvasContextState`; every command carries everything the backend needs.
#[derive(Clone, Debug)]
pub enum CanvasCommand {
    Recreate(Size),
    ClearRect {
        rect: Rect,
        transform: Affine,
    },
    FillRect {
        rect: Rect,
        style: Color,
        transform: Affine,
    },
    StrokeRect {
        rect: Rect,
        style: Color,
        stroke: Stroke,
        transform: Affine,
    },
    FillPath {
        path: BezPath,
        fill_rule: FillRule,
        style: Color,
        transform: Affine,
    },
    StrokePath {
        path: BezPath,
        style: Color,
        stroke: Stroke,
        transform: Affine,
    },
    ClipPath {
        path: BezPath,
        fill_rule: FillRule,
        transform: Affine,
    },
    PopClips(usize),
    DrawImage {
        source: ImageSource,
        source_rect: Rect,
        dest_rect: Rect,
        smoothing: bool,
        transform: Affine,
    },
    PutImageData {
        x: i32,
        y: i32,
        snapshot: Arc<Snapshot>,
    },
    FillText {
        text: String,
        x: f64,
        y: f64,
        max_width: Option<f64>,
        font_size: f32,
        font_family: String,
        font_weight: f32,
        text_align: TextAlign,
        text_baseline: TextBaseline,
        style: Color,
        transform: Affine,
    },
    StrokeText {
        text: String,
        x: f64,
        y: f64,
        max_width: Option<f64>,
        font_size: f32,
        font_family: String,
        font_weight: f32,
        text_align: TextAlign,
        text_baseline: TextBaseline,
        style: Color,
        stroke: Stroke,
        transform: Affine,
    },
}
