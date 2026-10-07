//! sm-compatible stdin protocol: a form feed (`\f`) ends a frame and replaces the shown text.

#[derive(Default)]
pub struct Frames {
    pending: Vec<u8>,
}

impl Frames {
    /// Adds input and returns the newest frame it completed, if any.
    pub fn push(&mut self, bytes: &[u8]) -> Option<String> {
        self.pending.extend_from_slice(bytes);
        let last = self.pending.iter().rposition(|&b| b == FORM_FEED)?;
        let start = self.pending[..last]
            .iter()
            .rposition(|&b| b == FORM_FEED)
            .map_or(0, |i| i + 1);
        let frame = clean(&self.pending[start..last]);
        self.pending.drain(..=last);
        Some(frame)
    }

    /// Returns the unterminated rest of the input at EOF, if it holds any text.
    // sm shows the empty rest here and clears the screen. Keeping the last frame
    // instead is friendlier to producers that end every frame with a form feed.
    pub fn finish(self) -> Option<String> {
        Some(clean(&self.pending)).filter(|text| !text.is_empty())
    }
}

const FORM_FEED: u8 = 0x0c;

fn clean(frame: &[u8]) -> String {
    String::from_utf8_lossy(frame).trim_matches('\n').to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn without_form_feeds_the_whole_input_is_one_frame() {
        let mut frames = Frames::default();
        assert_eq!(frames.push(b"hello\nworld\n"), None);
        assert_eq!(frames.finish().as_deref(), Some("hello\nworld"));
    }

    #[test]
    fn shows_only_the_newest_completed_frame() {
        let mut frames = Frames::default();
        assert_eq!(frames.push(b"a\x0cb\x0c").as_deref(), Some("b"));
        assert_eq!(frames.finish(), None);
    }

    #[test]
    fn frames_span_chunks() {
        let mut frames = Frames::default();
        assert_eq!(frames.push(b"he"), None);
        assert_eq!(frames.push(b"llo\x0cwor").as_deref(), Some("hello"));
        assert_eq!(frames.finish().as_deref(), Some("wor"));
    }

    #[test]
    fn strips_surrounding_newlines() {
        let mut frames = Frames::default();
        assert_eq!(frames.push(b"\n\n12:00\n\n\x0c").as_deref(), Some("12:00"));
        assert_eq!(frames.push(b"\n"), None);
        assert_eq!(frames.finish(), None);
    }

    #[test]
    fn keeps_utf8_split_across_chunks() {
        let mut frames = Frames::default();
        assert_eq!(frames.push(&[0xC3]), None);
        assert_eq!(frames.push(&[0xA4, b'\x0c']).as_deref(), Some("ä"));
    }
}
