//! Images to show instead of the text.

use objc2::AnyThread;
use objc2::rc::Retained;
use objc2_app_kit::{NSImage, NSPasteboard, NSPasteboardTypeString};
use objc2_foundation::NSString;

/// The image on the clipboard, unless it also holds text. Apps like Keynote put an image
/// of copied text next to the text itself.
pub fn pasted() -> Option<Retained<NSImage>> {
    let pasteboard = NSPasteboard::generalPasteboard();
    if pasteboard
        .stringForType(unsafe { NSPasteboardTypeString })
        .is_some()
    {
        return None;
    }
    NSImage::initWithPasteboard(NSImage::alloc(), &pasteboard).filter(has_size)
}

/// The image file at `path`, if it can be read.
pub fn open(path: &str) -> Option<Retained<NSImage>> {
    NSImage::initWithContentsOfFile(NSImage::alloc(), &NSString::from_str(path)).filter(has_size)
}

/// Images without a size cannot be scaled to fit the screen.
fn has_size(image: &Retained<NSImage>) -> bool {
    image.size().width > 0.0 && image.size().height > 0.0
}
