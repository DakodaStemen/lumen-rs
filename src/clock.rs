//! Time formatting and natural ordering of file names.

/// Formats milliseconds as `m:ss`, or `h:mm:ss` from one hour up.
pub fn clock(ms: i64) -> String {
    let seconds = (ms / 1000).max(0);
    let (hours, rest) = (seconds / 3600, seconds % 3600);
    let (minutes, seconds) = (rest / 60, rest % 60);
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
}

/// One piece of a name split into digit runs and text, so `ep2` sorts before `ep10`.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum NaturalPart {
    Number(u128),
    Text(String),
}

pub fn natural_key(name: &str) -> Vec<NaturalPart> {
    let mut parts = Vec::new();
    let mut run = String::new();
    let mut digits = false;
    let mut flush = |run: &mut String, digits: bool| {
        if run.is_empty() {
            return;
        }
        parts.push(if digits {
            NaturalPart::Number(run.parse().unwrap_or(u128::MAX))
        } else {
            NaturalPart::Text(run.to_lowercase())
        });
        run.clear();
    };
    for c in name.chars() {
        if c.is_ascii_digit() != digits {
            flush(&mut run, digits);
            digits = c.is_ascii_digit();
        }
        run.push(c);
    }
    flush(&mut run, digits);
    parts
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_like_lumen() {
        assert_eq!(clock(0), "0:00");
        assert_eq!(clock(-5), "0:00");
        assert_eq!(clock(59_999), "0:59");
        assert_eq!(clock(61_000), "1:01");
        assert_eq!(clock(3_600_000), "1:00:00");
        assert_eq!(clock(3_725_000), "1:02:05");
    }

    #[test]
    fn sorts_numbers_by_value_and_ignores_case() {
        let mut names = ["ep10.mkv", "Ep2.mkv", "ep1.mkv"];
        names.sort_by_key(|n| natural_key(n));
        assert_eq!(names, ["ep1.mkv", "Ep2.mkv", "ep10.mkv"]);
        assert!(natural_key("ep2.mp4") < natural_key("ep10.mp4"));
    }
}
