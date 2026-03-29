# ADE (Advanced Developer Environment)

A GPU-accelerated macOS terminal with a built-in git code review panel. Press **Cmd+G** to toggle a GitHub Desktop-style view with commit history, file changes, and syntax-highlighted diffs — right next to the code you're writing.

Built in Rust with [GPUI](https://github.com/zed-industries/zed) (Zed's UI framework) and [alacritty_terminal](https://crates.io/crates/alacritty_terminal) for terminal emulation.

## Features

### Terminal
- Full terminal emulation (xterm-256color) with smooth trackpad scrolling
- Split panes: vertical (Cmd+D) and horizontal (Cmd+Shift+D) with draggable dividers
- Tabs with process name titles
- Mouse support for TUI apps (vim, htop, etc.) with macOS natural scrolling
- Double-click to select a word, triple-click to select a line
- Clipboard copy/paste, mouse selection, and scrollback

### Code Review (Cmd+G)
- **History tab** — 3-panel layout: commit list, file list, and unified diff
- **Changes tab** — uncommitted working tree diffs with status badges (M/A/D/?) and staged/unstaged indicators
- Syntax highlighting in diffs for 16 languages (Rust, JS, TS, Python, Go, C/C++, Java, Ruby, Shell, HTML, CSS, JSON, YAML, Markdown)
- Word-level change highlighting within modified lines
- Shift+Click or Shift+Up/Down to select multiple commits and view a combined diff
- Drag-to-select text in diffs and commit descriptions; Cmd+C copies from the active selection
- Copy commit hash from each commit row (click the hash button)
- Auto-refresh: working tree changes update every ~2s
- Selections persist across refreshes, tab switches, and mode toggles
- Virtual scrolling handles repos with 100K+ commits
- Metadata bar with author, commit hash, and colored +/- stats

### Toolbar
- Fish-style shortened current directory path
- Branch name with dirty/clean indicator
- Colored diff stats (green +N, yellow ~N, red -N) visible in all modes
- Git info hides automatically when not in a git repo

### Theme
- "Midnight Workshop" dark theme with blue-tinted backgrounds and amber-gold accents
- Layered background depth, hover feedback, and pill-shaped badges throughout

## Install

### From source

```bash
cargo build --release
./target/release/ade
```

### macOS app bundle

```bash
cargo build --release
./scripts/bundle-macos.sh        # creates Ade.app
./scripts/create-dmg.sh          # creates Ade.dmg (drag-to-install)
```

## Requirements

- macOS (Apple Silicon or Intel)
- Rust toolchain (edition 2024)
- No external build dependencies — pure Rust via `cargo build`

## Keyboard Shortcuts

### General

| Shortcut | Action |
|----------|--------|
| Cmd+C | Copy selection (or send SIGINT if no selection) |
| Cmd+V | Paste from clipboard |
| Cmd+A | Select all |
| Cmd+Q | Quit |

### Code Review

| Shortcut | Action |
|----------|--------|
| Cmd+G | Toggle Code Review panel on/off |
| Cmd+1 | Switch to Changes tab |
| Cmd+2 | Switch to History tab |
| Left / Right | Cycle active panel (commits → files → diff → commits) |
| Up / Down | Move selection in list panels; scroll diff line-by-line |
| Shift+Click | Select a range of commits |
| Shift+Up / Shift+Down | Extend commit selection one row at a time |

Active panel highlighted in bright blue; inactive panels dimmed. Selecting a commit auto-loads the first file's diff. Last active tab remembered across Cmd+G toggles.

### Panes

| Shortcut | Action |
|----------|--------|
| Cmd+D | Split vertically (side-by-side) |
| Cmd+Shift+D | Split horizontally (top/bottom) |
| Cmd+] | Focus next pane |
| Cmd+[ | Focus previous pane |
| Cmd+W | Close active pane |

### Tabs

| Shortcut | Action |
|----------|--------|
| Cmd+T | New tab |
| Cmd+Shift+W | Close tab |
| Cmd+} | Next tab |
| Cmd+{ | Previous tab |
| Cmd+1 through Cmd+9 | Switch to tab N (Terminal mode) |

## Tech Stack

- **[GPUI](https://github.com/zed-industries/zed)** — GPU-accelerated UI framework
- **[alacritty_terminal](https://crates.io/crates/alacritty_terminal)** — terminal emulation and PTY I/O
- **[git2](https://crates.io/crates/git2)** — libgit2 bindings for commit log, diff, and branch status

## License

MIT
