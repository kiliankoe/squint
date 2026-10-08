//! The Markdown subset squint formats, kept free of AppKit so it can be tested.

use std::ops::Range;

use pulldown_cmark::{Event, HeadingLevel, Options, Parser, Tag};

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Style {
    pub bold: bool,
    pub italic: bool,
    pub strikethrough: bool,
    pub code: bool,
    /// Size relative to the body text, larger for headings.
    pub scale: f64,
}

pub const PLAIN: Style = Style {
    bold: false,
    italic: false,
    strikethrough: false,
    code: false,
    scale: 1.0,
};

/// A stretch of text in one style.
#[derive(Debug, Clone, PartialEq)]
pub struct Run {
    pub text: String,
    pub style: Style,
}

/// Formats bold, italic, strikethrough, code and `#`/`##` headings, removing their
/// markers. Everything else stays as typed, so the result reads like the source.
pub fn parse(source: &str) -> Vec<Run> {
    let mut styles = vec![PLAIN; source.len()];
    let mut hidden = vec![false; source.len()];
    let mut hide = |range: Range<usize>| hidden[range].fill(true);
    // Open elements with the source range their content spans so far.
    let mut open: Vec<(Tag, Range<usize>, Option<Range<usize>>)> = Vec::new();

    for (event, range) in Parser::new_ext(source, Options::ENABLE_STRIKETHROUGH).into_offset_iter()
    {
        if !matches!(event, Event::End(_))
            && let Some((_, _, content)) = open.last_mut()
        {
            let start = content.as_ref().map_or(range.start, |c| c.start);
            *content = Some(start..range.end);
        }
        match event {
            Event::Start(tag) => open.push((tag, range, None)),
            Event::End(_) => {
                let (tag, range, content) = open.pop().expect("events are balanced");
                if let Some(parent) = open.last_mut() {
                    let start = parent.2.as_ref().map_or(range.start, |c| c.start);
                    parent.2 = Some(start..range.end);
                }
                let Some(content) = content else { continue };
                let apply: fn(&mut Style) = match tag {
                    Tag::Strong => |s| s.bold = true,
                    Tag::Emphasis => |s| s.italic = true,
                    Tag::Strikethrough => |s| s.strikethrough = true,
                    // Setext headings (underlined with `---`) would surprise in plain text.
                    Tag::Heading { level, .. } if source[range.clone()].starts_with('#') => {
                        match level {
                            HeadingLevel::H1 => |s| {
                                s.bold = true;
                                s.scale = 2.0;
                            },
                            HeadingLevel::H2 => |s| {
                                s.bold = true;
                                s.scale = 1.5;
                            },
                            _ => continue,
                        }
                    }
                    _ => continue,
                };
                styles[content.clone()].iter_mut().for_each(apply);
                hide(range.start..content.start);
                // A heading's range includes the line break, which has to stay.
                let end = range.start + source[range.clone()].trim_end_matches('\n').len();
                hide(content.end..end);
            }
            Event::Code(code) => {
                // Code spanning lines has its line breaks turned into spaces, so it
                // cannot be found in the source and stays as typed.
                if let Some(offset) = source[range.clone()].find(&*code) {
                    let content = range.start + offset..range.start + offset + code.len();
                    styles[content.clone()]
                        .iter_mut()
                        .for_each(|s| s.code = true);
                    hide(range.start..content.start);
                    hide(content.end..range.end);
                }
            }
            // An escaped character is reported starting right after its backslash.
            Event::Text(_) if range.start > 0 && source.as_bytes()[range.start - 1] == b'\\' => {
                hide(range.start - 1..range.start);
            }
            // A backslash ending a line breaks it, which the line break already does.
            Event::HardBreak if source[range.clone()].starts_with('\\') => {
                hide(range.start..range.start + 1);
            }
            _ => {}
        }
    }

    let mut runs: Vec<Run> = Vec::new();
    for (i, c) in source.char_indices() {
        if hidden[i] {
            continue;
        }
        match runs.last_mut() {
            Some(run) if run.style == styles[i] => run.text.push(c),
            _ => runs.push(Run {
                text: c.to_string(),
                style: styles[i],
            }),
        }
    }
    runs
}

#[cfg(test)]
mod tests {
    use super::*;

    const BOLD: Style = Style {
        bold: true,
        ..PLAIN
    };
    const ITALIC: Style = Style {
        italic: true,
        ..PLAIN
    };

    fn run(text: &str, style: Style) -> Run {
        Run {
            text: text.into(),
            style,
        }
    }

    fn plain(text: &str) -> Vec<Run> {
        vec![run(text, PLAIN)]
    }

    #[test]
    fn leaves_plain_text_alone() {
        assert_eq!(parse(""), vec![]);
        assert_eq!(parse("Hello\n\nworld"), plain("Hello\n\nworld"));
        assert_eq!(parse("snake_case 5 * 3"), plain("snake_case 5 * 3"));
    }

    #[test]
    fn shows_unsupported_markdown_as_typed() {
        for source in [
            "- item\n1. item",
            "> quote",
            "### Three",
            "Title\n---",
            "[link](url)",
            "a &amp; b",
            "    indented",
        ] {
            assert_eq!(parse(source), plain(source));
        }
    }

    #[test]
    fn formats_inline_markers() {
        assert_eq!(
            parse("**Bold** and *it* or ~~gone~~"),
            vec![
                run("Bold", BOLD),
                run(" and ", PLAIN),
                run("it", ITALIC),
                run(" or ", PLAIN),
                run(
                    "gone",
                    Style {
                        strikethrough: true,
                        ..PLAIN
                    }
                ),
            ]
        );
    }

    #[test]
    fn nests_and_spans_lines() {
        assert_eq!(
            parse("***both***"),
            vec![run(
                "both",
                Style {
                    bold: true,
                    italic: true,
                    ..PLAIN
                }
            ),]
        );
        assert_eq!(parse("**two\nlines**"), vec![run("two\nlines", BOLD)]);
    }

    #[test]
    fn scales_headings() {
        assert_eq!(
            parse("# One\n## Two\nthree"),
            vec![
                run("One", Style { scale: 2.0, ..BOLD }),
                run("\n", PLAIN),
                run("Two", Style { scale: 1.5, ..BOLD }),
                run("\nthree", PLAIN),
            ]
        );
    }

    #[test]
    fn shows_code_verbatim() {
        let code = Style {
            code: true,
            ..PLAIN
        };
        assert_eq!(
            parse("pw: `x*y*`"),
            vec![run("pw: ", PLAIN), run("x*y*", code)]
        );
        assert_eq!(parse("`` a`b ``"), vec![run("a`b", code)]);
    }

    #[test]
    fn drops_escaping_backslashes() {
        assert_eq!(parse(r"\*not\* \\ a\b"), plain(r"*not* \ a\b"));
        // A backslash at the end of a line is a line break; before two spaces it is text.
        assert_eq!(parse("a\\\nb"), plain("a\nb"));
        assert_eq!(parse("a\\  \nb"), plain("a\\  \nb"));
    }
}
