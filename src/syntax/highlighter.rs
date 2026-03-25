//! Syntax highlighting engine using tree-sitter reconstruct-then-parse pipeline.
//!
//! Reconstructs full source files from diff lines, parses with tree-sitter,
//! and maps byte ranges back to per-line highlight spans for GPUI's StyledText.

use std::collections::HashMap;
use std::ops::Range;
use std::sync::Arc;

use gpui::HighlightStyle;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

use super::languages::Language;
use super::theme::{HIGHLIGHT_NAMES, style_for_highlight};
use crate::git::types::{DiffLineType, FileDiff};

/// Maximum content size for highlighting (500KB). Content exceeding this
/// threshold is skipped to maintain frame budget. Tree-sitter can parse
/// ~1MB in ~10ms, but we leave headroom for the mapping pass.
const MAX_HIGHLIGHT_BYTES: usize = 500 * 1024;

/// Syntax highlighter with lazy-cached language configurations.
///
/// Maintains a `Highlighter` instance and a cache of `HighlightConfiguration`
/// per language. Configurations are created on first use and cached for reuse.
pub struct SyntaxHighlighter {
    highlighter: Highlighter,
    configs: HashMap<Language, Arc<HighlightConfiguration>>,
}

impl SyntaxHighlighter {
    /// Create a new SyntaxHighlighter with empty configuration cache.
    pub fn new() -> Self {
        Self {
            highlighter: Highlighter::new(),
            configs: HashMap::new(),
        }
    }

    /// Get or create the HighlightConfiguration for a language.
    /// Returns None if the language's grammar fails to initialize.
    fn get_or_create_config(&mut self, lang: Language) -> Option<Arc<HighlightConfiguration>> {
        if let Some(config) = self.configs.get(&lang) {
            return Some(Arc::clone(config));
        }

        match lang.create_highlight_config() {
            Ok(config) => {
                let arc = Arc::new(config);
                self.configs.insert(lang, Arc::clone(&arc));
                Some(arc)
            }
            Err(e) => {
                eprintln!(
                    "Warning: failed to create highlight config for {:?}: {}",
                    lang, e
                );
                None
            }
        }
    }

    /// Highlight a source string, returning per-line highlight spans.
    ///
    /// Takes source code and a language, returns a Vec where each entry
    /// corresponds to one line and contains the highlight spans for that line.
    pub fn highlight_file(
        &mut self,
        source: &str,
        lang: Option<Language>,
    ) -> Vec<Vec<(Range<usize>, HighlightStyle)>> {
        if source.is_empty() {
            return Vec::new();
        }

        let lang = match lang {
            Some(l) => l,
            None => {
                // Unknown language: return empty highlights for each line
                let line_count = source.lines().count();
                return vec![Vec::new(); line_count];
            }
        };

        let config = match self.get_or_create_config(lang) {
            Some(c) => c,
            None => {
                let line_count = source.lines().count();
                return vec![Vec::new(); line_count];
            }
        };

        // Build line offset table
        let mut line_offsets: Vec<(usize, usize)> = Vec::new();
        let mut pos = 0;
        for line in source.split('\n') {
            let start = pos;
            let end = pos + line.len();
            line_offsets.push((start, end));
            pos = end + 1; // skip the \n
        }

        // Run highlighting
        let events = match self
            .highlighter
            .highlight(&config, source.as_bytes(), None, |_| None)
        {
            Ok(events) => events,
            Err(_) => {
                return vec![Vec::new(); line_offsets.len()];
            }
        };

        let mut result: Vec<Vec<(Range<usize>, HighlightStyle)>> =
            vec![Vec::new(); line_offsets.len()];
        let mut style_stack: Vec<usize> = Vec::new();

        for event in events {
            match event {
                Ok(HighlightEvent::Source { start, end }) => {
                    if let Some(&idx) = style_stack.last() {
                        let style = style_for_highlight(idx);
                        if style.color.is_some() {
                            map_range_to_lines(start, end, &line_offsets, style, &mut result);
                        }
                    }
                }
                Ok(HighlightEvent::HighlightStart(h)) => {
                    style_stack.push(h.0);
                }
                Ok(HighlightEvent::HighlightEnd) => {
                    style_stack.pop();
                }
                Err(_) => break,
            }
        }

        result
    }

    /// Highlight a FileDiff, returning per-row highlight spans.
    ///
    /// The returned Vec is indexed by flat row position (matching the order
    /// of DiffRows produced by flatten). HunkHeader rows get empty highlights.
    /// Add lines use new-side highlights, Remove lines use old-side highlights,
    /// Context lines use new-side highlights.
    pub fn highlight_diff(
        &mut self,
        file_diff: &FileDiff,
    ) -> Vec<Vec<(Range<usize>, HighlightStyle)>> {
        // Detect language from file extension
        let ext = file_diff.path.rsplit('.').next().unwrap_or("");
        let lang = Language::from_extension(ext);

        // Count total flat rows
        let total_rows: usize = file_diff
            .hunks
            .iter()
            .map(|h| 1 + h.lines.len()) // 1 for hunk header + lines
            .sum();

        if lang.is_none() {
            return vec![Vec::new(); total_rows];
        }
        let lang = lang.unwrap();

        let config = match self.get_or_create_config(lang) {
            Some(c) => c,
            None => return vec![Vec::new(); total_rows],
        };

        // Reconstruct sides
        let (new_side, new_offsets, old_side, old_offsets) = reconstruct_sides(file_diff);

        // Size guard
        if new_side.len() > MAX_HIGHLIGHT_BYTES || old_side.len() > MAX_HIGHLIGHT_BYTES {
            return vec![Vec::new(); total_rows];
        }

        // Highlight both sides
        let new_highlights =
            highlight_source(&mut self.highlighter, &config, &new_side, &new_offsets);
        let old_highlights =
            highlight_source(&mut self.highlighter, &config, &old_side, &old_offsets);

        // Merge: build result indexed by flat row
        let mut result: Vec<Vec<(Range<usize>, HighlightStyle)>> = vec![Vec::new(); total_rows];

        for (flat_idx, highlights) in &new_highlights {
            if *flat_idx < result.len() {
                result[*flat_idx] = highlights.clone();
            }
        }
        // Old-side highlights override for Remove lines only
        for (flat_idx, highlights) in &old_highlights {
            if *flat_idx < result.len() {
                result[*flat_idx] = highlights.clone();
            }
        }

        result
    }
}

/// Reconstruct new-side and old-side source strings from a FileDiff.
///
/// Returns (new_side_text, new_line_offsets, old_side_text, old_line_offsets).
/// Each offset entry is (byte_start, byte_end, flat_row_index).
fn reconstruct_sides(
    file_diff: &FileDiff,
) -> (
    String,
    Vec<(usize, usize, usize)>,
    String,
    Vec<(usize, usize, usize)>,
) {
    let mut new_side = String::new();
    let mut new_offsets: Vec<(usize, usize, usize)> = Vec::new();
    let mut old_side = String::new();
    let mut old_offsets: Vec<(usize, usize, usize)> = Vec::new();

    let mut prev_new_lineno: Option<u32> = None;
    let mut prev_old_lineno: Option<u32> = None;
    let mut flat_idx: usize = 0;

    for hunk in &file_diff.hunks {
        // HunkHeader occupies one flat row
        flat_idx += 1;

        for line in &hunk.lines {
            match line.line_type {
                DiffLineType::Add | DiffLineType::Context => {
                    // Insert gap marker at hunk boundaries (non-contiguous lines)
                    if let (Some(prev), Some(curr)) = (prev_new_lineno, line.new_lineno) {
                        if curr > prev + 1 {
                            new_side.push('\n');
                        }
                    }
                    let start = new_side.len();
                    new_side.push_str(&line.content);
                    let end = new_side.len();
                    new_offsets.push((start, end, flat_idx));
                    new_side.push('\n');
                    prev_new_lineno = line.new_lineno;
                }
                DiffLineType::Remove => {}
                DiffLineType::HunkHeader => {}
            }

            match line.line_type {
                DiffLineType::Remove | DiffLineType::Context => {
                    if let (Some(prev), Some(curr)) = (prev_old_lineno, line.old_lineno) {
                        if curr > prev + 1 {
                            old_side.push('\n');
                        }
                    }
                    let start = old_side.len();
                    old_side.push_str(&line.content);
                    let end = old_side.len();
                    old_offsets.push((start, end, flat_idx));
                    old_side.push('\n');
                    prev_old_lineno = line.old_lineno;
                }
                DiffLineType::Add => {}
                DiffLineType::HunkHeader => {}
            }

            flat_idx += 1;
        }
    }

    (new_side, new_offsets, old_side, old_offsets)
}

/// Highlight a source string and map byte ranges back to per-line spans.
///
/// Returns a HashMap keyed by flat_row_index, each containing the highlights
/// for that row with line-local byte offsets.
fn highlight_source(
    highlighter: &mut Highlighter,
    config: &HighlightConfiguration,
    source: &str,
    line_offsets: &[(usize, usize, usize)],
) -> HashMap<usize, Vec<(Range<usize>, HighlightStyle)>> {
    let mut result: HashMap<usize, Vec<(Range<usize>, HighlightStyle)>> = HashMap::new();

    if source.is_empty() || line_offsets.is_empty() {
        return result;
    }

    let events = match highlighter.highlight(config, source.as_bytes(), None, |_| None) {
        Ok(events) => events,
        Err(_) => return result,
    };

    let mut style_stack: Vec<usize> = Vec::new();

    for event in events {
        match event {
            Ok(HighlightEvent::Source { start, end }) => {
                if let Some(&idx) = style_stack.last() {
                    let style = style_for_highlight(idx);
                    if style.color.is_none() {
                        continue;
                    }
                    // Find overlapping lines via binary search (offsets are sorted by start)
                    let first = line_offsets.partition_point(|&(_, line_end, _)| line_end <= start);
                    for &(line_start, line_end, flat_idx) in &line_offsets[first..] {
                        if end <= line_start {
                            break; // past the highlight range
                        }
                        // Compute line-local range
                        let local_start = start.saturating_sub(line_start);
                        let local_end = if end < line_end {
                            end - line_start
                        } else {
                            line_end - line_start
                        };
                        if local_start < local_end {
                            result
                                .entry(flat_idx)
                                .or_default()
                                .push((local_start..local_end, style));
                        }
                    }
                }
            }
            Ok(HighlightEvent::HighlightStart(h)) => {
                style_stack.push(h.0);
            }
            Ok(HighlightEvent::HighlightEnd) => {
                style_stack.pop();
            }
            Err(_) => break,
        }
    }

    result
}

/// Map a highlight range to per-line spans using binary search on the line offset table.
fn map_range_to_lines(
    start: usize,
    end: usize,
    line_offsets: &[(usize, usize)],
    style: HighlightStyle,
    result: &mut [Vec<(Range<usize>, HighlightStyle)>],
) {
    // Binary search: find first line whose end > start
    let first = line_offsets.partition_point(|&(_, line_end)| line_end <= start);
    for (line_idx, &(line_start, line_end)) in line_offsets[first..].iter().enumerate() {
        let actual_idx = first + line_idx;
        if end <= line_start {
            break; // past the highlight range
        }
        let local_start = start.saturating_sub(line_start);
        let local_end = if end < line_end {
            end - line_start
        } else {
            line_end - line_start
        };
        if local_start < local_end && actual_idx < result.len() {
            result[actual_idx].push((local_start..local_end, style));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::types::{DiffHunk, DiffLine, DiffLineType, FileDiff};

    #[test]
    fn test_highlight_file_rust_returns_non_empty() {
        let mut hl = SyntaxHighlighter::new();
        let source = "fn main() { let x = 42; }";
        let spans = hl.highlight_file(source, Some(Language::Rust));
        assert!(!spans.is_empty(), "Should return at least one line");
        // At least one line should have highlights (keywords, numbers, etc.)
        let has_highlights = spans.iter().any(|line| !line.is_empty());
        assert!(has_highlights, "Rust code should produce some highlights");
    }

    #[test]
    fn test_highlight_file_empty_input() {
        let mut hl = SyntaxHighlighter::new();
        let spans = hl.highlight_file("", Some(Language::Rust));
        assert!(spans.is_empty(), "Empty input should return empty Vec");
    }

    #[test]
    fn test_highlight_file_unknown_language() {
        let mut hl = SyntaxHighlighter::new();
        let spans = hl.highlight_file("some content here", None);
        assert!(!spans.is_empty(), "Should return entries for each line");
        assert!(
            spans.iter().all(|line| line.is_empty()),
            "Unknown language should produce empty highlights for all lines"
        );
    }

    #[test]
    fn test_reconstruct_new_side() {
        let file_diff = FileDiff {
            path: "test.rs".to_string(),
            additions: 1,
            deletions: 1,
            hunks: vec![DiffHunk {
                header: "@@ -1,2 +1,2 @@".to_string(),
                lines: vec![
                    DiffLine {
                        line_type: DiffLineType::Context,
                        content: "use std::io;".to_string(),
                        old_lineno: Some(1),
                        new_lineno: Some(1),
                    },
                    DiffLine {
                        line_type: DiffLineType::Remove,
                        content: "fn old() {}".to_string(),
                        old_lineno: Some(2),
                        new_lineno: None,
                    },
                    DiffLine {
                        line_type: DiffLineType::Add,
                        content: "fn new() {}".to_string(),
                        old_lineno: None,
                        new_lineno: Some(2),
                    },
                ],
            }],
        };

        let (new_side, new_offsets, old_side, old_offsets) = reconstruct_sides(&file_diff);

        // New side should contain Context + Add lines
        assert!(new_side.contains("use std::io;"));
        assert!(new_side.contains("fn new() {}"));
        assert!(!new_side.contains("fn old() {}"));
        assert_eq!(new_offsets.len(), 2); // context + add

        // Old side should contain Context + Remove lines
        assert!(old_side.contains("use std::io;"));
        assert!(old_side.contains("fn old() {}"));
        assert!(!old_side.contains("fn new() {}"));
        assert_eq!(old_offsets.len(), 2); // context + remove
    }

    #[test]
    fn test_reconstruct_old_side() {
        let file_diff = FileDiff {
            path: "test.rs".to_string(),
            additions: 0,
            deletions: 2,
            hunks: vec![DiffHunk {
                header: "@@ -1,3 +1,1 @@".to_string(),
                lines: vec![
                    DiffLine {
                        line_type: DiffLineType::Context,
                        content: "line1".to_string(),
                        old_lineno: Some(1),
                        new_lineno: Some(1),
                    },
                    DiffLine {
                        line_type: DiffLineType::Remove,
                        content: "line2".to_string(),
                        old_lineno: Some(2),
                        new_lineno: None,
                    },
                    DiffLine {
                        line_type: DiffLineType::Remove,
                        content: "line3".to_string(),
                        old_lineno: Some(3),
                        new_lineno: None,
                    },
                ],
            }],
        };

        let (_new_side, _new_offsets, old_side, old_offsets) = reconstruct_sides(&file_diff);
        assert!(old_side.contains("line1"));
        assert!(old_side.contains("line2"));
        assert!(old_side.contains("line3"));
        assert_eq!(old_offsets.len(), 3);
    }

    #[test]
    fn test_line_byte_offsets_mapping() {
        let mut hl = SyntaxHighlighter::new();
        // Multi-line source with known structure
        let source = "fn main() {\n    let x = 42;\n}";
        let spans = hl.highlight_file(source, Some(Language::Rust));
        assert_eq!(spans.len(), 3, "Should have 3 lines");

        // Line 0: "fn main() {" - should have highlights for "fn" keyword
        // Verify ranges are within line bounds
        for (line_idx, line_spans) in spans.iter().enumerate() {
            let line_content = source.split('\n').nth(line_idx).unwrap();
            for (range, _style) in line_spans {
                assert!(
                    range.end <= line_content.len(),
                    "Range {:?} exceeds line {} length {} (content: {:?})",
                    range,
                    line_idx,
                    line_content.len(),
                    line_content
                );
            }
        }
    }

    #[test]
    fn test_multibyte_utf8_no_panic() {
        let mut hl = SyntaxHighlighter::new();
        let source = "let emoji = \"hello\";\nlet cjk = \"test\";";
        // Should not panic with multi-byte content
        let spans = hl.highlight_file(source, Some(Language::Rust));
        assert!(!spans.is_empty());
    }

    #[test]
    fn test_highlight_diff_rust_file() {
        let mut hl = SyntaxHighlighter::new();
        let file_diff = FileDiff {
            path: "src/main.rs".to_string(),
            additions: 1,
            deletions: 0,
            hunks: vec![DiffHunk {
                header: "@@ -0,0 +1 @@".to_string(),
                lines: vec![DiffLine {
                    line_type: DiffLineType::Add,
                    content: "fn main() { let x = 42; }".to_string(),
                    old_lineno: None,
                    new_lineno: Some(1),
                }],
            }],
        };

        let highlights = hl.highlight_diff(&file_diff);
        // 1 hunk header + 1 line = 2 rows
        assert_eq!(highlights.len(), 2);
        // Hunk header should have no highlights
        assert!(highlights[0].is_empty());
        // The Rust line should have some highlights
        assert!(
            !highlights[1].is_empty(),
            "Rust code should produce highlights"
        );
    }

    #[test]
    fn test_highlight_diff_unknown_extension() {
        let mut hl = SyntaxHighlighter::new();
        let file_diff = FileDiff {
            path: "Makefile.weird_ext_xyz".to_string(),
            additions: 1,
            deletions: 0,
            hunks: vec![DiffHunk {
                header: "@@ -0,0 +1 @@".to_string(),
                lines: vec![DiffLine {
                    line_type: DiffLineType::Add,
                    content: "hello world".to_string(),
                    old_lineno: None,
                    new_lineno: Some(1),
                }],
            }],
        };

        let highlights = hl.highlight_diff(&file_diff);
        assert_eq!(highlights.len(), 2);
        // All rows should have empty highlights for unknown language
        assert!(highlights.iter().all(|h| h.is_empty()));
    }

    #[test]
    fn test_size_guard() {
        let mut hl = SyntaxHighlighter::new();
        // Create content over 500KB
        let big_line = "fn func() { let x = 42; }\n".repeat(25_000); // ~650KB
        let file_diff = FileDiff {
            path: "big.rs".to_string(),
            additions: 25_000,
            deletions: 0,
            hunks: vec![DiffHunk {
                header: "@@ -0,0 +1,25000 @@".to_string(),
                lines: (1..=25_000)
                    .map(|i| DiffLine {
                        line_type: DiffLineType::Add,
                        content: format!("fn func_{}() {{ let x = {}; }}", i, i),
                        old_lineno: None,
                        new_lineno: Some(i as u32),
                    })
                    .collect(),
            }],
        };

        let highlights = hl.highlight_diff(&file_diff);
        // Should return vec of empty vecs due to size guard
        assert_eq!(highlights.len(), 1 + 25_000); // 1 header + 25000 lines
        assert!(
            highlights.iter().all(|h| h.is_empty()),
            "Size guard should produce empty highlights"
        );
    }

    #[test]
    fn test_performance_10k_lines() {
        let mut hl = SyntaxHighlighter::new();
        let lines: Vec<DiffLine> = (0..10_000)
            .map(|i| DiffLine {
                line_type: DiffLineType::Add,
                content: format!("fn func_{}() {{ let x = {}; }}", i, i),
                old_lineno: None,
                new_lineno: Some((i + 1) as u32),
            })
            .collect();

        let file_diff = FileDiff {
            path: "perf_test.rs".to_string(),
            additions: 10_000,
            deletions: 0,
            hunks: vec![DiffHunk {
                header: "@@ -0,0 +1,10000 @@".to_string(),
                lines,
            }],
        };

        let start = std::time::Instant::now();
        let highlights = hl.highlight_diff(&file_diff);
        let elapsed = start.elapsed();

        assert_eq!(highlights.len(), 1 + 10_000);
        // In release mode, 10K unique Rust functions highlights in ~100ms.
        // Use 200ms threshold to avoid flaky CI. Typical diffs (100-1000 lines)
        // complete in <10ms. Debug mode is ~50-100x slower due to unoptimized
        // tree-sitter, so we use a relaxed threshold (15000ms).
        let threshold_ms = if cfg!(debug_assertions) { 15000 } else { 200 };
        assert!(
            elapsed.as_millis() < threshold_ms,
            "10K lines should highlight in under {}ms, took {}ms",
            threshold_ms,
            elapsed.as_millis()
        );
    }

    #[test]
    fn test_gap_markers_at_hunk_boundaries() {
        let file_diff = FileDiff {
            path: "test.rs".to_string(),
            additions: 2,
            deletions: 0,
            hunks: vec![
                DiffHunk {
                    header: "@@ -1,1 +1,2 @@".to_string(),
                    lines: vec![
                        DiffLine {
                            line_type: DiffLineType::Context,
                            content: "line1".to_string(),
                            old_lineno: Some(1),
                            new_lineno: Some(1),
                        },
                        DiffLine {
                            line_type: DiffLineType::Add,
                            content: "line2".to_string(),
                            old_lineno: None,
                            new_lineno: Some(2),
                        },
                    ],
                },
                DiffHunk {
                    header: "@@ -10,1 +11,2 @@".to_string(),
                    lines: vec![
                        DiffLine {
                            line_type: DiffLineType::Context,
                            content: "line10".to_string(),
                            old_lineno: Some(10),
                            new_lineno: Some(11),
                        },
                        DiffLine {
                            line_type: DiffLineType::Add,
                            content: "line11".to_string(),
                            old_lineno: None,
                            new_lineno: Some(12),
                        },
                    ],
                },
            ],
        };

        let (new_side, _new_offsets, _old_side, _old_offsets) = reconstruct_sides(&file_diff);
        // Should contain gap marker (extra newline) between non-contiguous line numbers
        // Line 2 -> Line 11 is non-contiguous, so there should be a gap
        let lines: Vec<&str> = new_side.split('\n').collect();
        // Content should be: line1, line2, (gap), line10, line11, (trailing empty)
        assert!(
            lines.len() > 4,
            "Should have gap markers between non-contiguous hunks, got {} lines",
            lines.len()
        );
    }

    #[test]
    fn test_config_caching() {
        let mut hl = SyntaxHighlighter::new();
        // First call creates config
        let source = "fn main() {}";
        let _ = hl.highlight_file(source, Some(Language::Rust));
        assert!(hl.configs.contains_key(&Language::Rust));

        // Second call reuses cached config
        let _ = hl.highlight_file(source, Some(Language::Rust));
        assert_eq!(hl.configs.len(), 1);
    }

    #[test]
    fn test_highlight_diff_multi_hunk() {
        let mut hl = SyntaxHighlighter::new();
        let file_diff = FileDiff {
            path: "test.rs".to_string(),
            additions: 2,
            deletions: 1,
            hunks: vec![
                DiffHunk {
                    header: "@@ -1,2 +1,2 @@".to_string(),
                    lines: vec![
                        DiffLine {
                            line_type: DiffLineType::Context,
                            content: "use std::io;".to_string(),
                            old_lineno: Some(1),
                            new_lineno: Some(1),
                        },
                        DiffLine {
                            line_type: DiffLineType::Remove,
                            content: "fn old() {}".to_string(),
                            old_lineno: Some(2),
                            new_lineno: None,
                        },
                        DiffLine {
                            line_type: DiffLineType::Add,
                            content: "fn new_func() {}".to_string(),
                            old_lineno: None,
                            new_lineno: Some(2),
                        },
                    ],
                },
                DiffHunk {
                    header: "@@ -10,1 +10,2 @@".to_string(),
                    lines: vec![DiffLine {
                        line_type: DiffLineType::Add,
                        content: "let y = 100;".to_string(),
                        old_lineno: None,
                        new_lineno: Some(10),
                    }],
                },
            ],
        };

        let highlights = hl.highlight_diff(&file_diff);
        // 2 hunk headers + 4 lines = 6 rows
        assert_eq!(highlights.len(), 6);
    }
}
