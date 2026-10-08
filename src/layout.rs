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

/// Space kept free at the screen edges, in percent, written in CSS shorthand order.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Padding {
    top: f64,
    right: f64,
    bottom: f64,
    left: f64,
}

impl Padding {
    pub const fn new(top: f64, right: f64, bottom: f64, left: f64) -> Self {
        Padding {
            top,
            right,
            bottom,
            left,
        }
    }

    /// The offset from the bottom left corner, as AppKit counts, and the size of the area
    /// inside the padding.
    pub fn inset(&self, screen: Size) -> ((f64, f64), Size) {
        let percent = |value: f64, of: f64| value / 100.0 * of;
        (
            (
                percent(self.left, screen.width),
                percent(self.bottom, screen.height),
            ),
            Size {
                width: screen.width - percent(self.left + self.right, screen.width),
                height: screen.height - percent(self.top + self.bottom, screen.height),
            },
        )
    }
}

impl std::str::FromStr for Padding {
    type Err = String;

    fn from_str(spec: &str) -> Result<Self, Self::Err> {
        let values = spec
            .split(',')
            .map(|v| match v.trim().parse::<f64>() {
                Ok(v) if (0.0..50.0).contains(&v) => Ok(v),
                _ => Err(format!("\"{v}\" is not a percentage from 0 to below 50")),
            })
            .collect::<Result<Vec<_>, _>>()?;
        match values[..] {
            [all] => Ok(Padding::new(all, all, all, all)),
            [vertical, horizontal] => Ok(Padding::new(vertical, horizontal, vertical, horizontal)),
            [top, right, bottom, left] => Ok(Padding::new(top, right, bottom, left)),
            _ => Err("expected 1, 2 or 4 comma-separated values".into()),
        }
    }
}

/// Factor by which something of size `content` grows to be as large as fits on `screen`.
/// `quarter_turns` is the rotation in 90° steps, as with sm's `-r`.
pub fn fit_scale(content: Size, screen: Size, quarter_turns: u8) -> f64 {
    let rotated = rotate(content, quarter_turns);
    f64::min(screen.width / rotated.width, screen.height / rotated.height)
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
    fn parses_padding_shorthands() {
        assert_eq!("5".parse(), Ok(Padding::new(5.0, 5.0, 5.0, 5.0)));
        assert_eq!("5,10".parse(), Ok(Padding::new(5.0, 10.0, 5.0, 10.0)));
        assert_eq!("1,2,3,4".parse(), Ok(Padding::new(1.0, 2.0, 3.0, 4.0)));
        assert!("1,2,3".parse::<Padding>().is_err());
        assert!("50".parse::<Padding>().is_err());
        assert!("-1".parse::<Padding>().is_err());
        assert!("x".parse::<Padding>().is_err());
    }

    #[test]
    fn padding_shrinks_the_screen_by_percentages() {
        // Top and bottom are relative to the height, left and right to the width.
        let pad = Padding::new(10.0, 5.0, 20.0, 15.0);
        assert_eq!(
            pad.inset(size(1000.0, 500.0)),
            ((150.0, 100.0), size(800.0, 350.0))
        );
    }

    #[test]
    fn fits_the_tighter_dimension() {
        assert_eq!(fit_scale(size(200.0, 100.0), size(1000.0, 1000.0), 0), 5.0);
        assert_eq!(fit_scale(size(100.0, 100.0), size(1000.0, 300.0), 0), 3.0);
    }

    #[test]
    fn rotation_swaps_dimensions() {
        assert_eq!(fit_scale(size(200.0, 100.0), size(1000.0, 500.0), 1), 2.5);
        assert_eq!(fit_scale(size(200.0, 100.0), size(1000.0, 500.0), 2), 5.0);
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
