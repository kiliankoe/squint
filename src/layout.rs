//! Geometry for fitting text to the screen, kept free of AppKit so it can be tested.

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Size {
    pub width: f64,
    pub height: f64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Center,
    Left,
    Right,
}

/// Font size that makes text measured at `reference` points as large as fits on `screen`.
/// `quarter_turns` is the rotation in 90° steps, as with sm's `-r`.
pub fn fit_font_size(reference: f64, measured: Size, screen: Size, quarter_turns: u8) -> f64 {
    let rotated = rotate(measured, quarter_turns);
    reference * f64::min(screen.width / rotated.width, screen.height / rotated.height)
}

/// Origin of the unrotated text frame such that, once rotated around its center,
/// it sits on `screen` according to `align`.
pub fn text_origin(text: Size, screen: Size, quarter_turns: u8, align: Align) -> (f64, f64) {
    let rotated = rotate(text, quarter_turns);
    let center_x = match align {
        Align::Center => screen.width / 2.0,
        Align::Left => rotated.width / 2.0,
        Align::Right => screen.width - rotated.width / 2.0,
    };
    (
        center_x - text.width / 2.0,
        (screen.height - text.height) / 2.0,
    )
}

fn rotate(size: Size, quarter_turns: u8) -> Size {
    if quarter_turns.is_multiple_of(2) {
        size
    } else {
        Size {
            width: size.height,
            height: size.width,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn size(width: f64, height: f64) -> Size {
        Size { width, height }
    }

    #[test]
    fn fits_the_tighter_dimension() {
        assert_eq!(
            fit_font_size(100.0, size(200.0, 100.0), size(1000.0, 1000.0), 0),
            500.0
        );
        assert_eq!(
            fit_font_size(100.0, size(100.0, 100.0), size(1000.0, 300.0), 0),
            300.0
        );
    }

    #[test]
    fn rotation_swaps_dimensions() {
        assert_eq!(
            fit_font_size(100.0, size(200.0, 100.0), size(1000.0, 500.0), 1),
            250.0
        );
        assert_eq!(
            fit_font_size(100.0, size(200.0, 100.0), size(1000.0, 500.0), 2),
            500.0
        );
    }

    #[test]
    fn places_text_according_to_alignment() {
        let text = size(400.0, 100.0);
        let screen = size(1000.0, 800.0);
        assert_eq!(text_origin(text, screen, 0, Align::Center), (300.0, 350.0));
        assert_eq!(text_origin(text, screen, 0, Align::Left), (0.0, 350.0));
        assert_eq!(text_origin(text, screen, 0, Align::Right), (600.0, 350.0));
    }

    #[test]
    fn aligns_the_rotated_bounds() {
        let text = size(400.0, 100.0);
        let screen = size(1000.0, 800.0);
        // Rotated, the text is 100 wide, so its center sits 50 from the left edge.
        assert_eq!(text_origin(text, screen, 1, Align::Left), (-150.0, 350.0));
        assert_eq!(text_origin(text, screen, 3, Align::Right), (750.0, 350.0));
    }
}
