# Changelog

All notable changes to ADE (Advanced Developer Environment) are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/).

## [v1.7] — 2026-03-25 — Multiple Commit Selection

### Added
- Shift+Click contiguous commit range selection with fixed anchor model
- Shift+Up/Down arrow to extend selection one commit at a time
- Combined diff view across selected commit range via `diff_tree_to_tree(oldest_parent, newest)`
- "Showing changes from X commits" header bar replacing commit detail when range selected
- Aggregated file list showing union of changed files across range with combined +/- stats
- OID-based commit selection persistence across Cmd+G mode toggles
- Path-based file selection persistence across diff refreshes
- Range collapse fallback when anchor commit missing after refresh
- `prepare_highlights()` sort+clip+snap for safe multi-source highlight combining
- `collect_diff_data()` shared helper eliminating ~100 lines of diff collection duplication
- `snap_char_boundary()` helper preventing GPUI panics on multi-byte UTF-8 chars
- 37 new unit tests (339 total)

### Changed
- Modifier guard in `on_code_review_key_down` split to allow Shift+Up/Down through while blocking other Shift+keys
- `set_commits()` now preserves selection by OID lookup instead of resetting to index 0
- `set_diff()` now preserves file selection by path lookup instead of resetting to index 0
- File list header format changed from "Changed Files (N)" to "N changed files"

### Fixed
- GPUI panic on `str::split_at` with multi-byte UTF-8 characters (em dash) in diff content caused by unsorted/overlapping highlight ranges from combining syntax and intra-line highlights

## [v1.6] — 2026-03-25 — Syntax Highlighting

### Added
- Per-token syntax highlighting in diff lines via tree-sitter reconstruct-then-parse pipeline
- 16 language grammars: Rust, JavaScript, TypeScript, Python, Go, C, C++, Java, Ruby, Shell, HTML, CSS, JSON, YAML, Markdown
- GitHub Dark color theme for syntax tokens (17 highlight groups)
- Language detection from file extension
- Intra-line word-diff highlighting with LCS algorithm and darker background spans on changed tokens
- Language injection support for JS inside `<script>` and CSS inside `<style>` in HTML files
- `SyntaxHighlighter` with lazy grammar initialization and per-language `HighlightConfiguration` caching
- Size guard: files over 500KB skip highlighting to prevent performance issues
- 62 new unit tests (302 total)

### Changed
- Diff view lines rendered via GPUI `StyledText` with highlight spans instead of plain text
- `DiffRow::Line` extended with `highlights` and `intra_line_highlights` fields

## [v1.5] — 2026-03-25 — Changes/History Tabs

### Added
- Changes/History tab switcher in Code Review panel with blue underline indicator
- Cmd+1 (Changes) / Cmd+2 (History) keyboard shortcuts (Code Review mode only)
- Working tree diffs: uncommitted file list with status badges (M/A/D/?) and +N/-N stats
- StagingState enum: staged/unstaged dot indicators on changed files
- Auto-refresh: 2s timer polls working tree changes while in Code Review mode
- Refresh on mode entry: Cmd+G triggers immediate working tree file list update
- Path-based selection preservation: selected file stays selected across auto-refresh
- Tab memory: last active tab preserved across Cmd+G toggle cycles
- "Changes (N)" file count badge on tab header
- Colored diff stats in toolbar: green +N, yellow ~N, red -N next to branch name
- `compute_diff_stats()` for toolbar stat computation
- `changes_file_count()` and `changes_files_ref()` accessors on CodeReviewPanel
- `files_changed()` optimization to skip unnecessary diff re-fetches
- 27 new unit tests (240 total)

### Changed
- Tab bar replaces "Commits" header in Code Review panel
- Left panel width varies by tab: 280px (History), 240px (Changes)
- Mode-dependent Cmd+1-9 dispatch: Code Review intercepts 1/2, Terminal passes through

## [v1.4] — 2026-03-24 — Code Review Navigation

### Added
- Left/Right arrow panel switching with circular navigation (commits → files → diff → commits)
- Up/Down arrow navigation in commit list, file list, and diff view
- Auto-cascade: selecting a commit auto-selects first file and loads its diff
- Active/inactive panel selection highlighting (bright blue vs dimmed blue)
- Diff panel focus indicator (top accent bar)
- Selection persistence across panel focus changes
- Git history refresh on Code Review mode entry (Cmd+G)
- 23 new unit tests for navigation state and scroll behavior

### Fixed
- Diff scroll using `scroll_to_item_strict` for reliable line-by-line viewport control
- Diff scroll boundary capped using visible row count (no phantom scroll positions)

## [v1.3] — 2026-03-23 — Security Review Fixes

### Added
- Comprehensive 5-agent security audit (28 findings fixed)
- `sanitize_git_string()` for all git-sourced data (bidi/control char stripping)
- OSC 52 clipboard guard (opt-in via `ADE_ALLOW_OSC52=1`)
- Bracketed paste end-bracket stripping
- Division-by-zero guards for mouse coordinate functions
- Commit history cap at 50,000 commits

### Fixed
- Crash prevention: Option-returning pane accessors prevent panic on stale IDs
- Thread-safe home dir detection (`getpwuid_r` over `getpwuid`)
- Diff size limits (5K files, 500K lines, 10K chars/line)
- Terminal Drop sends `Msg::Shutdown` to prevent orphan PTYs
- Tab/pane creation limits (MAX_TABS=50, MAX_PANES=16)

### Removed
- `syntect` dependency (eliminated unmaintained `yaml-rust` transitive dep)
- SSH/HTTPS transport from `git2` (unused, removed via `default-features=false`)

## [v1.2] — 2026-03-23 — alacritty_terminal Migration

### Changed
- Migrated terminal backend from Ghostty to alacritty_terminal 0.25.1 (pure Rust, crates.io)
- Rewrote cell-based rendering via new `TerminalElement` GPUI Element
- Rewrote input handling via `TerminalView` entity with key encoding module
- Rewrote pane/tab integration for alacritty_terminal types

### Added
- `key_encode.rs` — pure Rust key-to-escape-sequence encoding for xterm-256color
- Double-click word selection, triple-click line selection
- Scroll-to-bottom on keyboard/paste input

### Removed
- Ghostty terminal backend and all Zig build dependencies

## [v1.1] — 2026-03-22 — App Packaging

### Added
- macOS .app bundle with Info.plist, app icon (.icns), and directory structure
- `scripts/bundle-macos.sh` — build script to produce Ade.app
- `scripts/create-dmg.sh` — DMG packaging with drag-to-install
- Finder-launch shell detection via `getpwuid` FFI with CWD fallback

## [v1.0] — 2026-03-22 — MVP

### Added
- Terminal embedding via alacritty_terminal with full PTY I/O
- Toggleable git history panel (Cmd+G) with commit log, file list, and unified diff
- Virtual scrolling for 100K+ commit repos (incremental batch loading)
- Line-type diff coloring (green/red/blue for additions/removals/hunk headers)
- Branch name and dirty/clean status in toolbar
- Git panel auto-updates on CWD change
- Vertical split (Cmd+D) and horizontal split (Cmd+Shift+D) with draggable dividers
- Pane navigation via keyboard (Cmd+Opt+Arrow) with active pane dimming
- Tabs: new (Cmd+T), close (Cmd+W), switch (Cmd+1-9, Cmd+Shift+[/])
- Tab titles from process name/CWD
- Native macOS look and feel with iTerm2-convention keybindings
- Clipboard copy/paste, mouse selection, scrollback
