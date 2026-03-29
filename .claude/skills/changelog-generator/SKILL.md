---
name: changelog-generator
description: Use when preparing release notes, creating changelogs from git commits, writing app store update descriptions, or generating user-facing summaries of code changes between versions or date ranges
---

# Changelog Generator

Transforms technical git commits into polished, user-friendly changelogs.

## When to Use

- Preparing release notes for a new version
- Creating weekly/monthly product update summaries
- Documenting changes for customers
- Writing changelog entries for app store submissions
- Generating update notifications
- Maintaining a public changelog or product updates page

## Process

1. **Scan Git History**: Analyze commits from a specific time period or between versions/tags
2. **Categorize Changes**: Group into logical categories (features, improvements, bug fixes, breaking changes, security)
3. **Translate to User Language**: Convert developer commit messages into customer-friendly descriptions
4. **Format**: Create clean, structured changelog entries
5. **Filter Noise**: Exclude internal commits (refactoring, tests, CI, docs unless user-facing)
6. **Follow Project Conventions**: Check for existing CHANGELOG.md style, Keep a Changelog format, or project-specific guidelines

## Usage Examples

```
Create a changelog from commits since last release
```

```
Generate changelog for commits from the past week
```

```
Create release notes for version 2.5.0
```

```
Create a changelog for commits between v2.4.0 and v2.5.0
```

## Output Format

Use [Keep a Changelog](https://keepachangelog.com/) categories:

- **Added** — new features
- **Changed** — changes in existing functionality
- **Deprecated** — soon-to-be removed features
- **Removed** — removed features
- **Fixed** — bug fixes
- **Security** — vulnerability fixes

### Example Output

```markdown
## [v2.5.0] - 2024-03-15

### Added
- Team workspaces: create separate workspaces for different projects,
  invite team members and keep everything organized
- Keyboard shortcuts: press ? to see all available shortcuts

### Changed
- File sync is now 2x faster across devices
- Search now includes file contents, not just titles

### Fixed
- Large images failing to upload
- Timezone confusion in scheduled posts
- Notification badge count showing incorrect number
```

## Writing Style — User-Facing Language

The most important job of the changelog is translating developer work into language a user cares about. Every entry must pass the "so what?" test from a user's perspective.

### Strip These (implementation details that don't belong)

| Never include | Why |
|---|---|
| Function/method names (`sanitize_git_string()`, `compute_diff_stats()`) | Users don't read source code |
| Struct/enum/type names (`StagingState`, `DiffRow::Line`, `TextSelection`) | Internal data model |
| Module paths (`src/theme/`, `code_review/`) | File layout is irrelevant to users |
| Test counts ("62 new unit tests", "302 total") | QA metric, not a feature |
| Architecture details ("via `uniform_list`", "GPUI Element", "FairMutex") | How it's built ≠ what it does |
| CSS/color values (`#0d1117`, `rgba()`, `2px accent border`) | Describe the visual result instead |
| Git internals (`diff_tree_to_tree`, `OID-based`) | Describe the user-visible behavior |
| Refactoring notes ("eliminated ~100 lines", "shared helper") | Invisible to users |
| Parameter/signature changes (`render_commit_detail signature simplified`) | API churn, not a feature |

### Write These Instead

| Instead of | Write |
|---|---|
| "`snap_char_boundary()` helper preventing GPUI panics on multi-byte UTF-8 chars" | "Crash when diff content contained multi-byte characters (e.g., em dashes)" |
| "StagingState enum: staged/unstaged dot indicators" | "Staged/unstaged indicators on changed files" |
| "`scroll_to_item_strict` for reliable line-by-line viewport control" | "Diff scrolling now tracks line-by-line without jumping past the end" |
| "Fractional scroll accumulator for smooth trackpad scrolling" | "Smooth trackpad scrolling in terminal" |
| "OSC 52 clipboard guard (opt-in via `ADE_ALLOW_OSC52=1`)" | "Clipboard access from terminal programs requires opt-in (`ADE_ALLOW_OSC52=1`)" |
| "Path-based selection preservation: selected file stays selected across auto-refresh" | "Selected file preserved across auto-refreshes" |

### Principles

- **Lead with what changed for the user**, not how it was implemented
- **Name the feature, not the code** — "Mouse support for TUI apps" not "Normal mouse encoding (X10-style) via `normal_mouse_sequence()`"
- **One concept per bullet** — merge related implementation bullets into a single user-facing statement
- **Keep env vars and keyboard shortcuts** — these are user-facing (`ADE_ALLOW_OSC52=1`, `Cmd+1`)
- **Consolidate related changes** — if 5 commits all improve "consistent spacing", that's one bullet, not five
- **Omit pure-refactoring entries entirely** — if nothing changed for the user, it doesn't belong

## General Guidelines

- Run from the git repository root
- Read existing CHANGELOG.md first to match its style and conventions
- If the project uses Keep a Changelog format, follow it strictly
- Omit empty categories (don't include "### Removed" if nothing was removed)
- Review generated output before appending to CHANGELOG.md
- When a date range is specified, use `git log --after/--before` flags
- When versions are specified, use `git log v1..v2` range syntax
