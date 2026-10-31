//! Reading the relay's settings from text, as an operator writes them.
//!
//! Strict on purpose: a typo in a retention setting should stop the relay at
//! startup, not quietly keep frames for a different length of time.

use std::time::Duration;

/// A duration such as `30d`, `24h`, `90m`, `45s` or `500ms`. A unit is
/// required: a bare number is ambiguous, and retention is a privacy setting.
#[must_use]
pub fn parse_duration(text: &str) -> Option<Duration> {
    let text = text.trim();
    let split = text.find(|c: char| !c.is_ascii_digit())?;
    let (number, unit) = text.split_at(split);
    let n: u64 = number.parse().ok()?;
    let seconds = |per: u64| n.checked_mul(per).map(Duration::from_secs);
    match unit {
        "ms" => Some(Duration::from_millis(n)),
        "s" => seconds(1),
        "m" => seconds(60),
        "h" => seconds(60 * 60),
        "d" => seconds(24 * 60 * 60),
        _ => None,
    }
}

/// A size such as `64MiB`, `512KiB`, `1GiB`, or a plain number of bytes.
#[must_use]
pub fn parse_size(text: &str) -> Option<u64> {
    let text = text.trim();
    let split = text
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(text.len());
    let (number, unit) = text.split_at(split);
    let n: u64 = number.parse().ok()?;
    let per = match unit {
        "" | "B" => 1,
        "KiB" => 1 << 10,
        "MiB" => 1 << 20,
        "GiB" => 1 << 30,
        _ => return None,
    };
    n.checked_mul(per)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn durations_parse_with_their_unit() {
        assert_eq!(
            parse_duration("30d"),
            Some(Duration::from_secs(30 * 86_400))
        );
        assert_eq!(parse_duration("24h"), Some(Duration::from_secs(86_400)));
        assert_eq!(parse_duration("90m"), Some(Duration::from_secs(5_400)));
        assert_eq!(parse_duration(" 45s "), Some(Duration::from_secs(45)));
        assert_eq!(parse_duration("500ms"), Some(Duration::from_millis(500)));
        for bad in [
            "30",
            "d",
            "30 d",
            "30days",
            "-1d",
            "1.5h",
            "",
            "99999999999999999999d",
        ] {
            assert_eq!(parse_duration(bad), None, "{bad:?}");
        }
    }

    #[test]
    fn sizes_parse_with_binary_units() {
        assert_eq!(parse_size("64MiB"), Some(64 << 20));
        assert_eq!(parse_size("512KiB"), Some(512 << 10));
        assert_eq!(parse_size("1GiB"), Some(1 << 30));
        assert_eq!(parse_size("1000"), Some(1000));
        for bad in ["64MB", "64 MiB", "MiB", "", "-5", "99999999999GiB"] {
            assert_eq!(parse_size(bad), None, "{bad:?}");
        }
    }
}
