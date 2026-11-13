//! Platform independent canvas resources.
//!
//! The layout here mirrors the split Servo uses for canvas:
//!
//! * a JS object owns a [`CanvasId`], never pixels or a platform surface;
//! * [`CanvasStore`] maps [`CanvasId`] to a [`target::CanvasTarget`];
//! * [`WidgetStore`] is the single WidgetId-keyed surface/scheduler registry;
//! * drawing commands are plain data ([`command::CanvasCommand`]);
//! * `ScriptThread` presents dirty canvases at frame boundaries.

pub mod command;
pub mod state;
pub mod store;
pub mod target;
pub mod widget;

pub use command::{CanvasCommand, FillRule, ImageSource};
pub use store::CanvasStore;
pub use widget::{WidgetState, WidgetStore};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextAlign {
    #[default]
    Start,
    End,
    Left,
    Right,
    Center,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TextBaseline {
    Top,
    Hanging,
    Middle,
    #[default]
    Alphabetic,
    Ideographic,
    Bottom,
}

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct TextMetrics {
    pub width: f64,
    pub actual_bounding_box_left: f64,
    pub actual_bounding_box_right: f64,
    pub font_bounding_box_ascent: f64,
    pub font_bounding_box_descent: f64,
    pub actual_bounding_box_ascent: f64,
    pub actual_bounding_box_descent: f64,
    pub em_height_ascent: f64,
    pub em_height_descent: f64,
    pub hanging_baseline: f64,
    pub alphabetic_baseline: f64,
    pub ideographic_baseline: f64,
}

pub type WidgetId = u32;

/// Desired native-surface configuration shared by the JS canvas object and
/// ScriptThread. This is state, not a queued platform command: ScriptThread
/// reconciles revisions through the `PlatformHost` trait.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct WidgetSpec {
    pub width: u32,
    pub height: u32,
    pub anchor: u32,
    pub exclusive_zone: i32,
    pub margin_top: i32,
    pub margin_right: i32,
    pub margin_bottom: i32,
    pub margin_left: i32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CanvasId(pub u64);

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Size {
    pub width: u32,
    pub height: u32,
}

impl Size {
    pub const fn new(width: u32, height: u32) -> Self {
        Self { width, height }
    }

    pub fn is_empty(self) -> bool {
        self.width == 0 || self.height == 0
    }

    pub fn backing(self) -> (u16, u16) {
        (
            self.width.clamp(1, u16::MAX as u32) as u16,
            self.height.clamp(1, u16::MAX as u32) as u16,
        )
    }
}

/// Which object a canvas belongs to.
///
/// An onscreen canvas is shown by a widget surface. An offscreen canvas has no
/// surface of its own, but may be linked to a widget as that widget's
/// `transferControlToOffscreen()` placeholder.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CanvasOwner {
    Onscreen { widget: WidgetId },
    Offscreen { placeholder: Option<WidgetId> },
    Detached,
}

impl CanvasOwner {
    pub fn widget(self) -> Option<WidgetId> {
        match self {
            CanvasOwner::Onscreen { widget } => Some(widget),
            CanvasOwner::Offscreen { placeholder } => placeholder,
            CanvasOwner::Detached => None,
        }
    }
}

/// A CPU snapshot of a canvas.
///
/// Pixels are premultiplied RGBA8, row-major, matching vello's pixmap layout.
#[derive(Clone, Debug)]
pub struct Snapshot {
    pub size: Size,
    pub data: Vec<u8>,
}

impl Snapshot {
    pub fn new(size: Size, data: Vec<u8>) -> Self {
        debug_assert_eq!(
            data.len(),
            size.width as usize * size.height as usize * 4,
            "snapshot buffer does not match size"
        );
        Self { size, data }
    }

    pub fn empty(size: Size) -> Self {
        Self {
            size,
            data: vec![0; size.width as usize * size.height as usize * 4],
        }
    }

    pub fn pixel(&self, x: u32, y: u32) -> Option<[u8; 4]> {
        if x >= self.size.width || y >= self.size.height {
            return None;
        }
        let offset = (y as usize * self.size.width as usize + x as usize) * 4;
        Some([
            self.data[offset],
            self.data[offset + 1],
            self.data[offset + 2],
            self.data[offset + 3],
        ])
    }

    /// Crop a rectangle out of this snapshot. Coordinates are clamped to the
    /// source bounds, as `getImageData` does for an out-of-bounds rectangle.
    pub fn crop(&self, x: i32, y: i32, width: i32, height: i32) -> Snapshot {
        let width = width.max(0) as u32;
        let height = height.max(0) as u32;
        let mut data = vec![0; width as usize * height as usize * 4];

        if width == 0 || height == 0 {
            return Snapshot::new(Size::new(width, height), data);
        }

        let x0 = x.max(0) as u32;
        let y0 = y.max(0) as u32;
        let x1 = ((x as i64 + width as i64).max(0) as u32).min(self.size.width);
        let y1 = ((y as i64 + height as i64).max(0) as u32).min(self.size.height);

        if x0 < x1 && y0 < y1 {
            let copy_width = x1 - x0;
            for row in y0..y1 {
                let source_offset = (row as usize * self.size.width as usize + x0 as usize) * 4;
                let destination_offset = ((row - y0) as usize * width as usize) * 4;
                let length = copy_width as usize * 4;
                data[destination_offset..destination_offset + length]
                    .copy_from_slice(&self.data[source_offset..source_offset + length]);
            }
        }

        Snapshot::new(Size::new(width, height), data)
    }

    /// Convert premultiplied RGBA8 to the unpremultiplied RGBA8 used by
    /// `ImageData.data`.
    pub fn to_unpremultiplied(&self) -> Vec<u8> {
        let mut output = Vec::with_capacity(self.data.len());
        for pixel in self.data.as_chunks::<4>().0 {
            let alpha = pixel[3];
            if alpha == 0 {
                output.extend_from_slice(&[0, 0, 0, 0]);
            } else {
                let unpremultiply = |component: u8| {
                    ((u16::from(component) * 255 + u16::from(alpha) / 2) / u16::from(alpha))
                        .min(255) as u8
                };
                output.extend_from_slice(&[
                    unpremultiply(pixel[0]),
                    unpremultiply(pixel[1]),
                    unpremultiply(pixel[2]),
                    alpha,
                ]);
            }
        }
        output
    }
}
