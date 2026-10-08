//! QR codes, encoded as a grid of modules (the small squares a code is made of) and drawn
//! as an image.

use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::Bool;
use objc2_app_kit::{NSBezierPath, NSColor, NSColorSpace, NSImage};
use objc2_foundation::{NSPoint, NSRect, NSSize};
use qrcode::{Color, QrCode};

/// `text` as a QR code in the given colors, drawn at whatever size it is shown so it stays
/// sharp. Fails if the text is too long for a QR code.
pub fn image(
    text: &str,
    foreground: Retained<NSColor>,
    background: Retained<NSColor>,
) -> Option<Retained<NSImage>> {
    let modules = Modules::encode(text)?;
    // The light border scanners need around the code, in modules.
    const QUIET_ZONE: usize = 4;
    let side = (modules.width + 2 * QUIET_ZONE) as f64;
    let draw = RcBlock::new(move |rect: NSRect| -> Bool {
        // Many scanners only read dark modules on light, so the colors may swap. They
        // resolve here, as the appearance can change while squint runs.
        let (mut dark, mut light) = (&foreground, &background);
        if luminance(dark) > luminance(light) {
            std::mem::swap(&mut dark, &mut light);
        }
        light.setFill();
        NSBezierPath::fillRect(rect);
        // One path for all modules, so neighbors join without antialiased seams.
        let path = NSBezierPath::bezierPath();
        for y in 0..modules.width {
            for x in 0..modules.width {
                if modules.is_dark(x, y) {
                    path.appendBezierPathWithRect(NSRect::new(
                        NSPoint::new((x + QUIET_ZONE) as f64, (y + QUIET_ZONE) as f64),
                        NSSize::new(1.0, 1.0),
                    ));
                }
            }
        }
        dark.setFill();
        path.fill();
        Bool::YES
    });
    Some(NSImage::imageWithSize_flipped_drawingHandler(
        NSSize::new(side, side),
        true,
        &draw,
    ))
}

fn luminance(color: &NSColor) -> f64 {
    color
        .colorUsingColorSpace(&NSColorSpace::sRGBColorSpace())
        .map_or(0.0, |c| {
            0.2126 * c.redComponent() + 0.7152 * c.greenComponent() + 0.0722 * c.blueComponent()
        })
}

struct Modules {
    /// Number of modules per side.
    width: usize,
    dark: Vec<bool>,
}

impl Modules {
    /// Encodes `text`, or fails if it is too long for a QR code.
    fn encode(text: &str) -> Option<Self> {
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
    fn is_dark(&self, x: usize, y: usize) -> bool {
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
