mod app;
mod layout;
mod stdin;

use clap::Parser;

use crate::layout::{Align, Padding};

#[derive(Parser, Debug)]
#[command(version)]
struct Cli {
    /// Text color, as a CSS color name or hex value [default: black in light mode, white in dark mode]
    #[arg(short, long, value_parser = parse_color)]
    foreground: Option<csscolorparser::Color>,

    /// Background color, as a CSS color name or hex value [default: white in light mode, black in dark mode]
    #[arg(short, long, value_parser = parse_color)]
    background: Option<csscolorparser::Color>,

    /// Swap foreground and background colors
    #[arg(short, long)]
    invert: bool,

    /// Font family, e.g. "Helvetica Neue"
    #[arg(short = 'n', long)]
    font: Option<String>,

    /// Rotation in quarter turns clockwise
    #[arg(short, long, default_value_t = 0, value_parser = clap::value_parser!(u8).range(0..=3))]
    rotate: u8,

    /// Alignment: 0 centered, 1 left, 2 right
    #[arg(short, long, default_value_t = 0, value_parser = clap::value_parser!(u8).range(0..=2))]
    align: u8,

    /// Space kept free at the screen edges, in percent of the screen height (top, bottom)
    /// and width (left, right). One value for all sides, two for vertical and horizontal,
    /// or four for top, right, bottom and left.
    #[arg(short, long, default_value = "3")]
    pad: Padding,

    /// Text to show. A single "-" reads it from stdin instead, where a form feed
    /// character (\f) replaces the shown text with what came before it.
    text: Vec<String>,
}

#[derive(Debug, PartialEq)]
pub enum Input {
    Text(String),
    Stdin,
}

#[derive(Debug, PartialEq)]
pub enum Color {
    /// Black in light mode, white in dark mode.
    Text,
    /// White in light mode, black in dark mode.
    Background,
    Custom(csscolorparser::Color),
}

#[derive(Debug)]
pub struct Config {
    pub input: Input,
    pub foreground: Color,
    pub background: Color,
    pub font: Option<String>,
    pub quarter_turns: u8,
    pub align: Align,
    pub padding: Padding,
}

fn parse_color(spec: &str) -> Result<csscolorparser::Color, String> {
    csscolorparser::parse(spec).map_err(|e| e.to_string())
}

impl From<Cli> for Config {
    fn from(cli: Cli) -> Self {
        let input = match cli.text.first().map(String::as_str) {
            None => Input::Text(String::new()),
            Some("-") => Input::Stdin,
            Some(_) => Input::Text(cli.text.join(" ")),
        };
        let foreground = cli.foreground.map_or(Color::Text, Color::Custom);
        let background = cli.background.map_or(Color::Background, Color::Custom);
        let (foreground, background) = if cli.invert {
            (background, foreground)
        } else {
            (foreground, background)
        };
        let align = match cli.align {
            1 => Align::Left,
            2 => Align::Right,
            _ => Align::Center,
        };
        Config {
            input,
            foreground,
            background,
            font: cli.font,
            quarter_turns: cli.rotate,
            align,
            padding: cli.pad,
        }
    }
}

fn main() {
    app::run(Config::from(Cli::parse()));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config(args: &[&str]) -> Config {
        Config::from(Cli::try_parse_from([&["squint"], args].concat()).unwrap())
    }

    #[test]
    fn joins_arguments_with_spaces() {
        assert_eq!(
            config(&["hello", "world"]).input,
            Input::Text("hello world".into())
        );
    }

    #[test]
    fn starts_empty_without_text() {
        assert_eq!(config(&[]).input, Input::Text(String::new()));
    }

    #[test]
    fn dash_reads_stdin() {
        assert_eq!(config(&["-"]).input, Input::Stdin);
        assert_eq!(config(&["--", "-"]).input, Input::Stdin);
    }

    #[test]
    fn defaults_follow_the_system_appearance() {
        let c = config(&[]);
        assert_eq!(c.foreground, Color::Text);
        assert_eq!(c.background, Color::Background);
    }

    #[test]
    fn invert_swaps_colors() {
        let c = config(&["-i", "-f", "red"]);
        assert_eq!(c.foreground, Color::Background);
        assert_eq!(c.background, Color::Custom(parse_color("red").unwrap()));
    }

    #[test]
    fn maps_alignment_numbers() {
        assert_eq!(config(&["-a", "1"]).align, Align::Left);
        assert_eq!(config(&["-a", "2"]).align, Align::Right);
    }

    #[test]
    fn pads_by_default() {
        assert_eq!(config(&[]).padding, Padding::new(3.0, 3.0, 3.0, 3.0));
        assert_eq!(config(&["-p", "0"]).padding, Padding::new(0.0, 0.0, 0.0, 0.0));
    }

    #[test]
    fn rejects_invalid_values() {
        assert!(Cli::try_parse_from(["squint", "-f", "notacolor"]).is_err());
        assert!(Cli::try_parse_from(["squint", "-r", "4"]).is_err());
        assert!(Cli::try_parse_from(["squint", "-a", "3"]).is_err());
        assert!(Cli::try_parse_from(["squint", "-p", "1,2,3"]).is_err());
    }
}
