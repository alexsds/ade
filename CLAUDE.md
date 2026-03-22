# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is ADE

ADE (Advanced Developer Environment) is a Rust terminal application that uses alacritty_terminal for terminal emulation and adds a toggleable git history/diff panel. Built with GPUI (Zed's GPU-accelerated UI framework). Rust edition 2024.

## Build & Run

```bash
cargo build                          # build (pure Rust, no external toolchain)
cargo run                            # launches terminal window using cwd's git repo
cargo test                           # all unit tests
cargo test -- test_walk_commits      # single test by name
cargo fmt --all -- --check           # format check
```

## Architecture

### Dependency Stack
- **GPUI** (from Zed repo, pinned git rev) — GPU-accelerated UI framework
- **alacritty_terminal** (0.25.1, crates.io) — VT100/xterm terminal emulation, PTY spawning, and I/O
- **git2** — libgit2 bindings for all git operations
- **syntect** — syntax highlighting for diff viewer (dependency wired, highlighter currently stubbed in `diff_view.rs`)

### Source Layout (`src/`)

`main.rs` — Entry point. Creates `AdeWindow` entity, opens GPUI window, wires PTY, git polling loop, and mode switching.

**Two modes** via `Mode` enum, toggled with **Cmd+G**:
- `Terminal` — full-screen alacritty_terminal view
- `CodeReview` — 3-panel GitHub Desktop-style layout (commit list / file list / diff viewer)

**Terminal**: `terminal.rs` — PTY spawn via alacritty_terminal's EventLoop and tty module, terminal state management (alacritty_terminal::Term with FairMutex), resize handling. `terminal_element.rs` — GPUI Element for cell-based rendering. `terminal_view.rs` — input handling, mouse events, selection, clipboard. `key_encode.rs` — pure Rust key-to-escape-sequence encoding for xterm-256color.

**Git data layer**: `git/provider.rs` runs all git2 calls on a background thread; main thread polls via channels at 100ms. `git/types.rs` defines `CommitInfo`, `BranchStatus`, `FileChange`, `DiffData`, `FileDiff`, `DiffHunk`, `DiffLine`, `Decoration`.

**Code Review panels**: `code_review/` — `CodeReviewPanel` entity (mod.rs) manages state. Sub-modules: `commit_list.rs` (scrollable via `uniform_list`), `file_list.rs` (status badges + stats), `diff_view.rs` (virtualized unified diff via `uniform_list`, flattens hunks into `DiffRow`).

**Chrome**: `toolbar.rs` (branch name + dirty dot + toggle button), `input.rs` (keybindings), `menu.rs` (macOS menu bar).

### Key Patterns
- **Background thread + channel polling**: PTY I/O and git ops use `mpsc` channels with background threads. GPUI main thread polls via async timers.
- **Entity model**: GPUI `Entity<T>` — `AdeWindow` owns `TerminalView` and `CodeReviewPanel` entities.
- **uniform_list**: GPUI's virtualized list (only renders visible items). Used for commit list, file list, and diff lines.
- **pending_diff_request**: `CodeReviewPanel` sets an OID on commit selection; main loop in `main.rs` polls it and dispatches to `GitProvider`. This decouples the panel from the git thread.
- **Weak entity callbacks**: UI click handlers use `cx.weak_entity()` to avoid ownership issues with closures.
- **FairMutex snapshot-and-release**: Terminal state (alacritty_terminal::Term) protected by FairMutex. sync() snapshots content while lock is held, then releases -- render path is lock-free.

## Gotchas

- **GPUI pinned rev**: GPUI is pulled from the Zed repo at a specific git rev in `Cargo.toml`. Updating it may introduce breaking API changes — test thoroughly.
- **Cmd+C dual behavior**: `input.rs` binds Cmd+C to `CopyOrInterrupt` — copies text if selection exists, sends 0x03 (SIGINT) otherwise. Don't bind raw `Copy` to Cmd+C.
- **FairMutex discipline**: Never hold the terminal FairMutex lock during GPUI layout/paint. Always snapshot during sync() and render from the snapshot.
