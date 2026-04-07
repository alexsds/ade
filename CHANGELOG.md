# Changelog

All notable changes to ADE (Advanced Developer Environment) are documented here.
Format follows [Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

## [v2.4] - Light and Dark Mode

### Added
- Light color palette alongside existing dark theme with full ThemeColors coverage (53 fields)
- Theme setting in settings modal: Dark, Light, or System mode
- macOS system appearance detection — "System" mode follows OS dark/light preference automatically
- Terminal colors adapt to active theme: background, foreground, cursor, selection, and all 16 ANSI colors
- Cmd+Shift+T keyboard shortcut to toggle between dark and light theme
- Light-appropriate syntax highlighting colors for diff view (14 token colors)

### Changed
- Theme system upgraded from static LazyLock to runtime-switchable AtomicU8 dispatch with GPUI Global
- Theme preference persists to ~/.config/ade/settings.json and restores on app restart
- Switching themes triggers instant repaint across all UI surfaces including terminal scrollback

## [v2.3] - Usability Improvements

### Added
- Settings modal (Cmd+,) with external editor dropdown — persists to ~/.config/ade/settings.json
- External editor integration: double-click a file in code review to open it in VS Code, Zed, Sublime Text, Cursor, IntelliJ IDEA, Nova, Xcode, or macOS default
- MRU (most recently used) pane activation — closing a pane returns focus to the previously active pane instead of first-in-tree-order
- Terminal selection clearing on keypress, plain click, and after Cmd+C copy (respects alt-screen mode for TUI apps)
- Chevron-down and check SVG icons for settings dropdown

### Changed
- Selection highlight colors updated to indigo accent at 16% opacity (active) and 8% opacity (inactive) for clearer visual hierarchy
- Commit list author and relative time split into separate left/right aligned elements (author truncates, time always visible)
- All external editors use macOS `open -a` for reliable launch without requiring CLI tools in PATH
- Pane divider resize cursor stays locked during entire drag operation (no flickering)
### Fixed
- Terminal text selection no longer persists after typing, arrow keys, or clicking in normal mode
- Pane divider cursor no longer flickers when dragging past the 8px hit area
- Double-click to open file in editor now works when launched via `open Ade.app`

## [v2.2] - Redesign

### Added
- Zinc/Indigo color palette replacing Midnight Workshop amber/blue theme
- 4-tier background hierarchy (base, panel, surface, elevated) with new bg_panel token
- Color-coded decoration badges: green (branch), indigo (HEAD), yellow (tag), blue (remote)
- SVG icon system via compile-time AssetSource with 6 Lucide icons
- Icons in tab labels (file-diff, clock), toggle button (terminal, columns-2), and branch badge (git-branch)
- Shift+Enter sends newline (LF) in terminal, matching iTerm2 behavior

### Changed
- Accent color changed from amber-gold (#e5a100) to indigo (#818CF8)
- Text colors updated to zinc gray hierarchy (#FAFAFA / #A1A1AA / #52525B)
- Diff colors updated to emerald green (#34D399) and rose (#FB7185)
- Border colors updated to zinc tones (#27272A default, #1E1E22 subtle)
- Selected items use purple-tinted background (#1E1B2E) with indigo left accent
- Toolbar branch indicator restyled as green-tinted pill with status dot
- Toolbar toggle button restyled with darker fill, visible border, and 8px radius

### Removed
- Commit search field (search is not a supported feature)

## [v2.1] - Fixes & Refinements

### Added
- Image preview in diff view for image files (PNG, JPEG, etc.) in both History and Changes tabs
- Unpushed commit indicator (up-arrow badge) in history tab with upstream tracking
- File path text selection in diff panel headers with Cmd+C copy
- Clickable URLs in terminal with Cmd+hover underline and Cmd+click to open
- OSC 8 hyperlink support for CLI tools that emit explicit links
- URL scheme allowlist (http, https, ftp, ssh, mailto) blocking dangerous schemes
- Toggle button label now shows destination mode ("Terminal" or "Code Review")

### Fixed
- Diff selection coordinates now match between rendering and click targeting
- Terminal panes properly resize to fill available space after closing a pane
- Terminal panes resize correctly when toggling Code Review mode off
- Enter and Ctrl+C reliably sent to PTY in interactive prompts (IME hardening)

### Changed
- New panes and tabs open in the same directory as the active terminal

## [v2.0] - UI Levelup

### Added
- New "Midnight Workshop" dark theme with blue-tinted backgrounds and amber-gold accents
- Layered background depth across panels for clearer visual hierarchy
- Hover feedback on all interactive elements: commits, files, tabs, and buttons
- Accent border on selected commit row for quick visual tracking
- Pill-shaped branch and tag badges on commits
- Pill-shaped colored stat badges (+N / -N) in the metadata bar
- Subtle inset row separators in commit and file lists
- Full-width hunk header bars in diff view
- Helpful empty-state messages with keyboard hints in all panels

### Changed
- Consistent spacing, sizing, and typography across the entire UI
- Toolbar branch name now bolder with accent color
- Active tab highlighted with accent underline; inactive tabs dimmed
- Tab close button only appears on hover
- Review tab labels brighten on hover
- Diff panel shows a clear border when focused

## [v1.9] - TUI Mouse/Scroll

### Added
- Mouse support for TUI apps (click, drag, scroll in apps like vim, htop, etc.)
- Momentum scroll filtering to prevent trackpad overshoot in TUI apps

### Fixed
- Mouse clicks and scrolling now correctly reach TUI applications
- macOS natural scrolling direction works properly in TUI and alt-screen apps

## [v1.8] - UX Polish

### Added
- Shortened current directory path in toolbar (fish-style)
- Diff stats (+N ~N -N) visible in toolbar at all times, not just Code Review mode
- Copy commit hash button on each commit row
- Smooth trackpad scrolling in terminal
- Drag-to-select text in diffs and commit descriptions
- Cmd+C copies from whichever area has an active selection
- Fixed metadata bar showing author, hash, and change stats for the selected commit
- Copy button with green checkmark confirmation feedback

### Changed
- Git info hidden in toolbar when not inside a git repo
- Commit detail area streamlined — metadata moved to a dedicated fixed bar

## [v1.7] - Multiple Commit Selection

### Added
- Shift+Click to select a range of commits
- Shift+Up/Down to extend the selection one commit at a time
- Combined diff view showing all changes across the selected range
- Aggregated file list with combined stats for multi-commit selections
- Selection preserved when toggling between Terminal and Code Review modes

### Changed
- File and commit selections persist across refreshes instead of resetting

### Fixed
- Crash when diff content contained multi-byte characters (e.g., em dashes)

## [v1.6] - Syntax Highlighting

### Added
- Syntax highlighting in diffs for 16 languages (Rust, JS, TS, Python, Go, C/C++, Java, Ruby, Shell, HTML, CSS, JSON, YAML, Markdown)
- Word-level change highlighting within modified lines
- Language auto-detection from file extension
- JS/CSS highlighting inside HTML `<script>` and `<style>` blocks
- Large files (>500KB) gracefully skip highlighting to stay fast

## [v1.5] - Changes/History Tabs

### Added
- Changes / History tab switcher in Code Review panel
- Cmd+1 (Changes) / Cmd+2 (History) keyboard shortcuts
- Uncommitted changes view with file status badges (M/A/D/?) and +/- stats
- Staged/unstaged indicators on changed files
- Auto-refresh of working tree changes every 2 seconds
- File count badge on the Changes tab header
- Colored diff stats in toolbar (green/yellow/red) next to branch name

### Changed
- Last active tab remembered when toggling Code Review on and off
- Selected file preserved across auto-refreshes

## [v1.4] - Code Review Navigation

### Added
- Arrow key navigation across all Code Review panels (commits, files, diff)
- Left/Right arrows cycle between panels; Up/Down scrolls within them
- Selecting a commit automatically loads the first file's diff
- Active panel highlighted in bright blue; inactive panels dimmed
- Git history refreshes when entering Code Review mode

### Fixed
- Diff scrolling now tracks line-by-line without jumping past the end

## [v1.3] - Security Hardening

### Added
- Control and bidirectional character stripping on all git-sourced text
- Clipboard access from terminal programs requires opt-in (`ADE_ALLOW_OSC52=1`)
- Commit history capped at 50,000 to prevent memory issues

### Fixed
- Several crash scenarios involving stale pane references and edge-case inputs
- Orphan terminal processes no longer left behind when closing tabs
- Diff view capped to safe limits for very large changesets

### Removed
- Unused SSH/HTTPS transport and unmaintained transitive dependencies

## [v1.2] - Terminal Backend Upgrade

### Changed
- Terminal engine replaced with alacritty_terminal for better compatibility and simpler builds

### Added
- Double-click to select a word, triple-click to select a line
- Terminal auto-scrolls to bottom on keyboard input or paste

### Removed
- Ghostty backend and Zig build dependency

## [v1.1] - App Packaging

### Added
- Native macOS .app bundle with app icon
- DMG installer with drag-to-Applications
- Correct shell detection when launched from Finder

## [v1.0] - MVP

### Added
- GPU-accelerated terminal with full shell integration
- Toggleable git history panel (Cmd+G) with commit log, file list, and unified diff
- Handles repos with 100K+ commits via virtual scrolling
- Color-coded diffs: green for additions, red for deletions, blue for hunk headers
- Branch name and dirty/clean indicator in toolbar
- Git panel auto-updates when you change directories
- Split panes: vertical (Cmd+D) and horizontal (Cmd+Shift+D) with draggable dividers
- Keyboard pane navigation (Cmd+Opt+Arrow)
- Tabs: new (Cmd+T), close (Cmd+W), switch (Cmd+1-9)
- Clipboard, mouse selection, and scrollback
