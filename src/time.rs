use regex::Regex;
use std::sync::OnceLock;

/// Mismo patrón que la app Python: HH:MM:SS(.ms), MM:SS(.ms) o segundos sueltos.
fn time_re() -> &'static Regex {
    static RE: OnceLock<Regex> = OnceLock::new();
    RE.get_or_init(|| {
        Regex::new(r"^(?:\d{1,2}:)?\d{1,2}:\d{2}(?:\.\d+)?$|^\d+(?:\.\d+)?$").unwrap()
    })
}

pub fn is_valid_time(t: &str) -> bool {
    time_re().is_match(t.trim())
}

/// Convierte "HH:MM:SS.ms", "MM:SS" o "SS" a segundos.
pub fn time_to_seconds(t: &str) -> Option<f64> {
    let t = t.trim();
    if t.is_empty() || !is_valid_time(t) {
        return None;
    }
    let mut parts: Vec<f64> = t
        .split(':')
        .map(|p| p.parse::<f64>().ok())
        .collect::<Option<Vec<_>>>()?;
    while parts.len() < 3 {
        parts.insert(0, 0.0);
    }
    let (h, m, s) = (parts[0], parts[1], parts[2]);
    Some(h * 3600.0 + m * 60.0 + s)
}

/// Formatea segundos como HH:MM:SS.
pub fn fmt_seconds(secs: f64) -> String {
    let secs = secs.max(0.0) as u64;
    let h = secs / 3600;
    let rem = secs % 3600;
    let m = rem / 60;
    let s = rem % 60;
    format!("{h:02}:{m:02}:{s:02}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_times() {
        assert_eq!(time_to_seconds("90"), Some(90.0));
        assert_eq!(time_to_seconds("90.5"), Some(90.5));
        assert_eq!(time_to_seconds("1:30"), Some(90.0));
        assert_eq!(time_to_seconds("01:02:03"), Some(3723.0));
        assert_eq!(time_to_seconds("1:02:03.5"), Some(3723.5));
        assert_eq!(time_to_seconds(""), None);
        assert_eq!(time_to_seconds("abc"), None);
        assert_eq!(time_to_seconds("1:2"), None);
    }

    #[test]
    fn format_times() {
        assert_eq!(fmt_seconds(90.0), "00:01:30");
        assert_eq!(fmt_seconds(3723.0), "01:02:03");
        assert_eq!(fmt_seconds(-5.0), "00:00:00");
    }
}
