//! Text preparation for paste-as-keystrokes.
//!
//! This module is deliberately pure and GUI-free: it normalizes text, applies the
//! paste-size cap, and computes metrics using the same US-layout HID mapper that
//! will later generate CH9329 keyboard reports.

use crate::protocol::hid::ascii_to_hid;
use zeroize::Zeroize;

/// Result counters for a paste operation.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct PasteStats {
    /// Normalized characters that have a HID mapping and were submitted.
    pub submitted: usize,
    /// Normalized characters retained by the cap but not mappable on the US HID
    /// text path.
    pub skipped: usize,
    /// Normalized characters dropped because they exceeded the paste cap.
    pub truncated: usize,
}

/// Paste submission outcome.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct PasteOutcome {
    /// Text classification counters.
    pub stats: PasteStats,
    /// Number of keyboard reports queued for this paste.
    pub reports: usize,
}

/// A normalized and capped paste payload plus its metrics.
#[derive(Clone, PartialEq, Eq, Debug)]
pub struct PreparedPaste {
    text: String,
    stats: PasteStats,
}

impl Drop for PreparedPaste {
    fn drop(&mut self) {
        self.text.zeroize();
    }
}

impl PreparedPaste {
    /// The normalized, capped text that should be typed.
    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    /// Metrics for this prepared paste.
    #[must_use]
    pub fn stats(&self) -> PasteStats {
        self.stats
    }
}

/// Normalizes line endings for keyboard injection.
///
/// The CH9329 text path maps `\n` to Enter, while `\r` has no HID mapping. Fold
/// CRLF and lone CR to one `\n` before counting or typing so Windows and classic
/// Mac line endings produce a single Enter.
#[must_use]
pub fn normalize_line_endings(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch == '\r' {
            if matches!(chars.peek(), Some('\n')) {
                let _ = chars.next();
            }
            out.push('\n');
        } else {
            out.push(ch);
        }
    }
    out
}

/// Normalizes `input`, caps it to `max_chars` normalized characters, and reports
/// how much text will be typed, skipped, or truncated.
///
/// The cap is measured in normalized characters after CRLF/CR -> LF folding and
/// before HID classification. `truncated` counts normalized characters dropped by
/// the cap; `skipped` counts retained characters without a US-layout HID mapping.
#[must_use]
pub fn prepare_paste(input: &str, max_chars: usize) -> PreparedPaste {
    let mut normalized = normalize_line_endings(input);
    let total = normalized.chars().count();
    let text: String = normalized.chars().take(max_chars).collect();
    let retained = text.chars().count();
    let mut stats = PasteStats {
        truncated: total.saturating_sub(retained),
        ..PasteStats::default()
    };
    for ch in text.chars() {
        if ascii_to_hid(ch).is_some() {
            stats.submitted += 1;
        } else {
            stats.skipped += 1;
        }
    }
    normalized.zeroize();
    PreparedPaste { text, stats }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crlf_and_lone_cr_normalize_to_single_lf() {
        assert_eq!(normalize_line_endings("a\r\nb\rc\n"), "a\nb\nc\n");
    }

    #[test]
    fn stats_count_after_normalization() {
        let paste = prepare_paste("a\r\n€\r", 99);
        assert_eq!(paste.text(), "a\n€\n");
        assert_eq!(
            paste.stats(),
            PasteStats {
                submitted: 3,
                skipped: 1,
                truncated: 0,
            }
        );
    }

    #[test]
    fn cap_is_measured_in_normalized_chars() {
        let exactly = prepare_paste("a\r\nb", 3);
        assert_eq!(exactly.text(), "a\nb");
        assert_eq!(exactly.stats().truncated, 0);

        let truncated = prepare_paste("a\r\nbc", 3);
        assert_eq!(truncated.text(), "a\nb");
        assert_eq!(
            truncated.stats(),
            PasteStats {
                submitted: 3,
                skipped: 0,
                truncated: 1,
            }
        );
    }

    #[test]
    fn lone_cr_at_cap_boundary_is_deterministic() {
        let paste = prepare_paste("ab\rc", 3);
        assert_eq!(paste.text(), "ab\n");
        assert_eq!(
            paste.stats(),
            PasteStats {
                submitted: 3,
                skipped: 0,
                truncated: 1,
            }
        );
    }

    #[test]
    fn skipped_uses_ascii_to_hid_source_of_truth() {
        let input = "A\t\n€\u{0}";
        let paste = prepare_paste(input, usize::MAX);
        let normalized = normalize_line_endings(input);
        let expected_submitted = normalized
            .chars()
            .filter(|&ch| ascii_to_hid(ch).is_some())
            .count();
        let expected_skipped = normalized.chars().count() - expected_submitted;
        assert_eq!(paste.stats().submitted, expected_submitted);
        assert_eq!(paste.stats().skipped, expected_skipped);
    }
}
