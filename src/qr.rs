//! QR codes as a grid of modules, the small squares a code is made of.

use qrcode::{Color, QrCode};

pub struct Modules {
    /// Number of modules per side.
    pub width: usize,
    dark: Vec<bool>,
}

impl Modules {
    /// Encodes `text`, or fails if it is too long for a QR code.
    pub fn encode(text: &str) -> Option<Self> {
        let code = QrCode::new(text).ok()?;
        Some(Modules {
            width: code.width(),
            dark: code
                .into_colors()
                .into_iter()
                .map(|c| c == Color::Dark)
                .collect(),
        })
    }

    /// Whether the module in column `x` and row `y`, counted from the top left, is dark.
    pub fn is_dark(&self, x: usize, y: usize) -> bool {
        self.dark[y * self.width + x]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_text_with_finder_patterns_in_three_corners() {
        let qr = Modules::encode("https://example.com").unwrap();
        assert_eq!(qr.width, 25);
        let last = qr.width - 1;
        // Finder patterns have a dark outer ring and a light ring inside it.
        for (x, y) in [(0, 0), (last, 0), (0, last)] {
            assert!(qr.is_dark(x, y));
        }
        assert!(!qr.is_dark(1, 1));
        assert!(!qr.is_dark(last - 1, 1));
    }

    #[test]
    fn fails_for_too_much_text() {
        assert!(Modules::encode(&"x".repeat(8000)).is_none());
    }
}
