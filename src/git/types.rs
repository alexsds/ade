/// Strip control characters and Unicode bidi overrides from git-sourced strings.
/// Prevents UI spoofing via crafted commit messages, branch names, file paths, etc.
/// Removes: C0 control chars (except tab/newline), DEL, C1 control chars,
/// and Unicode bidirectional override characters.
pub fn sanitize_git_string(s: &str) -> String {
    s.chars()
        .filter(|&c| {
            // Allow tab and newline
            if c == '\t' || c == '\n' {
                return true;
            }
            // Strip C0 control characters (U+0000-U+001F) and DEL (U+007F)
            if c < ' ' || c == '\x7f' {
                return false;
            }
            // Strip C1 control characters (U+0080-U+009F)
            if ('\u{0080}'..='\u{009F}').contains(&c) {
                return false;
            }
            // Strip Unicode bidirectional overrides and isolates
            if matches!(
                c,
                '\u{200E}' // LEFT-TO-RIGHT MARK
                | '\u{200F}' // RIGHT-TO-LEFT MARK
                | '\u{202A}' // LEFT-TO-RIGHT EMBEDDING
                | '\u{202B}' // RIGHT-TO-LEFT EMBEDDING
                | '\u{202C}' // POP DIRECTIONAL FORMATTING
                | '\u{202D}' // LEFT-TO-RIGHT OVERRIDE
                | '\u{202E}' // RIGHT-TO-LEFT OVERRIDE
                | '\u{2066}' // LEFT-TO-RIGHT ISOLATE
                | '\u{2067}' // RIGHT-TO-LEFT ISOLATE
                | '\u{2068}' // FIRST STRONG ISOLATE
                | '\u{2069}' // POP DIRECTIONAL ISOLATE
            ) {
                return false;
            }
            true
        })
        .collect()
}

/// Branch/tag decoration for a commit
#[derive(Debug, Clone)]
pub enum Decoration {
    Branch { name: String },
    Tag { name: String },
    Head,
    RemoteBranch { name: String },
}

/// Single commit metadata
#[derive(Debug, Clone)]
pub struct CommitInfo {
    pub oid: String,          // hex string, e.g. "abc1234..."
    pub summary: String,      // first line of commit message
    pub body: Option<String>, // remaining commit message lines
    pub author_name: String,
    pub author_email: String,
    pub time_seconds: i64, // seconds since epoch (from git2::Time)
    pub time_offset: i32,  // UTC offset in minutes (from git2::Time)
    pub decorations: Vec<Decoration>,
    pub is_ahead: bool, // true if commit is ahead of upstream tracking branch
}

/// Status of the current branch
#[derive(Debug, Clone)]
pub struct BranchStatus {
    pub branch_name: String,
    pub is_dirty: bool,
}

/// Staging state for a working tree file change
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum StagingState {
    Staged,   // only INDEX_* flags set (green dot)
    Unstaged, // only WT_* flags set (orange dot)
    Partial,  // both INDEX_* and WT_* flags (split dot)
}

/// A changed file in a commit's diff
#[derive(Debug, Clone)]
pub struct FileChange {
    pub path: String,
    pub status_char: char, // 'A' added, 'M' modified, 'D' deleted, 'R' renamed, 'C' copied
    pub additions: u64,
    pub deletions: u64,
    pub staging_state: Option<StagingState>, // None for commit diffs, Some for working tree
}

/// Type of diff line
#[derive(Debug, Clone, PartialEq)]
pub enum DiffLineType {
    Context,
    Add,
    Remove,
    HunkHeader,
}

/// Single line in a diff hunk
#[derive(Debug, Clone)]
pub struct DiffLine {
    pub line_type: DiffLineType,
    pub content: String,
    pub old_lineno: Option<u32>,
    pub new_lineno: Option<u32>,
}

/// A diff hunk (section of changes)
#[derive(Debug, Clone)]
pub struct DiffHunk {
    pub header: String, // e.g. "@@ -10,5 +10,7 @@"
    pub lines: Vec<DiffLine>,
}

/// Full diff for a single file
#[derive(Debug, Clone)]
pub struct FileDiff {
    pub path: String,
    pub additions: u64,
    pub deletions: u64,
    pub hunks: Vec<DiffHunk>,
}

/// Complete diff data for a commit
#[derive(Debug, Clone)]
pub struct DiffData {
    pub files: Vec<FileChange>,
    pub file_diffs: Vec<FileDiff>,
}

/// Format a git timestamp as relative time ("3 hours ago", "2 days ago")
pub fn format_relative_time(seconds_since_epoch: i64, _offset_minutes: i32) -> String {
    let now = chrono::Utc::now().timestamp();
    let diff = now - seconds_since_epoch;

    // Handle future timestamps (crafted commits can have arbitrary dates)
    if diff < 0 {
        return "just now".to_string();
    }

    if diff < 60 {
        "just now".to_string()
    } else if diff < 3600 {
        let mins = diff / 60;
        if mins == 1 {
            "1 minute ago".to_string()
        } else {
            format!("{} minutes ago", mins)
        }
    } else if diff < 86400 {
        let hours = diff / 3600;
        if hours == 1 {
            "1 hour ago".to_string()
        } else {
            format!("{} hours ago", hours)
        }
    } else if diff < 2592000 {
        let days = diff / 86400;
        if days == 1 {
            "1 day ago".to_string()
        } else {
            format!("{} days ago", days)
        }
    } else if diff < 31536000 {
        let months = diff / 2592000;
        if months == 1 {
            "1 month ago".to_string()
        } else {
            format!("{} months ago", months)
        }
    } else {
        let years = diff / 31536000;
        if years == 1 {
            "1 year ago".to_string()
        } else {
            format!("{} years ago", years)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commit_info_construction() {
        let commit = CommitInfo {
            oid: "abc1234".to_string(),
            summary: "Initial commit".to_string(),
            body: Some("Extended description".to_string()),
            author_name: "Test Author".to_string(),
            author_email: "test@example.com".to_string(),
            time_seconds: 1700000000,
            time_offset: 0,
            decorations: vec![
                Decoration::Branch {
                    name: "main".to_string(),
                },
                Decoration::Tag {
                    name: "v1.0".to_string(),
                },
            ],
            is_ahead: false,
        };

        assert_eq!(commit.oid, "abc1234");
        assert_eq!(commit.summary, "Initial commit");
        assert_eq!(commit.body, Some("Extended description".to_string()));
        assert_eq!(commit.author_name, "Test Author");
        assert_eq!(commit.author_email, "test@example.com");
        assert_eq!(commit.time_seconds, 1700000000);
        assert_eq!(commit.time_offset, 0);
        assert_eq!(commit.decorations.len(), 2);
    }

    #[test]
    fn test_branch_status_construction() {
        let status = BranchStatus {
            branch_name: "main".to_string(),
            is_dirty: true,
        };
        assert_eq!(status.branch_name, "main");
        assert!(status.is_dirty);
    }

    #[test]
    fn test_staging_state_traits() {
        // StagingState must be Copy + Clone + PartialEq + Debug
        let staged = StagingState::Staged;
        let unstaged = StagingState::Unstaged;
        let partial = StagingState::Partial;

        // Copy
        let staged_copy = staged;
        assert_eq!(staged, staged_copy);

        // PartialEq
        assert_ne!(staged, unstaged);
        assert_ne!(unstaged, partial);
        assert_ne!(staged, partial);

        // Debug
        let debug_str = format!("{:?}", staged);
        assert!(debug_str.contains("Staged"));
    }

    #[test]
    fn test_file_change_with_staging_state() {
        // FileChange with Some(StagingState)
        let change_staged = FileChange {
            path: "src/main.rs".to_string(),
            status_char: 'M',
            additions: 10,
            deletions: 3,
            staging_state: Some(StagingState::Staged),
        };
        assert_eq!(change_staged.staging_state, Some(StagingState::Staged));

        // FileChange with None (commit diffs)
        let change_none = FileChange {
            path: "src/main.rs".to_string(),
            status_char: 'M',
            additions: 10,
            deletions: 3,
            staging_state: None,
        };
        assert_eq!(change_none.staging_state, None);
    }

    #[test]
    fn test_file_change_construction() {
        let change = FileChange {
            path: "src/main.rs".to_string(),
            status_char: 'M',
            additions: 10,
            deletions: 3,
            staging_state: None,
        };
        assert_eq!(change.path, "src/main.rs");
        assert_eq!(change.status_char, 'M');
        assert_eq!(change.additions, 10);
        assert_eq!(change.deletions, 3);
        assert_eq!(change.staging_state, None);
    }

    #[test]
    fn test_diff_line_types() {
        let add = DiffLine {
            line_type: DiffLineType::Add,
            content: "+new line".to_string(),
            old_lineno: None,
            new_lineno: Some(5),
        };
        assert_eq!(add.line_type, DiffLineType::Add);
        assert_eq!(add.old_lineno, None);
        assert_eq!(add.new_lineno, Some(5));

        let remove = DiffLine {
            line_type: DiffLineType::Remove,
            content: "-old line".to_string(),
            old_lineno: Some(3),
            new_lineno: None,
        };
        assert_eq!(remove.line_type, DiffLineType::Remove);
    }

    #[test]
    fn test_diff_data_construction() {
        let diff = DiffData {
            files: vec![FileChange {
                path: "hello.txt".to_string(),
                status_char: 'A',
                additions: 1,
                deletions: 0,
                staging_state: None,
            }],
            file_diffs: vec![FileDiff {
                path: "hello.txt".to_string(),
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
            }],
        };
        assert_eq!(diff.files.len(), 1);
        assert_eq!(diff.file_diffs.len(), 1);
        assert_eq!(diff.file_diffs[0].hunks.len(), 1);
        assert_eq!(diff.file_diffs[0].hunks[0].lines.len(), 1);
    }

    #[test]
    fn test_format_relative_time_just_now() {
        let now = chrono::Utc::now().timestamp();
        assert_eq!(format_relative_time(now, 0), "just now");
    }

    #[test]
    fn test_format_relative_time_minutes() {
        let now = chrono::Utc::now().timestamp();
        assert_eq!(format_relative_time(now - 60, 0), "1 minute ago");
        assert_eq!(format_relative_time(now - 300, 0), "5 minutes ago");
    }

    #[test]
    fn test_format_relative_time_hours() {
        let now = chrono::Utc::now().timestamp();
        assert_eq!(format_relative_time(now - 3600, 0), "1 hour ago");
        assert_eq!(format_relative_time(now - 10800, 0), "3 hours ago");
    }

    #[test]
    fn test_format_relative_time_days() {
        let now = chrono::Utc::now().timestamp();
        assert_eq!(format_relative_time(now - 86400, 0), "1 day ago");
        assert_eq!(format_relative_time(now - 172800, 0), "2 days ago");
    }

    #[test]
    fn test_format_relative_time_months() {
        let now = chrono::Utc::now().timestamp();
        assert_eq!(format_relative_time(now - 2592000, 0), "1 month ago");
    }

    #[test]
    fn test_format_relative_time_years() {
        let now = chrono::Utc::now().timestamp();
        assert_eq!(format_relative_time(now - 31536000, 0), "1 year ago");
        assert_eq!(format_relative_time(now - 63072000, 0), "2 years ago");
    }

    #[test]
    fn test_format_relative_time_future() {
        let now = chrono::Utc::now().timestamp();
        // Future timestamps should display "just now" (not negative)
        assert_eq!(format_relative_time(now + 3600, 0), "just now");
        assert_eq!(format_relative_time(now + 86400, 0), "just now");
    }

    #[test]
    fn test_sanitize_git_string_strips_control_chars() {
        assert_eq!(sanitize_git_string("hello\x01\x02world\x7f"), "helloworld");
    }

    #[test]
    fn test_sanitize_git_string_preserves_tab_newline() {
        assert_eq!(sanitize_git_string("hello\tworld\n"), "hello\tworld\n");
    }

    #[test]
    fn test_sanitize_git_string_strips_bidi() {
        // U+202E = RIGHT-TO-LEFT OVERRIDE
        assert_eq!(sanitize_git_string("hello\u{202E}world"), "helloworld");
        // U+200F = RIGHT-TO-LEFT MARK
        assert_eq!(sanitize_git_string("hello\u{200F}world"), "helloworld");
    }

    #[test]
    fn test_sanitize_git_string_strips_c1_control() {
        // U+0085 = NEXT LINE (C1 control)
        assert_eq!(sanitize_git_string("hello\u{0085}world"), "helloworld");
    }

    #[test]
    fn test_sanitize_git_string_preserves_normal_unicode() {
        assert_eq!(sanitize_git_string("日本語テスト"), "日本語テスト");
    }

    #[test]
    fn test_decoration_variants() {
        let branch = Decoration::Branch {
            name: "main".to_string(),
        };
        let tag = Decoration::Tag {
            name: "v1.0".to_string(),
        };

        match &branch {
            Decoration::Branch { name } => assert_eq!(name, "main"),
            _ => panic!("Expected Branch"),
        }
        match &tag {
            Decoration::Tag { name } => assert_eq!(name, "v1.0"),
            _ => panic!("Expected Tag"),
        }

        let head = Decoration::Head;
        match &head {
            Decoration::Head => {}
            _ => panic!("Expected Head"),
        }
        let remote = Decoration::RemoteBranch {
            name: "origin/main".to_string(),
        };
        match &remote {
            Decoration::RemoteBranch { name } => assert_eq!(name, "origin/main"),
            _ => panic!("Expected RemoteBranch"),
        }
    }
}
