# ADE (Advanced Developer Environment)

A GPU-accelerated macOS terminal with built-in git history and diff viewer. Press **Cmd+G** to toggle a GitHub Desktop-style code review panel — right next to the code you're writing.

Built in Rust with [GPUI](https://github.com/zed-industries/zed) (Zed's UI framework) and [alacritty_terminal](https://crates.io/crates/alacritty_terminal) for terminal emulation.

## Features

**Terminal**
- Full PTY emulation via alacritty_terminal (xterm-256color)
- Vertical/horizontal pane splitting (Cmd+D / Cmd+Shift+D)
- Tabs with process name titles (Cmd+T, Cmd+W, Cmd+1-9)
- Mouse selection, clipboard, scrollback
- iTerm2-style keybindings

**Code Review (Cmd+G)**
- 3-panel layout: commit list | file list | unified diff
- Keyboard navigation: Left/Right to switch panels, Up/Down to navigate items
- Auto-cascade: selecting a commit loads first file's diff automatically
- Active/inactive panel highlighting
- Virtual scrolling for 100K+ commit repos
- Line-type diff coloring (additions, removals, hunk headers)
- Branch name and dirty status in toolbar

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

### Code Review

| Shortcut | Action |
|----------|--------|
| Cmd+G | Toggle Code Review panel on/off |
| Left / Right | Switch active panel (commits / files / diff) — wraps around |
| Up / Down | Move selection in commit or file list; scroll diff line-by-line |

Selecting a commit auto-selects the first changed file and loads its diff. Active panel shows bright blue highlight, inactive panels show dimmed highlight. Diff panel shows a top accent bar when focused.

### Panes

| Shortcut | Action |
|----------|--------|
| Cmd+D | Split active pane vertically (side-by-side) |
| Cmd+Shift+D | Split active pane horizontally (top/bottom) |
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
| Cmd+1 through Cmd+9 | Switch to tab N |

## Tech Stack

- **[GPUI](https://github.com/zed-industries/zed)** — GPU-accelerated rendering with `uniform_list` virtualization
- **[alacritty_terminal](https://crates.io/crates/alacritty_terminal)** — VT100/xterm terminal emulation and PTY I/O
- **[git2](https://crates.io/crates/git2)** — libgit2 bindings for commit log, diff, and branch status

## License

MIT
