<p align="center">
  <img src="resources/icon.png" width="128" alt="ADE logo" />
</p>

<h1 align="center">ADE</h1>

<p align="center">
  <strong>The terminal that reviews code.</strong>
</p>

<p align="center">
  A GPU-accelerated macOS terminal with a built-in git code review panel.<br>
  Press <kbd>Cmd</kbd>+<kbd>G</kbd> to toggle a 3-panel code review view with commit history, file changes,<br>
  and syntax-highlighted diffs — right next to the code you're writing.
</p>

<p align="center">
  <a href="LICENSE"><img src="https://img.shields.io/badge/license-MIT-blue.svg" alt="MIT License" /></a>
  <img src="https://img.shields.io/badge/platform-macOS-lightgrey.svg" alt="macOS" />
  <img src="https://img.shields.io/badge/rust-edition%202024-orange.svg" alt="Rust Edition 2024" />
</p>

<br>

<p align="center">
  <img src="assets/hero.gif" width="800" alt="ADE — terminal mode and code review panel" />
</p>

<br>

## Why ADE?

Most developers flip between a terminal and a separate git GUI — or run `git log` and `git diff` manually. ADE puts a full code review panel inside your terminal, one keystroke away.

- **No window switching.** Your terminal and your diffs live in the same window.
- **No setup.** ADE reads your repo automatically. No config files, no plugins.
- **No compromise.** Full terminal emulation (alacritty_terminal) with splits, tabs, and mouse support — it replaces your terminal, not just supplements it.

## Features

### Code Review — <kbd>Cmd</kbd>+<kbd>G</kbd>

<p align="center">
  <img src="assets/code-review.png" width="800" alt="Code review panel — commit list, file list, and syntax-highlighted diff" />
  <br>
  <em>3-panel layout: commit history, file list, and syntax-highlighted unified diff</em>
</p>

| | |
|---|---|
| **History tab** | Browse commits, select files, read syntax-highlighted unified diffs — all in a 3-panel layout |
| **Changes tab** | See uncommitted working tree diffs with staged/unstaged indicators and status badges (M/A/D/?) |
| **Multi-commit select** | Shift+Click or Shift+Arrow to select a range of commits and view a combined diff |
| **Syntax highlighting** | 16 languages — Rust, JS/TS, Python, Go, C/C++, Java, Ruby, Shell, HTML, CSS, JSON, YAML, Markdown |
| **Word-level diffs** | Inline highlights show exactly what changed within each modified line |
| **Virtual scrolling** | Handles repos with 100K+ commits without lag |
| **Auto-refresh** | Working tree changes update every ~2s; selections persist across refreshes |

### Terminal

<p align="center">
  <img src="assets/terminal-splits.png" width="800" alt="Terminal with split panes and tabs" />
  <br>
  <em>Split panes and tabs with GPU-accelerated rendering</em>
</p>

| | |
|---|---|
| **Full emulation** | xterm-256color via alacritty_terminal — your shell, your tools, your escape sequences |
| **Split panes** | Vertical (<kbd>Cmd</kbd>+<kbd>D</kbd>) and horizontal (<kbd>Cmd</kbd>+<kbd>Shift</kbd>+<kbd>D</kbd>) with draggable dividers |
| **Tabs** | Open, close, and switch tabs with process name titles |
| **Mouse support** | Click, drag, and scroll in TUI apps (vim, htop, etc.) with macOS natural scrolling |
| **Selection** | Double-click word, triple-click line, drag to select, clipboard copy/paste |
| **GPU-accelerated** | Rendered with GPUI (Zed's framework) — smooth scrolling, no tearing |

### Toolbar

- Fish-style shortened current directory path
- Branch name with dirty/clean indicator
- Colored diff stats (green +N, yellow ~N, red -N) visible in all modes

### Theme

"Midnight Workshop" — a dark theme with blue-tinted backgrounds, amber-gold accents, layered depth, and hover feedback throughout.

## Install

### Download

Pre-built `.dmg` available on the [Releases](https://github.com/alexsds/ade/releases) page.

<!-- ### Homebrew
```bash
brew install --cask ade
``` -->

### Build from source

```bash
git clone https://github.com/alexsds/ade.git
cd ade
cargo build --release
./target/release/ade
```

#### macOS app bundle

```bash
cargo build --release
./scripts/bundle-macos.sh        # creates Ade.app
./scripts/create-dmg.sh          # creates Ade.dmg (drag-to-install)
```

### Requirements

- macOS (Apple Silicon or Intel)
- Rust toolchain (edition 2024) — only needed when building from source

## Keyboard Shortcuts

<details>
<summary><strong>General</strong></summary>

| Shortcut | Action |
|----------|--------|
| <kbd>Cmd</kbd>+<kbd>C</kbd> | Copy selection (or send SIGINT if no selection) |
| <kbd>Cmd</kbd>+<kbd>V</kbd> | Paste from clipboard |
| <kbd>Cmd</kbd>+<kbd>A</kbd> | Select all |
| <kbd>Cmd</kbd>+<kbd>Q</kbd> | Quit |

</details>

<details>
<summary><strong>Code Review</strong></summary>

| Shortcut | Action |
|----------|--------|
| <kbd>Cmd</kbd>+<kbd>G</kbd> | Toggle code review panel on/off |
| <kbd>Cmd</kbd>+<kbd>1</kbd> | Switch to Changes tab |
| <kbd>Cmd</kbd>+<kbd>2</kbd> | Switch to History tab |
| <kbd>Left</kbd> / <kbd>Right</kbd> | Cycle active panel (commits → files → diff) |
| <kbd>Up</kbd> / <kbd>Down</kbd> | Move selection in list panels; scroll diff line-by-line |
| <kbd>Shift</kbd>+<kbd>Click</kbd> | Select a range of commits |
| <kbd>Shift</kbd>+<kbd>Up</kbd>/<kbd>Down</kbd> | Extend commit selection |

</details>

<details>
<summary><strong>Panes</strong></summary>

| Shortcut | Action |
|----------|--------|
| <kbd>Cmd</kbd>+<kbd>D</kbd> | Split vertically (side-by-side) |
| <kbd>Cmd</kbd>+<kbd>Shift</kbd>+<kbd>D</kbd> | Split horizontally (top/bottom) |
| <kbd>Cmd</kbd>+<kbd>]</kbd> | Focus next pane |
| <kbd>Cmd</kbd>+<kbd>[</kbd> | Focus previous pane |
| <kbd>Cmd</kbd>+<kbd>W</kbd> | Close active pane |

</details>

<details>
<summary><strong>Tabs</strong></summary>

| Shortcut | Action |
|----------|--------|
| <kbd>Cmd</kbd>+<kbd>T</kbd> | New tab |
| <kbd>Cmd</kbd>+<kbd>Shift</kbd>+<kbd>W</kbd> | Close tab |
| <kbd>Cmd</kbd>+<kbd>}</kbd> | Next tab |
| <kbd>Cmd</kbd>+<kbd>{</kbd> | Previous tab |
| <kbd>Cmd</kbd>+<kbd>1</kbd>–<kbd>9</kbd> | Switch to tab N (terminal mode) |

</details>

## Roadmap

- [x] Full terminal emulation (alacritty_terminal)
- [x] GPU-accelerated rendering (GPUI)
- [x] Split panes and tabs
- [x] Git commit history browser
- [x] Unified diff viewer with syntax highlighting
- [x] Working tree changes panel
- [x] Multi-commit selection with combined diffs
- [x] Word-level diff highlighting
- [x] Mouse support for TUI apps
- [x] macOS app bundle and DMG installer
- [ ] Homebrew formula
- [ ] Configurable themes

## Tech Stack

- **[GPUI](https://github.com/zed-industries/zed)** — GPU-accelerated UI framework (from Zed)
- **[alacritty_terminal](https://crates.io/crates/alacritty_terminal)** — terminal emulation and PTY I/O
- **[git2](https://crates.io/crates/git2)** — libgit2 bindings for commit log, diff, and branch status
- **[tree-sitter](https://tree-sitter.github.io/)** — syntax highlighting for 16 languages

## Contributing

Contributions welcome! See [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

If you find a bug or have a feature request, please [open an issue](https://github.com/alexsds/ade/issues).

## License

[MIT](LICENSE)
