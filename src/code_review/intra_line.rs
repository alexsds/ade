//! Intra-line change highlighting: word-diff algorithm and pair detection.
//!
//! Detects adjacent Remove/Add line pairs in diffs, computes word-level diffs
//! to find changed tokens, and populates intra_line_highlights with darker
//! background spans on those tokens.

use std::ops::Range;

use gpui::HighlightStyle;

use super::diff_view::DiffRow;
use crate::git::types::DiffLineType;
use crate::theme;

/// A token with its byte offset in the original string.
struct Token<'a> {
    text: &'a str,
    start: usize,
}

/// Tokenize a string into word and non-word tokens.
/// Word chars: alphanumeric + underscore. Non-word: everything else.
/// Concatenating all token texts reconstructs the original string.
fn tokenize(s: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let chars: Vec<(usize, char)> = s.char_indices().collect();
    let mut i = 0;
    while i < chars.len() {
        let (byte_pos, ch) = chars[i];
        let is_word = ch.is_alphanumeric() || ch == '_';
        let mut end = i + 1;
        while end < chars.len() {
            let (_, next_ch) = chars[end];
            let next_is_word = next_ch.is_alphanumeric() || next_ch == '_';
            if next_is_word != is_word {
                break;
            }
            end += 1;
        }
        let end_byte = if end < chars.len() {
            chars[end].0
        } else {
            s.len()
        };
        tokens.push(Token {
            text: &s[byte_pos..end_byte],
            start: byte_pos,
        });
        i = end;
    }
    tokens
}

/// Compute changed byte ranges between old and new lines using LCS word diff.
/// Returns (old_changed_ranges, new_changed_ranges).
/// If either string is empty, returns empty vecs (no useful intra-line diff).
fn word_diff(old_line: &str, new_line: &str) -> (Vec<Range<usize>>, Vec<Range<usize>>) {
    if old_line.is_empty() || new_line.is_empty() {
        return (vec![], vec![]);
    }

    let old_tokens = tokenize(old_line);
    let new_tokens = tokenize(new_line);

    let m = old_tokens.len();
    let n = new_tokens.len();

    // LCS DP table
    let mut dp = vec![vec![0u32; n + 1]; m + 1];
    for i in 1..=m {
        for j in 1..=n {
            dp[i][j] = if old_tokens[i - 1].text == new_tokens[j - 1].text {
                dp[i - 1][j - 1] + 1
            } else {
                dp[i - 1][j].max(dp[i][j - 1])
            };
        }
    }

    // Backtrack to find which tokens are in LCS
    let mut old_in_lcs = vec![false; m];
    let mut new_in_lcs = vec![false; n];
    let (mut i, mut j) = (m, n);
    while i > 0 && j > 0 {
        if old_tokens[i - 1].text == new_tokens[j - 1].text {
            old_in_lcs[i - 1] = true;
            new_in_lcs[j - 1] = true;
            i -= 1;
            j -= 1;
        } else if dp[i - 1][j] >= dp[i][j - 1] {
            i -= 1;
        } else {
            j -= 1;
        }
    }

    // Collect changed ranges (tokens NOT in LCS)
    let old_ranges = old_tokens
        .iter()
        .enumerate()
        .filter(|(idx, _)| !old_in_lcs[*idx])
        .map(|(_, t)| t.start..t.start + t.text.len())
        .collect();
    let new_ranges = new_tokens
        .iter()
        .enumerate()
        .filter(|(idx, _)| !new_in_lcs[*idx])
        .map(|(_, t)| t.start..t.start + t.text.len())
        .collect();

    (old_ranges, new_ranges)
}

/// Process DiffRows to detect adjacent Remove/Add pairs within each hunk,
/// compute word-level diffs, and populate intra_line_highlights on paired lines.
///
/// Pair detection operates per-hunk (not across HunkHeader boundaries).
/// Within each hunk, consecutive Remove lines followed by consecutive Add lines
/// are paired positionally: 1st remove with 1st add, etc.
/// Excess unpaired removes or adds get no intra-line highlights.
pub fn compute_intra_line_highlights(rows: &mut Vec<DiffRow>) {
    let t = theme::theme();
    // Collect hunk boundaries (indices of HunkHeader rows)
    let mut hunk_starts: Vec<usize> = Vec::new();
    for (i, row) in rows.iter().enumerate() {
        if matches!(row, DiffRow::HunkHeader(_)) {
            hunk_starts.push(i);
        }
    }

    // Process each hunk's lines independently
    let mut hunk_ranges: Vec<Range<usize>> = Vec::new();
    for (idx, &start) in hunk_starts.iter().enumerate() {
        let end = if idx + 1 < hunk_starts.len() {
            hunk_starts[idx + 1]
        } else {
            rows.len()
        };
        // Lines within this hunk start after the HunkHeader
        hunk_ranges.push(start + 1..end);
    }

    // Also handle rows that have no HunkHeader (edge case: all Line rows)
    if hunk_starts.is_empty() && !rows.is_empty() {
        hunk_ranges.push(0..rows.len());
    }

    // For each hunk, find Remove/Add pairs and compute word diffs
    // We collect pairs first, then apply highlights (to avoid borrow issues)
    let mut highlight_pairs: Vec<(usize, usize)> = Vec::new(); // (remove_idx, add_idx)

    for range in &hunk_ranges {
        let mut i = range.start;
        while i < range.end {
            // Look for consecutive Remove lines
            let remove_start = i;
            while i < range.end {
                if let DiffRow::Line {
                    line_type: DiffLineType::Remove,
                    ..
                } = &rows[i]
                {
                    i += 1;
                } else {
                    break;
                }
            }
            let remove_count = i - remove_start;

            if remove_count == 0 {
                i += 1;
                continue;
            }

            // Look for consecutive Add lines immediately after
            let add_start = i;
            while i < range.end {
                if let DiffRow::Line {
                    line_type: DiffLineType::Add,
                    ..
                } = &rows[i]
                {
                    i += 1;
                } else {
                    break;
                }
            }
            let add_count = i - add_start;

            if add_count == 0 {
                // No adds following removes -- no pairs
                continue;
            }

            // Pair min(remove_count, add_count) lines positionally
            let pair_count = remove_count.min(add_count);
            for p in 0..pair_count {
                highlight_pairs.push((remove_start + p, add_start + p));
            }
        }
    }

    // Now compute word diffs and apply highlights for each pair
    for (remove_idx, add_idx) in highlight_pairs {
        let (old_content, new_content) = {
            let old = if let DiffRow::Line { content, .. } = &rows[remove_idx] {
                content.clone()
            } else {
                continue;
            };
            let new = if let DiffRow::Line { content, .. } = &rows[add_idx] {
                content.clone()
            } else {
                continue;
            };
            (old, new)
        };

        let (old_ranges, new_ranges) = word_diff(&old_content, &new_content);

        // Apply darker red background to changed tokens on remove line
        if !old_ranges.is_empty() {
            if let DiffRow::Line {
                intra_line_highlights,
                ..
            } = &mut rows[remove_idx]
            {
                for range in old_ranges {
                    intra_line_highlights.push((
                        range,
                        HighlightStyle {
                            background_color: Some(t.colors.diff_remove_word_bg),
                            ..Default::default()
                        },
                    ));
                }
            }
        }

        // Apply darker green background to changed tokens on add line
        if !new_ranges.is_empty() {
            if let DiffRow::Line {
                intra_line_highlights,
                ..
            } = &mut rows[add_idx]
            {
                for range in new_ranges {
                    intra_line_highlights.push((
                        range,
                        HighlightStyle {
                            background_color: Some(t.colors.diff_add_word_bg),
                            ..Default::default()
                        },
                    ));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // === Tokenizer tests ===

    #[test]
    fn test_tokenize_mixed() {
        let tokens = tokenize("fn foo(bar)");
        let texts: Vec<&str> = tokens.iter().map(|t| t.text).collect();
        assert_eq!(texts, vec!["fn", " ", "foo", "(", "bar", ")"]);
        // Verify byte ranges reconstruct original
        let reconstructed: String = tokens.iter().map(|t| t.text).collect();
        assert_eq!(reconstructed, "fn foo(bar)");
    }

    #[test]
    fn test_tokenize_empty() {
        let tokens = tokenize("");
        assert!(tokens.is_empty());
    }

    #[test]
    fn test_tokenize_single_word() {
        let tokens = tokenize("hello");
        assert_eq!(tokens.len(), 1);
        assert_eq!(tokens[0].text, "hello");
        assert_eq!(tokens[0].start, 0);
    }

    #[test]
    fn test_tokenize_byte_ranges() {
        let tokens = tokenize("fn foo(bar)");
        // "fn" starts at 0, " " at 2, "foo" at 3, "(" at 6, "bar" at 7, ")" at 10
        assert_eq!(tokens[0].start, 0);
        assert_eq!(tokens[1].start, 2);
        assert_eq!(tokens[2].start, 3);
        assert_eq!(tokens[3].start, 6);
        assert_eq!(tokens[4].start, 7);
        assert_eq!(tokens[5].start, 10);
    }

    // === Word diff tests ===

    #[test]
    fn test_word_diff_single_change() {
        let (old_ranges, new_ranges) = word_diff("hello world", "hello earth");
        // "world" at bytes 6..11, "earth" at bytes 6..11
        assert_eq!(old_ranges.len(), 1);
        assert_eq!(old_ranges[0], 6..11);
        assert_eq!(new_ranges.len(), 1);
        assert_eq!(new_ranges[0], 6..11);
    }

    #[test]
    fn test_word_diff_function_rename() {
        let (old_ranges, new_ranges) = word_diff("fn old_func() {}", "fn new_func() {}");
        // Only "old_func" and "new_func" should differ
        assert_eq!(old_ranges.len(), 1);
        assert_eq!(new_ranges.len(), 1);
        assert_eq!(&"fn old_func() {}"[old_ranges[0].clone()], "old_func");
        assert_eq!(&"fn new_func() {}"[new_ranges[0].clone()], "new_func");
    }

    #[test]
    fn test_word_diff_identical() {
        let (old_ranges, new_ranges) = word_diff("identical", "identical");
        assert!(old_ranges.is_empty());
        assert!(new_ranges.is_empty());
    }

    #[test]
    fn test_word_diff_empty_old() {
        let (old_ranges, new_ranges) = word_diff("", "new content");
        assert!(old_ranges.is_empty());
        assert!(new_ranges.is_empty());
    }

    #[test]
    fn test_word_diff_empty_new() {
        let (old_ranges, new_ranges) = word_diff("old content", "");
        assert!(old_ranges.is_empty());
        assert!(new_ranges.is_empty());
    }

    #[test]
    fn test_word_diff_unicode() {
        let (old_ranges, new_ranges) = word_diff("hello cafe\u{0301}", "hello world");
        // cafe\u{0301} is "cafe" + combining accent = multi-byte
        // Both sides should have changed spans
        assert!(!old_ranges.is_empty());
        assert!(!new_ranges.is_empty());
        // Verify ranges are valid byte ranges
        let old_str = "hello cafe\u{0301}";
        for r in &old_ranges {
            let _ = &old_str[r.clone()]; // should not panic
        }
    }

    // === Pair detection tests ===

    fn make_line(line_type: DiffLineType, content: &str) -> DiffRow {
        DiffRow::Line {
            old_lineno: None,
            new_lineno: None,
            content: content.to_string(),
            line_type,
            highlights: vec![],
            intra_line_highlights: vec![],
        }
    }

    #[test]
    fn test_pair_detection_single_pair() {
        let mut rows = vec![
            DiffRow::HunkHeader("@@ -1,1 +1,1 @@".to_string()),
            make_line(DiffLineType::Remove, "fn old_func() {}"),
            make_line(DiffLineType::Add, "fn new_func() {}"),
        ];
        compute_intra_line_highlights(&mut rows);

        // Remove line should have intra-line highlights
        if let DiffRow::Line {
            intra_line_highlights,
            ..
        } = &rows[1]
        {
            assert!(
                !intra_line_highlights.is_empty(),
                "Remove line should have intra-line highlights"
            );
        }
        // Add line should have intra-line highlights
        if let DiffRow::Line {
            intra_line_highlights,
            ..
        } = &rows[2]
        {
            assert!(
                !intra_line_highlights.is_empty(),
                "Add line should have intra-line highlights"
            );
        }
    }

    #[test]
    fn test_pair_detection_unequal_counts() {
        // 3 Removes followed by 2 Adds: first 2 pair, 3rd Remove gets nothing
        let mut rows = vec![
            DiffRow::HunkHeader("@@ -1,3 +1,2 @@".to_string()),
            make_line(DiffLineType::Remove, "line one old"),
            make_line(DiffLineType::Remove, "line two old"),
            make_line(DiffLineType::Remove, "line three old"),
            make_line(DiffLineType::Add, "line one new"),
            make_line(DiffLineType::Add, "line two new"),
        ];
        compute_intra_line_highlights(&mut rows);

        // First 2 removes should be paired
        if let DiffRow::Line {
            intra_line_highlights,
            ..
        } = &rows[1]
        {
            assert!(
                !intra_line_highlights.is_empty(),
                "1st Remove should be paired"
            );
        }
        if let DiffRow::Line {
            intra_line_highlights,
            ..
        } = &rows[2]
        {
            assert!(
                !intra_line_highlights.is_empty(),
                "2nd Remove should be paired"
            );
        }
        // 3rd remove should NOT be paired
        if let DiffRow::Line {
            intra_line_highlights,
            ..
        } = &rows[3]
        {
            assert!(
                intra_line_highlights.is_empty(),
                "3rd Remove should NOT be paired"
            );
        }
    }

    #[test]
    fn test_pair_detection_standalone_remove() {
        // Remove followed by Context (not Add) -- no pairing
        let mut rows = vec![
            DiffRow::HunkHeader("@@ -1,2 +1,1 @@".to_string()),
            make_line(DiffLineType::Remove, "deleted line"),
            make_line(DiffLineType::Context, "context line"),
        ];
        compute_intra_line_highlights(&mut rows);

        if let DiffRow::Line {
            intra_line_highlights,
            ..
        } = &rows[1]
        {
            assert!(
                intra_line_highlights.is_empty(),
                "Standalone Remove should have no intra-line highlights"
            );
        }
    }

    #[test]
    fn test_pair_detection_no_cross_hunk() {
        // Hunk 1 ends with Remove, Hunk 2 starts with Add -- should NOT pair
        let mut rows = vec![
            DiffRow::HunkHeader("@@ -1,1 +1,0 @@".to_string()),
            make_line(DiffLineType::Remove, "removed in hunk 1"),
            DiffRow::HunkHeader("@@ -10,0 +10,1 @@".to_string()),
            make_line(DiffLineType::Add, "added in hunk 2"),
        ];
        compute_intra_line_highlights(&mut rows);

        if let DiffRow::Line {
            intra_line_highlights,
            ..
        } = &rows[1]
        {
            assert!(
                intra_line_highlights.is_empty(),
                "Remove in hunk 1 should NOT pair with Add in hunk 2"
            );
        }
        if let DiffRow::Line {
            intra_line_highlights,
            ..
        } = &rows[3]
        {
            assert!(
                intra_line_highlights.is_empty(),
                "Add in hunk 2 should NOT pair with Remove in hunk 1"
            );
        }
    }

    #[test]
    fn test_compute_highlights_populates_paired_leaves_unpaired() {
        let mut rows = vec![
            DiffRow::HunkHeader("@@ -1,3 +1,3 @@".to_string()),
            make_line(DiffLineType::Context, "unchanged"),
            make_line(DiffLineType::Remove, "fn old_func() {}"),
            make_line(DiffLineType::Add, "fn new_func() {}"),
        ];
        compute_intra_line_highlights(&mut rows);

        // Context line should have no highlights
        if let DiffRow::Line {
            intra_line_highlights,
            ..
        } = &rows[1]
        {
            assert!(
                intra_line_highlights.is_empty(),
                "Context line should have no highlights"
            );
        }
        // Remove and Add should have highlights
        if let DiffRow::Line {
            intra_line_highlights,
            ..
        } = &rows[2]
        {
            assert!(!intra_line_highlights.is_empty());
            // Verify it uses darker red background
            assert!(intra_line_highlights[0].1.background_color.is_some());
        }
        if let DiffRow::Line {
            intra_line_highlights,
            ..
        } = &rows[3]
        {
            assert!(!intra_line_highlights.is_empty());
            // Verify it uses darker green background
            assert!(intra_line_highlights[0].1.background_color.is_some());
        }
    }

    #[test]
    fn test_highlight_colors_correct() {
        let mut rows = vec![
            DiffRow::HunkHeader("@@".to_string()),
            make_line(DiffLineType::Remove, "old_word"),
            make_line(DiffLineType::Add, "new_word"),
        ];
        compute_intra_line_highlights(&mut rows);

        // Remove line: darker red = diff_remove_word_bg
        if let DiffRow::Line {
            intra_line_highlights,
            ..
        } = &rows[1]
        {
            assert!(!intra_line_highlights.is_empty());
            let bg = intra_line_highlights[0].1.background_color.unwrap();
            let expected = crate::theme::theme().colors.diff_remove_word_bg;
            assert_eq!(bg, expected);
        }

        // Add line: darker green = diff_add_word_bg
        if let DiffRow::Line {
            intra_line_highlights,
            ..
        } = &rows[2]
        {
            assert!(!intra_line_highlights.is_empty());
            let bg = intra_line_highlights[0].1.background_color.unwrap();
            let expected = crate::theme::theme().colors.diff_add_word_bg;
            assert_eq!(bg, expected);
        }
    }
}
