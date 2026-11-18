use tiny_skia::{Path, PathBuilder};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArrowStyle {
    Classic,
    Curved,
}

pub struct ArrowGeometry {
    pub shaft_path: Path,
    pub head_path: Path,
}

pub fn build_arrow_geometry(
    p1: (f32, f32),
    p2: (f32, f32),
    thickness: f32,
    style: ArrowStyle,
) -> Option<ArrowGeometry> {
    let dx = p2.0 - p1.0;
    let dy = p2.1 - p1.1;
    let len = (dx * dx + dy * dy).sqrt();

    if len < 2.0 {
        return None;
    }

    let dir_x = dx / len;
    let dir_y = dy / len;
    let norm_x = -dir_y;
    let norm_y = dir_x;

    let head_len = (18.0 + thickness * 2.5).min(len * 0.85);
    let head_half_width = (10.0 + thickness * 2.0).min(len * 0.7);

    let base_x = p2.0 - dir_x * head_len;
    let base_y = p2.1 - dir_y * head_len;

    let left_x = base_x + norm_x * head_half_width;
    let left_y = base_y + norm_y * head_half_width;

    let right_x = base_x - norm_x * head_half_width;
    let right_y = base_y - norm_y * head_half_width;

    let mut head_builder = PathBuilder::new();

    match style {
        ArrowStyle::Classic => {
            head_builder.move_to(p2.0, p2.1);
            head_builder.line_to(left_x, left_y);
            head_builder.line_to(right_x, right_y);
            head_builder.close();
        }
        ArrowStyle::Curved => {
            let notch_depth = head_len * 0.35;
            let notch_x = base_x + dir_x * notch_depth;
            let notch_y = base_y + dir_y * notch_depth;

            head_builder.move_to(p2.0, p2.1);
            head_builder.line_to(left_x, left_y);
            head_builder.quad_to(
                left_x * 0.5 + notch_x * 0.5,
                left_y * 0.5 + notch_y * 0.5,
                notch_x,
                notch_y,
            );
            head_builder.quad_to(
                right_x * 0.5 + notch_x * 0.5,
                right_y * 0.5 + notch_y * 0.5,
                right_x,
                right_y,
            );
            head_builder.close();
        }
    }

    let mut shaft_builder = PathBuilder::new();
    shaft_builder.move_to(p1.0, p1.1);
    // Shaft extends slightly into the head base for clean overlap
    let shaft_end_x = base_x + dir_x * (thickness * 0.5);
    let shaft_end_y = base_y + dir_y * (thickness * 0.5);
    shaft_builder.line_to(shaft_end_x, shaft_end_y);

    let head_path = head_builder.finish()?;
    let shaft_path = shaft_builder.finish()?;

    Some(ArrowGeometry {
        shaft_path,
        head_path,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_arrow_geometry_generation() {
        let geom = build_arrow_geometry((0.0, 0.0), (100.0, 0.0), 3.0, ArrowStyle::Curved);
        assert!(geom.is_some());
    }
}
