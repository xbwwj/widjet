use vello_cpu::kurbo::{Arc, BezPath, PathEl, Point, RoundedRect, Shape, Vec2};

fn ensure_subpath(path: &mut BezPath, x: f64, y: f64) {
    let needs_subpath = match path.elements().last() {
        None | Some(PathEl::ClosePath) => true,
        _ => false,
    };
    if needs_subpath {
        path.move_to((x, y));
    }
}

pub fn close_path(path: &mut BezPath) {
    path.close_path();
}

pub fn move_to(path: &mut BezPath, x: f64, y: f64) {
    if x.is_finite() && y.is_finite() {
        path.move_to((x, y));
    }
}

pub fn line_to(path: &mut BezPath, x: f64, y: f64) {
    if !x.is_finite() || !y.is_finite() {
        return;
    }
    let needs_move = match path.elements().last() {
        None | Some(PathEl::ClosePath) => true,
        _ => false,
    };
    if needs_move {
        path.move_to((x, y));
    } else {
        path.line_to((x, y));
    }
}

pub fn quadratic_curve_to(path: &mut BezPath, cpx: f64, cpy: f64, x: f64, y: f64) {
    if ![cpx, cpy, x, y].iter().all(|value| value.is_finite()) {
        return;
    }
    ensure_subpath(path, cpx, cpy);
    path.quad_to((cpx, cpy), (x, y));
}

pub fn bezier_curve_to(
    path: &mut BezPath,
    cp1x: f64,
    cp1y: f64,
    cp2x: f64,
    cp2y: f64,
    x: f64,
    y: f64,
) {
    if ![cp1x, cp1y, cp2x, cp2y, x, y]
        .iter()
        .all(|value| value.is_finite())
    {
        return;
    }
    ensure_subpath(path, cp1x, cp1y);
    path.curve_to((cp1x, cp1y), (cp2x, cp2y), (x, y));
}

pub fn rect(path: &mut BezPath, x: f64, y: f64, width: f64, height: f64) {
    if ![x, y, width, height].iter().all(|value| value.is_finite()) {
        return;
    }
    path.move_to((x, y));
    path.line_to((x + width, y));
    path.line_to((x + width, y + height));
    path.line_to((x, y + height));
    path.close_path();
    path.move_to((x, y));
}

pub fn round_rect(path: &mut BezPath, x: f64, y: f64, width: f64, height: f64, radius: f64) {
    if ![x, y, width, height, radius]
        .iter()
        .all(|value| value.is_finite())
        || radius < 0.0
    {
        return;
    }
    let rect = RoundedRect::new(x, y, x + width, y + height, radius);
    path.extend(rect.to_path(0.1));
}

pub fn arc(
    path: &mut BezPath,
    x: f64,
    y: f64,
    radius: f64,
    start_angle: f64,
    end_angle: f64,
    counterclockwise: bool,
) {
    ellipse(
        path,
        x,
        y,
        radius,
        radius,
        0.0,
        start_angle,
        end_angle,
        counterclockwise,
    );
}

#[allow(clippy::too_many_arguments)]
pub fn ellipse(
    path: &mut BezPath,
    x: f64,
    y: f64,
    radius_x: f64,
    radius_y: f64,
    rotation: f64,
    start_angle: f64,
    end_angle: f64,
    counterclockwise: bool,
) {
    if ![x, y, radius_x, radius_y, rotation, start_angle, end_angle]
        .iter()
        .all(|value| value.is_finite())
        || radius_x < 0.0
        || radius_y < 0.0
    {
        return;
    }

    let start = start_angle;
    let sweep = if counterclockwise {
        if end_angle >= start {
            -(std::f64::consts::TAU - (end_angle - start))
        } else {
            -(start - end_angle)
        }
    } else if start > end_angle {
        std::f64::consts::TAU - (start - end_angle)
    } else {
        end_angle - start
    };

    let start_point = point_on_ellipse(x, y, radius_x, radius_y, rotation, start);
    ensure_subpath(path, start_point.x, start_point.y);
    path.extend(Arc::new((x, y), (radius_x, radius_y), start, sweep, rotation).append_iter(0.1));
}

pub fn arc_to(path: &mut BezPath, x1: f64, y1: f64, x2: f64, y2: f64, radius: f64) {
    if ![x1, y1, x2, y2, radius]
        .iter()
        .all(|value| value.is_finite())
    {
        return;
    }
    if radius < 0.0 {
        return;
    }
    ensure_subpath(path, x1, y1);

    let Some(Point { x: x0, y: y0 }) = path.current_position() else {
        return;
    };
    if (x0, y0) == (x1, y1) || (x1, y1) == (x2, y2) || radius == 0.0 {
        path.line_to((x1, y1));
        return;
    }

    let a2 = (x0 - x1).powi(2) + (y0 - y1).powi(2);
    let b2 = (x1 - x2).powi(2) + (y1 - y2).powi(2);
    if a2 == 0.0 || b2 == 0.0 {
        path.line_to((x1, y1));
        return;
    }

    let direction = (x1 - x0) * (y2 - y1) - (y1 - y0) * (x2 - x1);
    if direction == 0.0 {
        path.line_to((x1, y1));
        return;
    }

    let c2 = (x0 - x2).powi(2) + (y0 - y2).powi(2);
    let cosx = ((a2 + b2 - c2) / (2.0 * (a2 * b2).sqrt())).clamp(-1.0, 1.0);
    let sinx = (1.0 - cosx.powi(2)).sqrt();
    if sinx == 0.0 {
        path.line_to((x1, y1));
        return;
    }
    let distance = radius / ((1.0 - cosx) / sinx);

    let anx = (x1 - x0) / a2.sqrt();
    let any = (y1 - y0) / a2.sqrt();
    let tangent1 = Point::new(x1 - anx * distance, y1 - any * distance);

    let bnx = (x1 - x2) / b2.sqrt();
    let bny = (y1 - y2) / b2.sqrt();
    let tangent2 = Point::new(x1 - bnx * distance, y1 - bny * distance);

    let anticlockwise = direction < 0.0;
    let center_x = tangent1.x + any * radius * if anticlockwise { 1.0 } else { -1.0 };
    let center_y = tangent1.y - anx * radius * if anticlockwise { 1.0 } else { -1.0 };
    let angle_start = (tangent1.y - center_y).atan2(tangent1.x - center_x);
    let angle_end = (tangent2.y - center_y).atan2(tangent2.x - center_x);

    path.line_to((tangent1.x, tangent1.y));
    let sweep = if anticlockwise {
        -(angle_start - angle_end).rem_euclid(std::f64::consts::TAU)
    } else {
        (angle_end - angle_start).rem_euclid(std::f64::consts::TAU)
    };
    path.extend(
        Arc::new(
            (center_x, center_y),
            (radius, radius),
            angle_start,
            sweep,
            0.0,
        )
        .append_iter(0.1),
    );
}

fn point_on_ellipse(
    x: f64,
    y: f64,
    radius_x: f64,
    radius_y: f64,
    rotation: f64,
    angle: f64,
) -> Point {
    let (sin, cos) = angle.sin_cos();
    let v = Vec2::new(radius_x * cos, radius_y * sin);
    let rotated = Vec2::new(
        v.x * rotation.cos() - v.y * rotation.sin(),
        v.x * rotation.sin() + v.y * rotation.cos(),
    );
    Point::new(x + rotated.x, y + rotated.y)
}
