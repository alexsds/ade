# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## What is ADE

ADE (Advanced Developer Environment) is a Rust terminal application that wraps Ghostty for terminal emulation and adds a toggleable git history/diff panel. Built with GPUI (Zed's GPU-accelerated UI framework) and the `gpui-ghostty` bridge crate. Rust edition 2024.

## Build & Run

```bash
cargo build                          # requires Zig 0.14.1 (ghostty_vt_sys compiles C)
cargo run                            # launches terminal window using cwd's git repo
cargo test                           # all unit tests
cargo test -- test_walk_commits      # single test by name
cargo fmt --all -- --check           # format check
./scripts/bootstrap-zig.sh           # install Zig 0.14.1 if missing
git submodule update --init --recursive  # init/update gpui-ghostty submodule
```

Vendor crate CI checks (run before committing vendor changes):
```bash
cargo clippy -p ghostty_vt -p ghostty_vt_sys -p gpui_ghostty_terminal --all-targets -- -D warnings
cargo test -p ghostty_vt -p ghostty_vt_sys -p gpui_ghostty_terminal
```

## Architecture

### Dependency Stack
- **GPUI** (from Zed repo, pinned git rev) — GPU-accelerated UI framework
- **gpui-ghostty** (submodule at `vendor/gpui-ghostty/`) — bridges Ghostty terminal emulation into GPUI views. Three crates: `ghostty_vt`, `ghostty_vt_sys`, `gpui_ghostty_terminal`
- **portable-pty** — cross-platform PTY spawning
- **git2** — libgit2 bindings for all git operations
- **syntect** — syntax highlighting for diff viewer (dependency wired, highlighter currently stubbed in `diff_view.rs`)

### Source Layout (`src/`)

`main.rs` — Entry point. Creates `AdeWindow` entity, opens GPUI window, wires PTY, git polling loop, and mode switching.

**Two modes** via `Mode` enum, toggled with **Cmd+G**:
- `Terminal` — full-screen Ghostty terminal view
- `CodeReview` — 3-panel GitHub Desktop-style layout (commit list / file list / diff viewer)

**Terminal**: `terminal.rs` — PTY spawn, I/O thread wiring (stdin_tx/stdout_rx channels), resize handling. Output batched at 16ms.

**Git data layer**: `git/provider.rs` runs all git2 calls on a background thread; main thread polls via channels at 100ms. `git/types.rs` defines `CommitInfo`, `BranchStatus`, `FileChange`, `DiffData`, `FileDiff`, `DiffHunk`, `DiffLine`, `Decoration`.

**Code Review panels**: `code_review/` — `CodeReviewPanel` entity (mod.rs) manages state. Sub-modules: `commit_list.rs` (scrollable via `uniform_list`), `file_list.rs` (status badges + stats), `diff_view.rs` (virtualized unified diff via `uniform_list`, flattens hunks into `DiffRow`).

**Chrome**: `toolbar.rs` (branch name + dirty dot + toggle button), `input.rs` (keybindings), `menu.rs` (macOS menu bar).

### Key Patterns
- **Background thread + channel polling**: PTY I/O and git ops use `mpsc` channels with background threads. GPUI main thread polls via async timers.
- **Entity model**: GPUI `Entity<T>` — `AdeWindow` owns `TerminalView` and `CodeReviewPanel` entities.
- **uniform_list**: GPUI's virtualized list (only renders visible items). Used for commit list, file list, and diff lines.
- **pending_diff_request**: `CodeReviewPanel` sets an OID on commit selection; main loop in `main.rs` polls it and dispatches to `GitProvider`. This decouples the panel from the git thread.
- **Weak entity callbacks**: UI click handlers use `cx.weak_entity()` to avoid ownership issues with closures.

## Gotchas

- **Zig required**: `ghostty_vt_sys` compiles C code via Zig. Build fails without Zig 0.14.1 on PATH. Run `./scripts/bootstrap-zig.sh` to install locally.
- **No root-level clippy**: `cargo clippy` on the `ade` crate requires a running GPUI window context and will fail in headless CI. Lint only the vendor crates (see commands above).
- **GPUI pinned rev**: GPUI is pulled from the Zed repo at a specific git rev in `Cargo.toml`. Updating it may introduce breaking API changes — test thoroughly.
- **Cmd+C dual behavior**: `input.rs` binds Cmd+C to `CopyOrInterrupt` — copies text if selection exists, sends 0x03 (SIGINT) otherwise. Don't bind raw `Copy` to Cmd+C.
