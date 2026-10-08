//! Placeholders like `{clock}` that are replaced by live values when the text is shown.

use jiff::{SignedDuration, Timestamp, Zoned, civil};

pub const CLOCK: &str = "{clock}";
pub const COUNTDOWN: &str = "{countdown}";

#[derive(Debug, Clone, PartialEq)]
pub struct Countdown {
    pub end: Timestamp,
    /// Replaces the whole text once the countdown reaches zero.
    pub zero: Option<String>,
}

/// Replaces placeholders with their values at `now`. A backslash before a placeholder
/// keeps it as typed.
pub fn fill(source: &str, now: &Zoned, countdown: Option<&Countdown>) -> String {
    let mut text = replace(source, CLOCK, &now.strftime("%H:%M").to_string());
    if let Some(countdown) = countdown {
        // Partial seconds round up, so the countdown shows 0:00 only once it is over.
        let left = countdown
            .end
            .duration_since(now.timestamp())
            .as_secs_f64()
            .ceil()
            .max(0.0) as i64;
        if left == 0
            && let Some(zero) = &countdown.zero
        {
            return fill(zero, now, None);
        }
        text = replace(&text, COUNTDOWN, &format_seconds(left));
    }
    text
}

fn replace(text: &str, placeholder: &str, value: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(i) = rest.find(placeholder) {
        result.push_str(&rest[..i]);
        if rest[..i].ends_with('\\') {
            result.push_str(placeholder);
        } else {
            result.push_str(value);
        }
        rest = &rest[i + placeholder.len()..];
    }
    result.push_str(rest);
    result
}

fn format_seconds(seconds: i64) -> String {
    let (days, hours, minutes, seconds) = (
        seconds / 86400,
        seconds / 3600 % 24,
        seconds / 60 % 60,
        seconds % 60,
    );
    match (days, hours) {
        (0, 0) => format!("{minutes}:{seconds:02}"),
        (0, _) => format!("{hours}:{minutes:02}:{seconds:02}"),
        _ => format!("{days}d {hours:02}:{minutes:02}:{seconds:02}"),
    }
}

/// Parses a countdown length such as `90` (seconds), `5m` or `1h30m`.
pub fn parse_duration(spec: &str) -> Result<SignedDuration, String> {
    let duration = match spec.parse::<u64>() {
        Ok(seconds) => SignedDuration::from_secs(seconds as i64),
        Err(_) => spec
            .parse::<SignedDuration>()
            .map_err(|_| format!("\"{spec}\" is not a length like 90, 5m or 1h30m"))?,
    };
    if duration.is_negative() {
        return Err("the length cannot be negative".into());
    }
    Ok(duration)
}

/// Parses an end time such as `18:00`, `2026-12-31` or `2026-12-31T23:59` in local time.
/// A time of day without a date means its next occurrence.
pub fn resolve_until(spec: &str, now: &Zoned) -> Result<Timestamp, String> {
    // Time parsing also accepts a full date and time and drops the date, so it goes last.
    let zoned = if let Ok(datetime) = spec.parse::<civil::DateTime>() {
        datetime.to_zoned(now.time_zone().clone())
    } else if let Ok(date) = spec.parse::<civil::Date>() {
        date.to_zoned(now.time_zone().clone())
    } else if let Ok(time) = spec.parse::<civil::Time>() {
        let today = now
            .date()
            .to_datetime(time)
            .to_zoned(now.time_zone().clone());
        let today = today.map_err(|e| e.to_string())?;
        if today > *now {
            Ok(today)
        } else {
            today.tomorrow()
        }
    } else {
        return Err(format!(
            "\"{spec}\" is not a time like 18:00, 2026-12-31 or 2026-12-31T23:59"
        ));
    };
    zoned.map(|z| z.timestamp()).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(datetime: &str) -> Zoned {
        format!("{datetime}[Europe/Berlin]").parse().unwrap()
    }

    fn countdown(end: &str, zero: Option<&str>) -> Countdown {
        Countdown {
            end: at(end).timestamp(),
            zero: zero.map(Into::into),
        }
    }

    #[test]
    fn fills_the_clock() {
        assert_eq!(
            fill("It is {clock}.", &at("2026-10-08T09:05:59"), None),
            "It is 09:05."
        );
    }

    #[test]
    fn formats_the_remaining_time() {
        let c = countdown("2026-10-08T12:00:00", None);
        let left = |now: &str| fill(COUNTDOWN, &at(now), Some(&c));
        // Partial seconds round up, so a 5 minute timer starts at 5:00 and ends at 0:00.
        assert_eq!(left("2026-10-08T11:55:00"), "5:00");
        assert_eq!(left("2026-10-08T11:59:59.5"), "0:01");
        assert_eq!(left("2026-10-08T10:57:57"), "1:02:03");
        assert_eq!(left("2026-10-06T09:00:00"), "2d 03:00:00");
        assert_eq!(left("2026-10-08T12:00:00"), "0:00");
        assert_eq!(left("2026-10-08T13:00:00"), "0:00");
    }

    #[test]
    fn replaces_the_text_at_zero() {
        let c = countdown("2026-10-08T12:00:00", Some("**Done!**"));
        assert_eq!(
            fill("Ends in {countdown}", &at("2026-10-08T11:00:00"), Some(&c)),
            "Ends in 1:00:00"
        );
        assert_eq!(
            fill("Ends in {countdown}", &at("2026-10-08T12:00:00"), Some(&c)),
            "**Done!**"
        );
    }

    #[test]
    fn leaves_unavailable_and_escaped_placeholders() {
        let now = at("2026-10-08T09:05:00");
        assert_eq!(
            fill("{countdown} {other}", &now, None),
            "{countdown} {other}"
        );
        assert_eq!(fill(r"\{clock} {clock}", &now, None), r"\{clock} 09:05");
    }

    #[test]
    fn parses_durations() {
        assert_eq!(parse_duration("90"), Ok(SignedDuration::from_secs(90)));
        assert_eq!(parse_duration("5m"), Ok(SignedDuration::from_mins(5)));
        assert_eq!(
            parse_duration("1h30m"),
            Ok(SignedDuration::from_secs(90 * 60))
        );
        assert!(parse_duration("soon").is_err());
        assert!(parse_duration("-5m").is_err());
    }

    #[test]
    fn resolves_end_times() {
        let now = at("2026-10-08T17:00:00");
        let resolve = |spec| resolve_until(spec, &now).map(|t| t.to_zoned(now.time_zone().clone()));
        assert_eq!(resolve("18:00"), Ok(at("2026-10-08T18:00:00")));
        assert_eq!(resolve("16:30"), Ok(at("2026-10-09T16:30:00")));
        assert_eq!(resolve("2026-12-31"), Ok(at("2026-12-31T00:00:00")));
        assert_eq!(resolve("2026-12-31T23:59"), Ok(at("2026-12-31T23:59:00")));
        assert!(resolve("tomorrow").is_err());
    }
}
