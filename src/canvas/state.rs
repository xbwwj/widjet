use vello_cpu::{
    kurbo::{Affine, Cap, Join, Stroke},
    peniko::Color,
};

use super::{TextAlign, TextBaseline};

/// The 2D drawing state that lives on the JS side.
///
/// This is intentionally separate from [`crate::canvas::CanvasStore`]: the
/// backend never needs to know the current fill style, transform, or saved
/// state stack.
#[derive(Clone, Debug)]
#[allow(dead_code)]
pub struct CanvasContextState {
    pub global_alpha: f32,
    pub fill_style: Color,
    pub fill_style_css: String,
    pub stroke_style: Color,
    pub stroke_style_css: String,
    pub line_width: f32,
    pub line_cap: String,
    pub line_join: String,
    pub miter_limit: f32,
    pub line_dash: Vec<f64>,
    pub line_dash_offset: f32,
    pub transform: Affine,
    pub shadow_offset_x: f32,
    pub shadow_offset_y: f32,
    pub shadow_blur: f32,
    pub shadow_color: Color,
    pub font: String,
    pub font_size: f32,
    pub font_family: String,
    pub font_weight: f32,
    pub text_align: TextAlign,
    pub text_baseline: TextBaseline,
    pub clips_pushed: usize,
}

impl Default for CanvasContextState {
    fn default() -> Self {
        Self {
            global_alpha: 1.0,
            fill_style: Color::from_rgba8(0, 0, 0, 255),
            fill_style_css: "#000000".to_string(),
            stroke_style: Color::from_rgba8(0, 0, 0, 255),
            stroke_style_css: "#000000".to_string(),
            line_width: 1.0,
            line_cap: "butt".to_string(),
            line_join: "miter".to_string(),
            miter_limit: 10.0,
            line_dash: Vec::new(),
            line_dash_offset: 0.0,
            transform: Affine::IDENTITY,
            shadow_offset_x: 0.0,
            shadow_offset_y: 0.0,
            shadow_blur: 0.0,
            shadow_color: Color::from_rgba8(0, 0, 0, 0),
            font: "10px sans-serif".to_string(),
            font_size: 10.0,
            font_family: "sans-serif".to_string(),
            font_weight: 400.0,
            text_align: TextAlign::Start,
            text_baseline: TextBaseline::Alphabetic,
            clips_pushed: 0,
        }
    }
}

impl CanvasContextState {
    pub fn stroke(&self) -> Stroke {
        let width = f64::from(self.line_width.max(0.0));
        let mut stroke = Stroke::new(width);
        stroke.miter_limit = f64::from(self.miter_limit.max(1.0));
        stroke.start_cap = match self.line_cap.as_str() {
            "round" => Cap::Round,
            "square" => Cap::Square,
            _ => Cap::Butt,
        };
        stroke.end_cap = stroke.start_cap;
        stroke.join = match self.line_join.as_str() {
            "round" => Join::Round,
            "bevel" => Join::Bevel,
            _ => Join::Miter,
        };
        stroke.dash_pattern = self.line_dash.iter().copied().collect();
        stroke.dash_offset = f64::from(self.line_dash_offset);
        stroke
    }

    pub fn effective_color(&self, color: Color) -> Color {
        if self.global_alpha >= 1.0 {
            return color;
        }
        let [r, g, b, a] = color.components;
        Color::new([r, g, b, a * self.global_alpha.clamp(0.0, 1.0)])
    }
}

/// Parse the small CSS color subset currently used by the canvas API.
pub fn parse_color(value: &str) -> Option<Color> {
    let value = value.trim().to_ascii_lowercase();
    match value.as_str() {
        "black" => return Some(Color::from_rgba8(0, 0, 0, 255)),
        "white" => return Some(Color::from_rgba8(255, 255, 255, 255)),
        "red" => return Some(Color::from_rgba8(255, 0, 0, 255)),
        "green" => return Some(Color::from_rgba8(0, 128, 0, 255)),
        "blue" => return Some(Color::from_rgba8(0, 0, 255, 255)),
        "transparent" => return Some(Color::from_rgba8(0, 0, 0, 0)),
        _ => {}
    }

    let hex = value.strip_prefix('#')?;
    let nibble = |byte: u8| -> Option<u8> {
        match byte {
            b'0'..=b'9' => Some(byte - b'0'),
            b'a'..=b'f' => Some(byte - b'a' + 10),
            _ => None,
        }
    };
    let pair = |a: u8, b: u8| -> Option<u8> { Some(nibble(a)? * 16 + nibble(b)?) };

    match hex.len() {
        3 => {
            let bytes = hex.as_bytes();
            Some(Color::from_rgba8(
                nibble(bytes[0])? * 17,
                nibble(bytes[1])? * 17,
                nibble(bytes[2])? * 17,
                255,
            ))
        }
        4 => {
            let bytes = hex.as_bytes();
            Some(Color::from_rgba8(
                nibble(bytes[0])? * 17,
                nibble(bytes[1])? * 17,
                nibble(bytes[2])? * 17,
                nibble(bytes[3])? * 17,
            ))
        }
        6 => {
            let bytes = hex.as_bytes();
            Some(Color::from_rgba8(
                pair(bytes[0], bytes[1])?,
                pair(bytes[2], bytes[3])?,
                pair(bytes[4], bytes[5])?,
                255,
            ))
        }
        8 => {
            let bytes = hex.as_bytes();
            Some(Color::from_rgba8(
                pair(bytes[0], bytes[1])?,
                pair(bytes[2], bytes[3])?,
                pair(bytes[4], bytes[5])?,
                pair(bytes[6], bytes[7])?,
            ))
        }
        _ => None,
    }
}

/// A parsed CSS font shorthand.
#[derive(Clone, Debug)]
pub struct ParsedFont {
    pub size: f32,
    pub family: String,
    pub weight: f32,
}

/// Minimal CSS font shorthand parser: `[weight] <size>px <family>`.
///
/// The weight token may be a numeric CSS weight (100..=1000) or one of the
/// usual keywords (`normal`, `bold`, `black`, ...). Other shorthand tokens are
/// accepted and ignored for now.
pub fn parse_font(value: &str) -> Option<ParsedFont> {
    let px = value.find("px")?;
    let prefix = value[..px].trim();
    let mut tokens = prefix.split_whitespace().collect::<Vec<_>>();
    let size = tokens.pop()?.parse::<f32>().ok()?;
    if !size.is_finite() || size <= 0.0 {
        return None;
    }

    let weight = tokens
        .iter()
        .rev()
        .find_map(|token| parse_font_weight(token))
        .unwrap_or(400.0);
    let family = value[px + 2..].trim();
    if family.is_empty() {
        return None;
    }

    Some(ParsedFont {
        size,
        family: family.to_string(),
        weight,
    })
}

fn parse_font_weight(token: &str) -> Option<f32> {
    let token = token.to_ascii_lowercase();
    let weight = match token.as_str() {
        "thin" => 100.0,
        "extralight" | "extra-light" | "ultralight" | "ultra-light" => 200.0,
        "light" => 300.0,
        "normal" | "regular" | "book" => 400.0,
        "medium" => 500.0,
        "semibold" | "semi-bold" | "demibold" | "demi-bold" => 600.0,
        "bold" => 700.0,
        "extrabold" | "extra-bold" | "ultrabold" | "ultra-bold" => 800.0,
        "black" | "heavy" => 900.0,
        "extrablack" | "extra-black" | "ultrablack" | "ultra-black" => 950.0,
        "bolder" => 700.0,
        "lighter" => 300.0,
        _ => token.parse::<f32>().ok()?,
    };

    weight.is_finite().then(|| weight.clamp(1.0, 1000.0))
}

pub fn parse_text_align(value: &str) -> Option<TextAlign> {
    match value {
        "start" => Some(TextAlign::Start),
        "end" => Some(TextAlign::End),
        "left" => Some(TextAlign::Left),
        "right" => Some(TextAlign::Right),
        "center" => Some(TextAlign::Center),
        _ => None,
    }
}

pub fn parse_text_baseline(value: &str) -> Option<TextBaseline> {
    match value {
        "top" => Some(TextBaseline::Top),
        "hanging" => Some(TextBaseline::Hanging),
        "middle" => Some(TextBaseline::Middle),
        "alphabetic" => Some(TextBaseline::Alphabetic),
        "ideographic" => Some(TextBaseline::Ideographic),
        "bottom" => Some(TextBaseline::Bottom),
        _ => None,
    }
}
