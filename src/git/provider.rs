use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;

use git2::{Oid, Repository, Sort, StatusOptions};

use super::types::{sanitize_git_string, *};

/// Maximum number of commits to load per repository.
/// Prevents unbounded memory growth for very large repositories (100K+ commits).
const MAX_COMMITS: usize = 50_000;

/// Maximum number of file deltas to process in a single diff.
/// Prevents OOM from commits touching thousands of files.
const MAX_DIFF_FILES: usize = 5_000;

/// Maximum total number of diff lines across all files in a single diff.
/// Prevents OOM from extremely large diffs (e.g., large binary-to-text).
const MAX_DIFF_LINES: usize = 500_000;

/// Maximum byte length of a single diff line's content.
/// Lines exceeding this are truncated with a marker.
const MAX_LINE_LENGTH: usize = 10_000;

/// Maximum blob size for image preview (10MB).
const MAX_IMAGE_BLOB_SIZE: usize = 10 * 1024 * 1024;

/// Requests that can be sent to the git background thread.
pub enum GitRequest {
    FetchLog {
        count: usize,
    },
    FetchMoreLog {
        batch_size: usize,
    },
    FetchDiff {
        commit_oid: String,
    },
    FetchRangeDiff {
        oldest_oid: String,
        newest_oid: String,
    },
    FetchStatus,
    FetchWorkingTreeFiles,
    FetchWorkingTreeDiff {
        path: String,
    },
    FetchBlob {
        commit_oid: String,
        path: String,
    },
}

/// Responses from the git background thread.
pub enum GitResponse {
    Log(Vec<CommitInfo>),
    MoreLog {
        commits: Vec<CommitInfo>,
        exhausted: bool,
    },
    Diff(DiffData),
    RangeDiff(DiffData),
    Status(BranchStatus),
    Error(String),
    WorkingTreeFiles(Vec<FileChange>),
    WorkingTreeDiff(DiffData),
    /// Decoded image blob, ready to render. Decode happened on background thread.
    BlobData {
        commit_oid: String,
        path: String,
        image: Arc<gpui::RenderImage>,
    },
    /// Blob fetch or decode error.
    BlobError {
        commit_oid: String,
        path: String,
        error: String,
    },
    /// Blob exceeded the 10MB size limit.
    BlobTooLarge {
        commit_oid: String,
        path: String,
    },
}

/// Git data provider that runs operations on a background thread.
///
/// All git2 calls happen on a dedicated background thread. The main thread
/// sends requests via `request_log`/`request_diff`/`request_status` and
/// polls for results with `try_recv`.
pub struct GitProvider {
    request_tx: mpsc::Sender<GitRequest>,
    response_rx: mpsc::Receiver<GitResponse>,
}

impl GitProvider {
    /// Spawn a new GitProvider with a background thread for the given repo path.
    ///
    /// Uses `Repository::discover()` so the path can be anywhere inside the
    /// working tree (not just the `.git` directory).
    pub fn new(repo_path: PathBuf) -> Self {
        let (request_tx, request_rx) = mpsc::channel::<GitRequest>();
        let (response_tx, response_rx) = mpsc::channel::<GitResponse>();

        let repo_path_for_thread = repo_path.clone();
        thread::spawn(move || {
            let repo = match Repository::discover(&repo_path_for_thread) {
                Ok(r) => r,
                Err(e) => {
                    let _ = response_tx.send(GitResponse::Error(format!(
                        "Failed to open repository: {}",
                        e
                    )));
                    return;
                }
            };

            let mut active_revwalk: Option<git2::Revwalk<'_>> = None;
            let mut revwalk_exhausted = false;
            let mut total_loaded: usize = 0;
            let mut ahead_count: usize = 0;

            while let Ok(request) = request_rx.recv() {
                let response = match request {
                    GitRequest::FetchLog { count } => {
                        // Fresh load: reset revwalk state and commit counter
                        active_revwalk = None;
                        revwalk_exhausted = false;
                        total_loaded = 0;
                        ahead_count = 0;
                        let decorations = build_decoration_map(&repo);
                        match repo.revwalk() {
                            Ok(mut revwalk) => {
                                if let Err(e) = revwalk.push_head() {
                                    let _ = response_tx.send(GitResponse::Error(format!(
                                        "Failed to push HEAD to revwalk: {}",
                                        e
                                    )));
                                    continue;
                                }
                                if let Err(e) = revwalk.set_sorting(Sort::TIME) {
                                    tracing::warn!("Failed to set revwalk sorting: {}", e);
                                }
                                let capped_count = count.min(MAX_COMMITS);
                                let mut commits =
                                    collect_batch(&repo, &mut revwalk, capped_count, &decorations);
                                total_loaded = commits.len();
                                revwalk_exhausted =
                                    commits.len() < capped_count || total_loaded >= MAX_COMMITS;
                                ahead_count = mark_ahead_commits(&repo, &mut commits);
                                active_revwalk = Some(revwalk);
                                GitResponse::Log(commits)
                            }
                            Err(e) => GitResponse::Error(e.to_string()),
                        }
                    }
                    GitRequest::FetchMoreLog { batch_size } => {
                        let remaining = MAX_COMMITS.saturating_sub(total_loaded);
                        if revwalk_exhausted || remaining == 0 {
                            GitResponse::MoreLog {
                                commits: vec![],
                                exhausted: true,
                            }
                        } else {
                            let decorations = build_decoration_map(&repo);
                            match active_revwalk.as_mut() {
                                Some(revwalk) => {
                                    let effective_batch = batch_size.min(remaining);
                                    let mut commits = collect_batch(
                                        &repo,
                                        revwalk,
                                        effective_batch,
                                        &decorations,
                                    );
                                    total_loaded += commits.len();
                                    // Mark ahead commits in this batch if any are still
                                    // within the ahead window (computed during FetchLog)
                                    let previously_loaded = total_loaded - commits.len();
                                    if previously_loaded < ahead_count {
                                        let to_mark = ahead_count - previously_loaded;
                                        for commit in commits.iter_mut().take(to_mark) {
                                            commit.is_ahead = true;
                                        }
                                    }
                                    let exhausted = commits.len() < effective_batch
                                        || total_loaded >= MAX_COMMITS;
                                    revwalk_exhausted = exhausted;
                                    GitResponse::MoreLog { commits, exhausted }
                                }
                                None => GitResponse::MoreLog {
                                    commits: vec![],
                                    exhausted: true,
                                },
                            }
                        }
                    }
                    GitRequest::FetchDiff { commit_oid } => match Oid::from_str(&commit_oid) {
                        Ok(oid) => match compute_diff(&repo, oid) {
                            Ok(diff) => GitResponse::Diff(diff),
                            Err(e) => GitResponse::Error(e.to_string()),
                        },
                        Err(e) => {
                            GitResponse::Error(format!("Invalid OID '{}': {}", commit_oid, e))
                        }
                    },
                    GitRequest::FetchRangeDiff {
                        oldest_oid,
                        newest_oid,
                    } => match (Oid::from_str(&oldest_oid), Oid::from_str(&newest_oid)) {
                        (Ok(oldest), Ok(newest)) => {
                            match compute_range_diff(&repo, oldest, newest) {
                                Ok(diff) => GitResponse::RangeDiff(diff),
                                Err(e) => GitResponse::Error(e.to_string()),
                            }
                        }
                        (Err(e), _) | (_, Err(e)) => {
                            GitResponse::Error(format!("Invalid range OID: {}", e))
                        }
                    },
                    GitRequest::FetchStatus => match get_branch_status(&repo) {
                        Ok(status) => GitResponse::Status(status),
                        Err(e) => GitResponse::Error(e.to_string()),
                    },
                    GitRequest::FetchWorkingTreeFiles => match compute_working_tree_files(&repo) {
                        Ok(files) => GitResponse::WorkingTreeFiles(files),
                        Err(e) => GitResponse::Error(format!("Working tree files: {}", e)),
                    },
                    GitRequest::FetchWorkingTreeDiff { path } => {
                        match compute_working_tree_file_diff(&repo, &path) {
                            Ok(diff) => GitResponse::WorkingTreeDiff(diff),
                            Err(e) => GitResponse::Error(format!("Working tree diff: {}", e)),
                        }
                    }
                    GitRequest::FetchBlob { commit_oid, path } => {
                        match Oid::from_str(&commit_oid) {
                            Ok(oid) => match read_blob_from_commit(&repo, oid, &path) {
                                Ok(data) => {
                                    if data.len() > MAX_IMAGE_BLOB_SIZE {
                                        GitResponse::BlobTooLarge { commit_oid, path }
                                    } else {
                                        match decode_image_bytes(&data) {
                                            Ok(image) => GitResponse::BlobData {
                                                commit_oid,
                                                path,
                                                image,
                                            },
                                            Err(e) => GitResponse::BlobError {
                                                commit_oid,
                                                path,
                                                error: e,
                                            },
                                        }
                                    }
                                }
                                Err(e) => GitResponse::BlobError {
                                    error: format!("Failed to read blob: {}", e),
                                    commit_oid,
                                    path,
                                },
                            },
                            Err(e) => {
                                let error = format!("Invalid OID '{}': {}", commit_oid, e);
                                GitResponse::BlobError {
                                    commit_oid,
                                    path,
                                    error,
                                }
                            }
                        }
                    }
                };
                if response_tx.send(response).is_err() {
                    break;
                }
            }
        });

        Self {
            request_tx,
            response_rx,
        }
    }

    /// Request the commit log (most recent `count` commits from HEAD).
    pub fn request_log(&self, count: usize) {
        if self
            .request_tx
            .send(GitRequest::FetchLog { count })
            .is_err()
        {
            tracing::warn!("Git background thread disconnected (FetchLog)");
        }
    }

    /// Request the diff for the given commit OID (hex string).
    pub fn request_diff(&self, oid_hex: &str) {
        if self
            .request_tx
            .send(GitRequest::FetchDiff {
                commit_oid: oid_hex.to_string(),
            })
            .is_err()
        {
            tracing::warn!("Git background thread disconnected (FetchDiff)");
        }
    }

    /// Request the current branch status (name + dirty flag).
    pub fn request_status(&self) {
        if self.request_tx.send(GitRequest::FetchStatus).is_err() {
            tracing::warn!("Git background thread disconnected (FetchStatus)");
        }
    }

    /// Request more commits from the persistent revwalk (incremental batch).
    pub fn request_more_log(&self, batch_size: usize) {
        if self
            .request_tx
            .send(GitRequest::FetchMoreLog { batch_size })
            .is_err()
        {
            tracing::warn!("Git background thread disconnected (FetchMoreLog)");
        }
    }

    /// Request the list of working tree changed files with status and staging state.
    pub fn request_working_tree_files(&self) {
        if self
            .request_tx
            .send(GitRequest::FetchWorkingTreeFiles)
            .is_err()
        {
            tracing::warn!("Git background thread disconnected (FetchWorkingTreeFiles)");
        }
    }

    /// Request a per-file working tree diff filtered by path.
    pub fn request_working_tree_diff(&self, path: &str) {
        if self
            .request_tx
            .send(GitRequest::FetchWorkingTreeDiff {
                path: path.to_string(),
            })
            .is_err()
        {
            tracing::warn!("Git background thread disconnected (FetchWorkingTreeDiff)");
        }
    }

    /// Request a combined diff for a range of commits (oldest_parent..newest).
    pub fn request_range_diff(&self, oldest_oid: &str, newest_oid: &str) {
        if self
            .request_tx
            .send(GitRequest::FetchRangeDiff {
                oldest_oid: oldest_oid.to_string(),
                newest_oid: newest_oid.to_string(),
            })
            .is_err()
        {
            tracing::warn!("Git background thread disconnected (FetchRangeDiff)");
        }
    }

    /// Request a file blob from a specific commit tree.
    pub fn request_blob(&self, commit_oid: &str, path: &str) {
        if self
            .request_tx
            .send(GitRequest::FetchBlob {
                commit_oid: commit_oid.to_string(),
                path: path.to_string(),
            })
            .is_err()
        {
            tracing::warn!("Git background thread disconnected (FetchBlob)");
        }
    }

    /// Non-blocking poll for responses from the background thread.
    pub fn try_recv(&self) -> Option<GitResponse> {
        self.response_rx.try_recv().ok()
    }
}

// ---------------------------------------------------------------------------
// Background thread helper functions
// ---------------------------------------------------------------------------

/// Collect up to `count` commits from a revwalk iterator.
///
/// Uses a manual counter loop (not `.take()`) to avoid borrow issues with
/// the mutable revwalk reference. The revwalk position advances in-place,
/// so subsequent calls continue from where the previous call left off.
fn collect_batch(
    repo: &Repository,
    revwalk: &mut git2::Revwalk<'_>,
    count: usize,
    decorations: &HashMap<Oid, Vec<Decoration>>,
) -> Vec<CommitInfo> {
    let mut commits = Vec::with_capacity(count.min(10_000));
    let mut collected = 0;
    while collected < count {
        match revwalk.next() {
            Some(Ok(oid)) => {
                if let Ok(commit) = repo.find_commit(oid) {
                    let author = commit.author();
                    let time = commit.time();
                    let commit_decorations = decorations.get(&oid).cloned().unwrap_or_default();
                    commits.push(CommitInfo {
                        oid: oid.to_string(),
                        summary: sanitize_git_string(commit.summary().unwrap_or("")),
                        body: commit.body().map(|b| sanitize_git_string(b)),
                        author_name: sanitize_git_string(author.name().unwrap_or("")),
                        author_email: sanitize_git_string(author.email().unwrap_or("")),
                        time_seconds: time.seconds(),
                        time_offset: time.offset_minutes(),
                        decorations: commit_decorations,
                        is_ahead: false,
                    });
                    collected += 1;
                }
            }
            Some(Err(_)) => continue, // skip unreadable OIDs
            None => break,            // end of history
        }
    }
    commits
}

/// Look up the OID of the upstream tracking branch for the current HEAD.
/// Returns None if HEAD is detached, no upstream is configured, or any lookup fails.
fn get_upstream_oid(repo: &Repository) -> Option<Oid> {
    let head = repo.head().ok()?;
    if !head.is_branch() {
        return None; // detached HEAD has no upstream
    }
    let branch_name = head.shorthand()?;
    let branch = repo
        .find_branch(branch_name, git2::BranchType::Local)
        .ok()?;
    let upstream = branch.upstream().ok()?;
    upstream.get().target()
}

/// Mark commits that are ahead of the upstream tracking branch.
/// Returns the number of commits ahead (for use in FetchMoreLog batching).
/// If no upstream exists, returns 0 and leaves all commits unchanged.
fn mark_ahead_commits(repo: &Repository, commits: &mut [CommitInfo]) -> usize {
    let Some(upstream_oid) = get_upstream_oid(repo) else {
        return 0;
    };
    let head_oid = match repo.head().ok().and_then(|h| h.target()) {
        Some(oid) => oid,
        None => return 0,
    };
    let (ahead, _behind) = match repo.graph_ahead_behind(head_oid, upstream_oid) {
        Ok(counts) => counts,
        Err(_) => return 0,
    };
    // The first `ahead` commits in time-sorted order are unpushed
    for commit in commits.iter_mut().take(ahead) {
        commit.is_ahead = true;
    }
    ahead
}

/// Read a blob (file content) from a specific commit's tree.
fn read_blob_from_commit(
    repo: &Repository,
    commit_oid: Oid,
    file_path: &str,
) -> Result<Vec<u8>, git2::Error> {
    let commit = repo.find_commit(commit_oid)?;
    let tree = commit.tree()?;
    let entry = tree.get_path(std::path::Path::new(file_path))?;
    let blob = repo.find_blob(entry.id())?;
    Ok(blob.content().to_vec())
}

/// Decode image bytes into a GPUI RenderImage.
/// Converts from RGBA to BGRA pixel format as required by GPUI/Metal.
/// Called on the background thread to avoid blocking the UI.
pub fn decode_image_bytes(bytes: &[u8]) -> Result<Arc<gpui::RenderImage>, String> {
    use image::Frame;
    use smallvec::SmallVec;
    let dynamic =
        image::load_from_memory(bytes).map_err(|e| format!("Image decode error: {}", e))?;
    let mut rgba = dynamic.into_rgba8();
    // GPUI expects BGRA, not RGBA (Pitfall 1 from RESEARCH.md)
    for pixel in rgba.pixels_mut() {
        let r = pixel.0[0];
        pixel.0[0] = pixel.0[2];
        pixel.0[2] = r;
    }
    let frame = Frame::new(rgba);
    Ok(Arc::new(gpui::RenderImage::new(SmallVec::from_elem(
        frame, 1,
    ))))
}

/// Build a map from commit OID to branch/tag decorations.
fn build_decoration_map(repo: &Repository) -> HashMap<Oid, Vec<Decoration>> {
    let mut map: HashMap<Oid, Vec<Decoration>> = HashMap::new();

    // Branches
    if let Ok(branches) = repo.branches(None) {
        for branch_result in branches {
            if let Ok((branch, _)) = branch_result {
                let name = match branch.name() {
                    Ok(Some(n)) => sanitize_git_string(n),
                    _ => continue,
                };
                let reference = match branch.into_reference().resolve() {
                    Ok(r) => r,
                    Err(_) => continue,
                };
                if let Some(oid) = reference.target() {
                    map.entry(oid)
                        .or_default()
                        .push(Decoration::Branch { name });
                }
            }
        }
    }

    // Tags
    if let Ok(tag_names) = repo.tag_names(None) {
        for tag_name in tag_names.iter().flatten() {
            if let Ok(reference) = repo.find_reference(&format!("refs/tags/{}", tag_name)) {
                // Peel to commit (handles both lightweight and annotated tags)
                if let Ok(obj) = reference.peel(git2::ObjectType::Commit) {
                    map.entry(obj.id()).or_default().push(Decoration::Tag {
                        name: sanitize_git_string(tag_name),
                    });
                }
            }
        }
    }

    map
}

/// Shared helper: collect FileChange, FileDiff, DiffHunk, and DiffLine data from a git2::Diff.
/// Enforces size limits (MAX_DIFF_FILES, MAX_DIFF_LINES, MAX_LINE_LENGTH) to prevent OOM.
/// Used by both compute_diff (single commit) and compute_range_diff (commit range).
fn collect_diff_data(diff: git2::Diff<'_>) -> Result<DiffData, git2::Error> {
    // Collect file changes from deltas (capped to MAX_DIFF_FILES)
    let mut files: Vec<FileChange> = Vec::new();
    let num_deltas = diff.deltas().len().min(MAX_DIFF_FILES);
    for delta in diff.deltas().take(num_deltas) {
        let path = sanitize_git_string(
            &delta
                .new_file()
                .path()
                .unwrap_or(std::path::Path::new(""))
                .to_string_lossy(),
        );
        let status_char = match delta.status() {
            git2::Delta::Added => 'A',
            git2::Delta::Modified => 'M',
            git2::Delta::Deleted => 'D',
            git2::Delta::Renamed => 'R',
            git2::Delta::Copied => 'C',
            _ => '?',
        };
        files.push(FileChange {
            path,
            status_char,
            additions: 0, // will be filled from line iteration
            deletions: 0,
            staging_state: None, // commit diffs don't have staging state
        });
    }

    // Collect per-file diffs with hunks and lines using RefCell
    // to share mutable state across the four diff.foreach closures.
    let file_diffs: RefCell<Vec<FileDiff>> = RefCell::new(Vec::new());
    let current_file: RefCell<Option<FileDiff>> = RefCell::new(None);
    let current_hunk: RefCell<Option<DiffHunk>> = RefCell::new(None);
    let total_lines: RefCell<usize> = RefCell::new(0);
    let file_count: RefCell<usize> = RefCell::new(0);

    diff.foreach(
        &mut |delta, _progress| {
            // Enforce file count limit
            let count = *file_count.borrow();
            if count >= MAX_DIFF_FILES {
                return false; // stop iteration
            }
            *file_count.borrow_mut() = count + 1;

            // File callback: start a new file
            if let Some(mut file) = current_file.borrow_mut().take() {
                if let Some(hunk) = current_hunk.borrow_mut().take() {
                    file.hunks.push(hunk);
                }
                file_diffs.borrow_mut().push(file);
            }
            let path = sanitize_git_string(
                &delta
                    .new_file()
                    .path()
                    .unwrap_or(std::path::Path::new(""))
                    .to_string_lossy(),
            );
            *current_file.borrow_mut() = Some(FileDiff {
                path,
                additions: 0,
                deletions: 0,
                hunks: Vec::new(),
            });
            true
        },
        Some(&mut |_delta, _binary| {
            // Binary callback: mark current file as binary (skip line-level diff)
            true
        }),
        Some(&mut |_delta, hunk| {
            // Enforce total line count limit
            if *total_lines.borrow() >= MAX_DIFF_LINES {
                return false;
            }
            // Hunk callback: start a new hunk
            if let Some(ref mut file) = *current_file.borrow_mut() {
                if let Some(prev_hunk) = current_hunk.borrow_mut().take() {
                    file.hunks.push(prev_hunk);
                }
            }
            let header = std::str::from_utf8(hunk.header())
                .unwrap_or("")
                .trim_end()
                .to_string();
            *current_hunk.borrow_mut() = Some(DiffHunk {
                header,
                lines: Vec::new(),
            });
            true
        }),
        Some(&mut |_delta, _hunk, line| {
            // Enforce total line count limit
            let lines_so_far = *total_lines.borrow();
            if lines_so_far >= MAX_DIFF_LINES {
                return false;
            }
            *total_lines.borrow_mut() = lines_so_far + 1;

            // Line callback: add line to current hunk
            let line_type = match line.origin() {
                '+' => DiffLineType::Add,
                '-' => DiffLineType::Remove,
                ' ' => DiffLineType::Context,
                'H' | 'F' => DiffLineType::HunkHeader,
                _ => DiffLineType::Context,
            };

            let raw_content = std::str::from_utf8(line.content()).unwrap_or("");
            let content = if raw_content.len() > MAX_LINE_LENGTH {
                let mut truncated = raw_content[..MAX_LINE_LENGTH].to_string();
                truncated.push_str("... (truncated)");
                truncated
            } else {
                raw_content.to_string()
            };

            if line_type == DiffLineType::Add {
                if let Some(ref mut file) = *current_file.borrow_mut() {
                    file.additions += 1;
                }
            } else if line_type == DiffLineType::Remove {
                if let Some(ref mut file) = *current_file.borrow_mut() {
                    file.deletions += 1;
                }
            }

            if let Some(ref mut hunk) = *current_hunk.borrow_mut() {
                hunk.lines.push(DiffLine {
                    line_type,
                    content,
                    old_lineno: line.old_lineno(),
                    new_lineno: line.new_lineno(),
                });
            }
            true
        }),
    )?;

    // Flush last file/hunk
    if let Some(mut file) = current_file.borrow_mut().take() {
        if let Some(hunk) = current_hunk.borrow_mut().take() {
            file.hunks.push(hunk);
        }
        file_diffs.borrow_mut().push(file);
    }

    let file_diffs = file_diffs.into_inner();

    // Update file change stats from the collected line data
    for (i, fd) in file_diffs.iter().enumerate() {
        if i < files.len() {
            files[i].additions = fd.additions;
            files[i].deletions = fd.deletions;
        }
    }

    Ok(DiffData { files, file_diffs })
}

/// Compute the diff for a single commit (against its first parent, or empty tree for root).
fn compute_diff(repo: &Repository, oid: Oid) -> Result<DiffData, git2::Error> {
    let commit = repo.find_commit(oid)?;
    let commit_tree = commit.tree()?;

    let parent_tree = if commit.parent_count() > 0 {
        Some(commit.parent(0)?.tree()?)
    } else {
        None // diff against empty tree for initial commit
    };

    let diff = repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&commit_tree), None)?;
    collect_diff_data(diff)
}

/// Compute a combined diff for a range of commits (oldest_parent..newest).
/// Diffs from the oldest commit's parent tree to the newest commit's tree (D-08).
/// For initial commits in the range, diffs against an empty tree.
fn compute_range_diff(
    repo: &Repository,
    oldest_oid: Oid,
    newest_oid: Oid,
) -> Result<DiffData, git2::Error> {
    let oldest_commit = repo.find_commit(oldest_oid)?;
    let newest_commit = repo.find_commit(newest_oid)?;
    let newest_tree = newest_commit.tree()?;

    // Diff from oldest commit's PARENT to newest commit (D-08)
    let oldest_parent_tree = if oldest_commit.parent_count() > 0 {
        Some(oldest_commit.parent(0)?.tree()?)
    } else {
        None // initial commit: diff against empty tree
    };

    let diff = repo.diff_tree_to_tree(oldest_parent_tree.as_ref(), Some(&newest_tree), None)?;
    collect_diff_data(diff)
}

/// Get the current branch name and dirty/clean status.
fn get_branch_status(repo: &Repository) -> Result<BranchStatus, git2::Error> {
    let branch_name = match repo.head() {
        Ok(head) => {
            if head.is_branch() {
                head.shorthand().unwrap_or("HEAD").to_string()
            } else {
                // Detached HEAD -- show short OID
                match head.target() {
                    Some(oid) => format!("{:.7}", oid),
                    None => "(detached)".to_string(),
                }
            }
        }
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => "(no commits)".to_string(),
        Err(e) if e.code() == git2::ErrorCode::NotFound => "(detached)".to_string(),
        Err(e) => return Err(e),
    };

    // Check for dirty working directory
    let mut opts = StatusOptions::new();
    opts.include_untracked(true);
    opts.exclude_submodules(true);
    let statuses = repo.statuses(Some(&mut opts))?;
    let is_dirty = statuses.iter().any(|e| e.status() != git2::Status::CURRENT);

    Ok(BranchStatus {
        branch_name,
        is_dirty,
    })
}

/// Compute the list of working tree changed files with status, stats, and staging state.
/// Returns all files that differ between HEAD and the combined index+workdir.
/// Handles empty repos (no HEAD) without crashing.
fn compute_working_tree_files(repo: &Repository) -> Result<Vec<FileChange>, git2::Error> {
    // Step 1: Get per-file status for staging state detection
    let mut status_opts = StatusOptions::new();
    status_opts
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .exclude_submodules(true);
    let statuses = repo.statuses(Some(&mut status_opts))?;

    let mut staging_map: HashMap<String, StagingState> = HashMap::new();
    for entry in statuses.iter() {
        let path = sanitize_git_string(entry.path().unwrap_or(""));
        let status = entry.status();
        let has_index = status.intersects(
            git2::Status::INDEX_NEW
                | git2::Status::INDEX_MODIFIED
                | git2::Status::INDEX_DELETED
                | git2::Status::INDEX_RENAMED
                | git2::Status::INDEX_TYPECHANGE,
        );
        let has_wt = status.intersects(
            git2::Status::WT_NEW
                | git2::Status::WT_MODIFIED
                | git2::Status::WT_DELETED
                | git2::Status::WT_RENAMED
                | git2::Status::WT_TYPECHANGE,
        );
        let state = match (has_index, has_wt) {
            (true, false) => StagingState::Staged,
            (false, true) => StagingState::Unstaged,
            (true, true) => StagingState::Partial,
            (false, false) => continue,
        };
        staging_map.insert(path, state);
    }

    // Step 2: Get combined diff HEAD -> workdir+index for file list + stats
    // Handle empty repo (no HEAD)
    let head_tree = match repo.head() {
        Ok(head) => Some(head.peel_to_tree()?),
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => None,
        Err(e) if e.code() == git2::ErrorCode::NotFound => None,
        Err(e) => return Err(e),
    };

    let mut diff_opts = git2::DiffOptions::new();
    diff_opts
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .show_untracked_content(true);

    let diff = repo.diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut diff_opts))?;

    // Step 3: Process diff to build FileChange list
    let mut files: Vec<FileChange> = Vec::new();
    let num_deltas = diff.deltas().len().min(MAX_DIFF_FILES);

    for delta in diff.deltas().take(num_deltas) {
        let path = sanitize_git_string(
            &delta
                .new_file()
                .path()
                .unwrap_or(std::path::Path::new(""))
                .to_string_lossy(),
        );
        let status_char = match delta.status() {
            git2::Delta::Added => 'A',
            git2::Delta::Modified => 'M',
            git2::Delta::Deleted => 'D',
            git2::Delta::Renamed => 'R',
            git2::Delta::Copied => 'C',
            git2::Delta::Untracked => '?',
            _ => '?',
        };
        let staging_state = staging_map.get(&path).copied();
        files.push(FileChange {
            path,
            status_char,
            additions: 0,
            deletions: 0,
            staging_state,
        });
    }

    // Second pass: count additions/deletions per file via foreach
    let file_additions: RefCell<Vec<u64>> = RefCell::new(vec![0; files.len()]);
    let file_deletions: RefCell<Vec<u64>> = RefCell::new(vec![0; files.len()]);
    let file_index: RefCell<usize> = RefCell::new(0);
    let total_lines: RefCell<usize> = RefCell::new(0);
    let file_count: RefCell<usize> = RefCell::new(0);

    let _ = diff.foreach(
        &mut |_delta, _progress| {
            let count = *file_count.borrow();
            if count >= MAX_DIFF_FILES {
                return false;
            }
            *file_index.borrow_mut() = count;
            *file_count.borrow_mut() = count + 1;
            true
        },
        None,
        None,
        Some(&mut |_delta, _hunk, line| {
            let lines_so_far = *total_lines.borrow();
            if lines_so_far >= MAX_DIFF_LINES {
                return false;
            }
            *total_lines.borrow_mut() = lines_so_far + 1;

            let idx = *file_index.borrow();
            match line.origin() {
                '+' => {
                    if let Some(val) = file_additions.borrow_mut().get_mut(idx) {
                        *val += 1;
                    }
                }
                '-' => {
                    if let Some(val) = file_deletions.borrow_mut().get_mut(idx) {
                        *val += 1;
                    }
                }
                _ => {}
            }
            true
        }),
    );

    // Apply stats to files
    let additions = file_additions.into_inner();
    let deletions = file_deletions.into_inner();
    for (i, file) in files.iter_mut().enumerate() {
        if i < additions.len() {
            file.additions = additions[i];
        }
        if i < deletions.len() {
            file.deletions = deletions[i];
        }
    }

    Ok(files)
}

/// Compute a per-file working tree diff filtered by pathspec.
/// Returns full DiffData with hunks and lines for the specified file.
/// Untracked files appear with all content as Add lines.
fn compute_working_tree_file_diff(repo: &Repository, path: &str) -> Result<DiffData, git2::Error> {
    // Handle empty repo (no HEAD)
    let head_tree = match repo.head() {
        Ok(head) => Some(head.peel_to_tree()?),
        Err(e) if e.code() == git2::ErrorCode::UnbornBranch => None,
        Err(e) if e.code() == git2::ErrorCode::NotFound => None,
        Err(e) => return Err(e),
    };

    let mut diff_opts = git2::DiffOptions::new();
    diff_opts
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .show_untracked_content(true)
        .pathspec(path);

    let diff = repo.diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut diff_opts))?;

    // Collect file changes from deltas
    let mut files: Vec<FileChange> = Vec::new();
    let num_deltas = diff.deltas().len().min(MAX_DIFF_FILES);
    for delta in diff.deltas().take(num_deltas) {
        let file_path = sanitize_git_string(
            &delta
                .new_file()
                .path()
                .unwrap_or(std::path::Path::new(""))
                .to_string_lossy(),
        );
        let status_char = match delta.status() {
            git2::Delta::Added => 'A',
            git2::Delta::Modified => 'M',
            git2::Delta::Deleted => 'D',
            git2::Delta::Renamed => 'R',
            git2::Delta::Copied => 'C',
            git2::Delta::Untracked => '?',
            _ => '?',
        };
        files.push(FileChange {
            path: file_path,
            status_char,
            additions: 0,
            deletions: 0,
            staging_state: None,
        });
    }

    // Collect per-file diffs with hunks and lines (same pattern as compute_diff)
    let file_diffs: RefCell<Vec<FileDiff>> = RefCell::new(Vec::new());
    let current_file: RefCell<Option<FileDiff>> = RefCell::new(None);
    let current_hunk: RefCell<Option<DiffHunk>> = RefCell::new(None);
    let total_lines: RefCell<usize> = RefCell::new(0);
    let file_count: RefCell<usize> = RefCell::new(0);

    diff.foreach(
        &mut |delta, _progress| {
            let count = *file_count.borrow();
            if count >= MAX_DIFF_FILES {
                return false;
            }
            *file_count.borrow_mut() = count + 1;

            if let Some(mut file) = current_file.borrow_mut().take() {
                if let Some(hunk) = current_hunk.borrow_mut().take() {
                    file.hunks.push(hunk);
                }
                file_diffs.borrow_mut().push(file);
            }
            let file_path = sanitize_git_string(
                &delta
                    .new_file()
                    .path()
                    .unwrap_or(std::path::Path::new(""))
                    .to_string_lossy(),
            );
            *current_file.borrow_mut() = Some(FileDiff {
                path: file_path,
                additions: 0,
                deletions: 0,
                hunks: Vec::new(),
            });
            true
        },
        Some(&mut |_delta, _binary| true),
        Some(&mut |_delta, hunk| {
            if *total_lines.borrow() >= MAX_DIFF_LINES {
                return false;
            }
            if let Some(ref mut file) = *current_file.borrow_mut() {
                if let Some(prev_hunk) = current_hunk.borrow_mut().take() {
                    file.hunks.push(prev_hunk);
                }
            }
            let header = std::str::from_utf8(hunk.header())
                .unwrap_or("")
                .trim_end()
                .to_string();
            *current_hunk.borrow_mut() = Some(DiffHunk {
                header,
                lines: Vec::new(),
            });
            true
        }),
        Some(&mut |_delta, _hunk, line| {
            let lines_so_far = *total_lines.borrow();
            if lines_so_far >= MAX_DIFF_LINES {
                return false;
            }
            *total_lines.borrow_mut() = lines_so_far + 1;

            let line_type = match line.origin() {
                '+' => DiffLineType::Add,
                '-' => DiffLineType::Remove,
                ' ' => DiffLineType::Context,
                'H' | 'F' => DiffLineType::HunkHeader,
                _ => DiffLineType::Context,
            };

            let raw_content = std::str::from_utf8(line.content()).unwrap_or("");
            let content = if raw_content.len() > MAX_LINE_LENGTH {
                let mut truncated = raw_content[..MAX_LINE_LENGTH].to_string();
                truncated.push_str("... (truncated)");
                truncated
            } else {
                raw_content.to_string()
            };

            if line_type == DiffLineType::Add {
                if let Some(ref mut file) = *current_file.borrow_mut() {
                    file.additions += 1;
                }
            } else if line_type == DiffLineType::Remove {
                if let Some(ref mut file) = *current_file.borrow_mut() {
                    file.deletions += 1;
                }
            }

            if let Some(ref mut hunk) = *current_hunk.borrow_mut() {
                hunk.lines.push(DiffLine {
                    line_type,
                    content,
                    old_lineno: line.old_lineno(),
                    new_lineno: line.new_lineno(),
                });
            }
            true
        }),
    )?;

    // Flush last file/hunk
    if let Some(mut file) = current_file.borrow_mut().take() {
        if let Some(hunk) = current_hunk.borrow_mut().take() {
            file.hunks.push(hunk);
        }
        file_diffs.borrow_mut().push(file);
    }

    let file_diffs = file_diffs.into_inner();

    // Update file change stats
    for (i, fd) in file_diffs.iter().enumerate() {
        if i < files.len() {
            files[i].additions = fd.additions;
            files[i].deletions = fd.deletions;
        }
    }

    Ok(DiffData { files, file_diffs })
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    /// Create a temporary git repository with two commits and a "feature" branch.
    fn create_test_repo() -> (tempfile::TempDir, Repository) {
        let dir = tempfile::tempdir().expect("create tempdir");
        let repo = Repository::init(dir.path()).expect("init repo");

        // Configure committer identity for test
        let mut config = repo.config().expect("get config");
        config
            .set_str("user.name", "Test Author")
            .expect("set name");
        config
            .set_str("user.email", "test@example.com")
            .expect("set email");

        // First commit: add hello.txt
        let file_path = dir.path().join("hello.txt");
        fs::write(&file_path, "hello world\n").expect("write hello.txt");
        add_and_commit(&repo, dir.path(), "Initial commit");

        // Second commit: modify hello.txt
        fs::write(&file_path, "hello world\nline 2\n").expect("modify hello.txt");
        add_and_commit(&repo, dir.path(), "Add line 2");

        // Create a branch "feature" pointing at HEAD
        {
            let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
            repo.branch("feature", &head_commit, false)
                .expect("create feature branch");
        }

        (dir, repo)
    }

    /// Stage all files and create a commit.
    fn add_and_commit(repo: &Repository, _workdir: &Path, message: &str) {
        let mut index = repo.index().expect("get index");
        index
            .add_all(["*"].iter(), git2::IndexAddOption::DEFAULT, None)
            .expect("add all");
        index.write().expect("write index");
        let tree_oid = index.write_tree().expect("write tree");
        let tree = repo.find_tree(tree_oid).expect("find tree");
        let sig = repo.signature().expect("signature");

        let parents: Vec<git2::Commit> = match repo.head() {
            Ok(head) => vec![head.peel_to_commit().expect("peel to commit")],
            Err(_) => vec![],
        };
        let parent_refs: Vec<&git2::Commit> = parents.iter().collect();

        repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &parent_refs)
            .expect("commit");
    }

    #[test]
    fn test_walk_commits_via_collect_batch() {
        let (_dir, repo) = create_test_repo();
        let decorations = build_decoration_map(&repo);
        let mut revwalk = repo.revwalk().expect("revwalk");
        revwalk.push_head().unwrap();
        revwalk.set_sorting(Sort::TIME).unwrap();
        let commits = collect_batch(&repo, &mut revwalk, 10, &decorations);

        assert_eq!(commits.len(), 2, "Expected 2 commits");

        // Most recent first (TIME sorting)
        assert_eq!(commits[0].summary, "Add line 2");
        assert_eq!(commits[1].summary, "Initial commit");

        // Author info
        assert_eq!(commits[0].author_name, "Test Author");
        assert_eq!(commits[0].author_email, "test@example.com");
    }

    #[test]
    fn test_compute_diff() {
        let (_dir, repo) = create_test_repo();

        // Get second commit (HEAD) OID
        let head = repo.head().unwrap();
        let head_oid = head.target().unwrap();

        let diff = compute_diff(&repo, head_oid).expect("compute diff");

        // Should have 1 file changed
        assert_eq!(diff.files.len(), 1, "Expected 1 file change");
        assert_eq!(diff.files[0].path, "hello.txt");
        assert_eq!(diff.files[0].status_char, 'M');

        // Should have file diffs with hunks containing Add/Remove lines
        assert_eq!(diff.file_diffs.len(), 1);
        let file_diff = &diff.file_diffs[0];
        assert!(!file_diff.hunks.is_empty(), "Expected at least one hunk");

        // Check that we have both add and remove (or just add) lines
        let has_add = file_diff
            .hunks
            .iter()
            .any(|h| h.lines.iter().any(|l| l.line_type == DiffLineType::Add));
        assert!(has_add, "Expected at least one Add line in the diff");
    }

    #[test]
    fn test_branch_status() {
        let (dir, repo) = create_test_repo();

        // Clean state
        let status = get_branch_status(&repo).expect("get branch status");
        // git init creates a default branch; the name might be "main" or "master"
        assert!(
            !status.branch_name.is_empty(),
            "Branch name should not be empty"
        );
        assert!(!status.is_dirty, "Repo should be clean after commits");

        // Dirty state: write a new untracked file
        let new_file = dir.path().join("untracked.txt");
        fs::write(&new_file, "new content\n").expect("write untracked");
        let dirty_status = get_branch_status(&repo).expect("get dirty status");
        assert!(
            dirty_status.is_dirty,
            "Repo should be dirty with untracked file"
        );
    }

    #[test]
    fn test_initial_commit_diff() {
        let (_dir, repo) = create_test_repo();

        // Get the initial commit OID (parent_count == 0)
        let mut revwalk = repo.revwalk().unwrap();
        revwalk.push_head().unwrap();
        revwalk.set_sorting(Sort::TIME | Sort::REVERSE).unwrap();
        let initial_oid = revwalk.next().unwrap().unwrap();

        // This should NOT panic -- initial commits have no parent
        let diff = compute_diff(&repo, initial_oid).expect("compute diff for initial commit");

        assert!(
            !diff.files.is_empty(),
            "Initial commit should have file changes"
        );
        assert_eq!(diff.files[0].path, "hello.txt");
        assert_eq!(
            diff.files[0].status_char, 'A',
            "Initial commit file should be Added"
        );
    }

    #[test]
    fn test_decoration_map() {
        let (_dir, repo) = create_test_repo();
        let map = build_decoration_map(&repo);

        // There should be at least one OID with decorations
        assert!(!map.is_empty(), "Decoration map should not be empty");

        // Find the "feature" branch decoration
        let has_feature = map.values().any(|decorations| {
            decorations
                .iter()
                .any(|d| matches!(d, Decoration::Branch { name } if name == "feature"))
        });
        assert!(has_feature, "Should find 'feature' branch decoration");
    }

    /// Create a temporary git repository with `n` commits.
    fn create_test_repo_with_n_commits(n: usize) -> (tempfile::TempDir, Repository) {
        let dir = tempfile::tempdir().expect("create tempdir");
        let repo = Repository::init(dir.path()).expect("init repo");

        let mut config = repo.config().expect("get config");
        config
            .set_str("user.name", "Test Author")
            .expect("set name");
        config
            .set_str("user.email", "test@example.com")
            .expect("set email");

        for i in 0..n {
            let file_path = dir.path().join("file.txt");
            fs::write(&file_path, format!("content {}\n", i)).expect("write");
            add_and_commit(&repo, dir.path(), &format!("Commit {}", i));
        }

        (dir, repo)
    }

    #[test]
    fn test_collect_batch() {
        let (_dir, repo) = create_test_repo();
        let decorations = build_decoration_map(&repo);
        let mut revwalk = repo.revwalk().expect("create revwalk");
        revwalk.push_head().expect("push head");
        revwalk.set_sorting(Sort::TIME).expect("set sorting");

        let commits = collect_batch(&repo, &mut revwalk, 10, &decorations);

        assert_eq!(commits.len(), 2, "Expected 2 commits from 2-commit repo");
        // Most recent first (TIME sorting)
        assert_eq!(commits[0].summary, "Add line 2");
        assert_eq!(commits[1].summary, "Initial commit");
    }

    #[test]
    fn test_incremental_batch() {
        let (_dir, repo) = create_test_repo_with_n_commits(5);
        let decorations = build_decoration_map(&repo);
        let mut revwalk = repo.revwalk().expect("create revwalk");
        revwalk.push_head().expect("push head");
        revwalk.set_sorting(Sort::TIME).expect("set sorting");

        // First batch: 2 commits
        let batch1 = collect_batch(&repo, &mut revwalk, 2, &decorations);
        assert_eq!(batch1.len(), 2, "First batch should have 2 commits");

        // Second batch: next 2 commits (continues from where we left off)
        let batch2 = collect_batch(&repo, &mut revwalk, 2, &decorations);
        assert_eq!(batch2.len(), 2, "Second batch should have 2 commits");

        // Third batch: remaining 1 commit
        let batch3 = collect_batch(&repo, &mut revwalk, 2, &decorations);
        assert_eq!(
            batch3.len(),
            1,
            "Third batch should have 1 remaining commit"
        );

        // No overlap between any batches
        let all_oids: Vec<String> = batch1
            .iter()
            .chain(batch2.iter())
            .chain(batch3.iter())
            .map(|c| c.oid.clone())
            .collect();
        let unique_oids: std::collections::HashSet<&str> =
            all_oids.iter().map(|s| s.as_str()).collect();
        assert_eq!(
            all_oids.len(),
            unique_oids.len(),
            "No duplicate OIDs across batches"
        );

        // All 5 commits should be covered
        assert_eq!(
            all_oids.len(),
            5,
            "All 5 commits should be yielded across batches"
        );
    }

    #[test]
    fn test_batch_exhaustion() {
        let (_dir, repo) = create_test_repo();
        let decorations = build_decoration_map(&repo);
        let mut revwalk = repo.revwalk().expect("create revwalk");
        revwalk.push_head().expect("push head");
        revwalk.set_sorting(Sort::TIME).expect("set sorting");

        let commits = collect_batch(&repo, &mut revwalk, 10, &decorations);

        assert_eq!(
            commits.len(),
            2,
            "Should return 2 commits from 2-commit repo"
        );
        assert!(
            commits.len() < 10,
            "Returned fewer than requested signals exhaustion"
        );
    }

    #[test]
    fn test_collect_batch_respects_count() {
        // Create a repo with 10 commits, request only 3 -- should return exactly 3
        let (_dir, repo) = create_test_repo_with_n_commits(10);
        let decorations = build_decoration_map(&repo);
        let mut revwalk = repo.revwalk().expect("revwalk");
        revwalk.push_head().unwrap();
        revwalk.set_sorting(Sort::TIME).unwrap();

        let commits = collect_batch(&repo, &mut revwalk, 3, &decorations);
        assert_eq!(
            commits.len(),
            3,
            "Should return exactly 3 commits when repo has 10"
        );
    }

    #[test]
    fn test_commit_cap_arithmetic() {
        // Test the cap arithmetic that will be used in FetchLog/FetchMoreLog handlers
        let max_commits: usize = 50_000;

        // Case 1: No commits loaded yet -- full budget available
        let total_loaded: usize = 0;
        let remaining = max_commits.saturating_sub(total_loaded);
        assert_eq!(remaining, 50_000);

        // Case 2: Some commits loaded -- partial budget
        let total_loaded: usize = 49_990;
        let remaining = max_commits.saturating_sub(total_loaded);
        assert_eq!(remaining, 10);
        let batch_size: usize = 500;
        let effective_batch = batch_size.min(remaining);
        assert_eq!(
            effective_batch, 10,
            "Should clamp batch to remaining budget"
        );

        // Case 3: Exactly at cap -- zero remaining
        let total_loaded: usize = 50_000;
        let remaining = max_commits.saturating_sub(total_loaded);
        assert_eq!(remaining, 0);

        // Case 4: Over cap (safety) -- saturating_sub returns 0
        let total_loaded: usize = 50_001;
        let remaining = max_commits.saturating_sub(total_loaded);
        assert_eq!(remaining, 0);

        // Case 5: Initial count clamped to MAX_COMMITS
        let count: usize = 100_000;
        let capped_count = count.min(max_commits);
        assert_eq!(
            capped_count, 50_000,
            "Should clamp initial count to MAX_COMMITS"
        );

        // Case 6: Initial count under cap -- no change
        let count: usize = 200;
        let capped_count = count.min(max_commits);
        assert_eq!(capped_count, 200, "Should not clamp when under MAX_COMMITS");
    }

    #[test]
    fn test_commit_cap_enforced_via_provider() {
        // Integration test: create a repo with 15 commits, load via GitProvider,
        // verify exhaustion is reported correctly.
        let (_dir, repo) = create_test_repo_with_n_commits(15);
        let repo_path = repo.workdir().expect("workdir").to_path_buf();
        let provider = GitProvider::new(repo_path);

        // Request initial load with large count
        provider.request_log(200);
        std::thread::sleep(std::time::Duration::from_millis(300));

        let mut total_received = 0;
        while let Some(response) = provider.try_recv() {
            match response {
                GitResponse::Log(commits) => {
                    total_received += commits.len();
                }
                _ => {}
            }
        }
        assert_eq!(total_received, 15, "Should load all 15 commits");

        // Request more -- should be exhausted
        provider.request_more_log(500);
        std::thread::sleep(std::time::Duration::from_millis(300));

        while let Some(response) = provider.try_recv() {
            match response {
                GitResponse::MoreLog { commits, exhausted } => {
                    assert!(exhausted, "Should be exhausted after loading all commits");
                    assert!(commits.is_empty(), "Should have no more commits");
                }
                _ => {}
            }
        }
    }

    #[test]
    fn test_commit_cap_resets_on_fresh_load() {
        // Verify that requesting FetchLog again resets internal state
        // so the full budget is available for a new repo/load.
        let (_dir, repo) = create_test_repo_with_n_commits(10);
        let repo_path = repo.workdir().expect("workdir").to_path_buf();
        let provider = GitProvider::new(repo_path);

        // First load
        provider.request_log(200);
        std::thread::sleep(std::time::Duration::from_millis(300));
        let mut first_count = 0;
        while let Some(response) = provider.try_recv() {
            if let GitResponse::Log(commits) = response {
                first_count = commits.len();
            }
        }
        assert_eq!(first_count, 10);

        // Second fresh load (simulates repo switch) -- should get all 10 again
        provider.request_log(200);
        std::thread::sleep(std::time::Duration::from_millis(300));
        let mut second_count = 0;
        while let Some(response) = provider.try_recv() {
            if let GitResponse::Log(commits) = response {
                second_count = commits.len();
            }
        }
        assert_eq!(
            second_count, 10,
            "Fresh load should reset counter and return all commits"
        );
    }

    #[test]
    fn test_request_more_log_method() {
        let (_dir, repo) = create_test_repo();
        let repo_path = repo.workdir().expect("workdir").to_path_buf();
        let provider = GitProvider::new(repo_path);

        // Should not panic -- just sends a request
        provider.request_more_log(500);

        // Give background thread time to process
        std::thread::sleep(std::time::Duration::from_millis(200));

        // Should receive a MoreLog response (since no revwalk was initialized
        // by FetchLog first, it should return exhausted=true)
        if let Some(response) = provider.try_recv() {
            match response {
                GitResponse::MoreLog { commits, exhausted } => {
                    assert!(exhausted, "Should be exhausted with no active revwalk");
                    assert!(commits.is_empty(), "Should have no commits");
                }
                _ => panic!("Expected MoreLog response"),
            }
        }
    }

    // --- Working tree tests (Phase 21, Plan 01) ---

    #[test]
    fn test_compute_working_tree_files_modified() {
        let (dir, repo) = create_test_repo();
        // Modify hello.txt without staging
        let file_path = dir.path().join("hello.txt");
        fs::write(&file_path, "hello world\nline 2\nline 3\n").expect("modify hello.txt");

        let files = compute_working_tree_files(&repo).expect("compute working tree files");
        assert!(!files.is_empty(), "Should have at least one changed file");
        let hello = files
            .iter()
            .find(|f| f.path == "hello.txt")
            .expect("hello.txt should be in changes");
        assert_eq!(hello.status_char, 'M');
        assert!(
            hello.additions > 0 || hello.deletions > 0,
            "Should have non-zero stats"
        );
        assert_eq!(hello.staging_state, Some(StagingState::Unstaged));
    }

    #[test]
    fn test_untracked_file_in_working_tree() {
        let (dir, repo) = create_test_repo();
        // Add a new untracked file
        let new_file = dir.path().join("newfile.txt");
        fs::write(&new_file, "brand new content\n").expect("write newfile.txt");

        let files = compute_working_tree_files(&repo).expect("compute working tree files");
        let untracked = files
            .iter()
            .find(|f| f.path == "newfile.txt")
            .expect("newfile.txt should be in changes");
        assert_eq!(untracked.status_char, '?');
        assert_eq!(untracked.staging_state, Some(StagingState::Unstaged));
        assert!(
            untracked.additions > 0,
            "Untracked file should have additions"
        );
    }

    #[test]
    fn test_staged_file_working_tree() {
        let (dir, repo) = create_test_repo();
        // Modify hello.txt and stage it
        let file_path = dir.path().join("hello.txt");
        fs::write(&file_path, "hello world\nline 2\nstaged change\n").expect("modify hello.txt");
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("hello.txt")).unwrap();
        index.write().unwrap();

        let files = compute_working_tree_files(&repo).expect("compute working tree files");
        let hello = files
            .iter()
            .find(|f| f.path == "hello.txt")
            .expect("hello.txt should be in changes");
        assert_eq!(hello.staging_state, Some(StagingState::Staged));
    }

    #[test]
    fn test_partial_staging() {
        let (dir, repo) = create_test_repo();
        // Modify hello.txt, stage it, then modify again
        let file_path = dir.path().join("hello.txt");
        fs::write(&file_path, "hello world\nline 2\nstaged change\n").expect("modify hello.txt");
        let mut index = repo.index().unwrap();
        index.add_path(Path::new("hello.txt")).unwrap();
        index.write().unwrap();
        // Modify again without staging
        fs::write(
            &file_path,
            "hello world\nline 2\nstaged change\nunstaged change\n",
        )
        .expect("modify again");

        let files = compute_working_tree_files(&repo).expect("compute working tree files");
        let hello = files
            .iter()
            .find(|f| f.path == "hello.txt")
            .expect("hello.txt should be in changes");
        assert_eq!(hello.staging_state, Some(StagingState::Partial));
    }

    #[test]
    fn test_compute_working_tree_file_diff() {
        let (dir, repo) = create_test_repo();
        // Modify hello.txt
        let file_path = dir.path().join("hello.txt");
        fs::write(&file_path, "hello world\nline 2\nnew line 3\n").expect("modify hello.txt");

        let diff = compute_working_tree_file_diff(&repo, "hello.txt").expect("compute diff");
        assert!(!diff.file_diffs.is_empty(), "Should have file diffs");
        let file_diff = &diff.file_diffs[0];
        assert_eq!(file_diff.path, "hello.txt");
        assert!(!file_diff.hunks.is_empty(), "Should have hunks");
        let has_add = file_diff
            .hunks
            .iter()
            .any(|h| h.lines.iter().any(|l| l.line_type == DiffLineType::Add));
        assert!(has_add, "Should have Add lines");
    }

    #[test]
    fn test_untracked_file_diff_shows_all_content() {
        let (dir, repo) = create_test_repo();
        // Add untracked file with known content
        let new_file = dir.path().join("brand_new.txt");
        fs::write(&new_file, "line one\nline two\nline three\n").expect("write brand_new.txt");

        let diff = compute_working_tree_file_diff(&repo, "brand_new.txt").expect("compute diff");
        assert!(!diff.file_diffs.is_empty(), "Should have file diffs");
        let file_diff = &diff.file_diffs[0];
        // All lines should be Add type (D-05)
        for hunk in &file_diff.hunks {
            for line in &hunk.lines {
                assert!(
                    line.line_type == DiffLineType::Add
                        || line.line_type == DiffLineType::HunkHeader,
                    "Untracked file lines should be Add or HunkHeader, got {:?}",
                    line.line_type
                );
            }
        }
    }

    #[test]
    fn test_empty_repo_working_tree() {
        // Init repo WITHOUT any commits
        let dir = tempfile::tempdir().expect("create tempdir");
        let repo = Repository::init(dir.path()).expect("init repo");
        let mut config = repo.config().expect("get config");
        config
            .set_str("user.name", "Test Author")
            .expect("set name");
        config
            .set_str("user.email", "test@example.com")
            .expect("set email");

        // Add a file without committing
        let file_path = dir.path().join("first.txt");
        fs::write(&file_path, "first content\n").expect("write first.txt");

        let files = compute_working_tree_files(&repo).expect("compute working tree files");
        assert!(
            !files.is_empty(),
            "Empty repo should still show untracked files"
        );
        let first = files
            .iter()
            .find(|f| f.path == "first.txt")
            .expect("first.txt should be present");
        assert_eq!(first.status_char, '?');
    }

    // --- Range diff tests (Phase 27, Plan 01) ---

    #[test]
    fn test_compute_range_diff_produces_diff_data() {
        // Create a repo with 3 commits, compute range diff from commit 1 to commit 3
        let (_dir, repo) = create_test_repo_with_n_commits(3);
        let decorations = build_decoration_map(&repo);
        let mut revwalk = repo.revwalk().expect("revwalk");
        revwalk.push_head().unwrap();
        revwalk.set_sorting(Sort::TIME).unwrap();
        let commits = collect_batch(&repo, &mut revwalk, 10, &decorations);
        // commits[0] = newest (Commit 2), commits[2] = oldest (Commit 0)
        let oldest_oid = Oid::from_str(&commits[2].oid).unwrap();
        let newest_oid = Oid::from_str(&commits[0].oid).unwrap();

        let diff = compute_range_diff(&repo, oldest_oid, newest_oid).expect("compute range diff");
        // Should produce a DiffData with files
        assert!(!diff.files.is_empty(), "Range diff should have files");
        assert!(
            !diff.file_diffs.is_empty(),
            "Range diff should have file_diffs"
        );
    }

    #[test]
    fn test_collect_diff_data_produces_file_changes() {
        let (_dir, repo) = create_test_repo();
        let head = repo.head().unwrap();
        let head_commit = head.peel_to_commit().unwrap();
        let parent = head_commit.parent(0).unwrap();
        let diff = repo
            .diff_tree_to_tree(
                Some(&parent.tree().unwrap()),
                Some(&head_commit.tree().unwrap()),
                None,
            )
            .unwrap();

        let data = collect_diff_data(diff).expect("collect_diff_data");
        assert!(!data.files.is_empty(), "Should have file changes");
        assert_eq!(data.files[0].path, "hello.txt");
    }

    #[test]
    fn test_fetch_range_diff_via_provider() {
        let (_dir, repo) = create_test_repo_with_n_commits(3);
        let repo_path = repo.workdir().expect("workdir").to_path_buf();

        // Get OIDs via collect_batch
        let decorations = build_decoration_map(&repo);
        let mut revwalk = repo.revwalk().expect("revwalk");
        revwalk.push_head().unwrap();
        revwalk.set_sorting(Sort::TIME).unwrap();
        let commits = collect_batch(&repo, &mut revwalk, 10, &decorations);
        let oldest_oid = commits[2].oid.clone();
        let newest_oid = commits[0].oid.clone();

        let provider = GitProvider::new(repo_path);
        provider.request_range_diff(&oldest_oid, &newest_oid);
        std::thread::sleep(std::time::Duration::from_millis(300));

        let mut got_range_diff = false;
        while let Some(response) = provider.try_recv() {
            match response {
                GitResponse::RangeDiff(diff) => {
                    got_range_diff = true;
                    assert!(!diff.files.is_empty(), "Range diff should have files");
                }
                _ => {}
            }
        }
        assert!(got_range_diff, "Should receive a RangeDiff response");
    }

    #[test]
    fn test_compute_range_diff_initial_commit() {
        // Range diff including the initial commit (no parent)
        let (_dir, repo) = create_test_repo();
        let decorations = build_decoration_map(&repo);
        let mut revwalk = repo.revwalk().expect("revwalk");
        revwalk.push_head().unwrap();
        revwalk.set_sorting(Sort::TIME).unwrap();
        let commits = collect_batch(&repo, &mut revwalk, 10, &decorations);
        // oldest = initial commit (commits[1]), newest = second commit (commits[0])
        let oldest_oid = Oid::from_str(&commits[1].oid).unwrap();
        let newest_oid = Oid::from_str(&commits[0].oid).unwrap();

        let diff = compute_range_diff(&repo, oldest_oid, newest_oid)
            .expect("compute range diff with initial commit");
        assert!(!diff.files.is_empty(), "Range diff should have files");
    }

    // --- Upstream tracking / ahead-count tests (Phase 44, Plan 01) ---

    #[test]
    fn test_commit_info_construction_with_is_ahead() {
        // Verify CommitInfo can be constructed with is_ahead = true and false
        let ahead_commit = CommitInfo {
            oid: "abc".to_string(),
            summary: "ahead".to_string(),
            body: None,
            author_name: "A".to_string(),
            author_email: "a@b".to_string(),
            time_seconds: 1000,
            time_offset: 0,
            decorations: vec![],
            is_ahead: true,
        };
        assert!(ahead_commit.is_ahead);

        let behind_commit = CommitInfo {
            oid: "def".to_string(),
            summary: "behind".to_string(),
            body: None,
            author_name: "A".to_string(),
            author_email: "a@b".to_string(),
            time_seconds: 999,
            time_offset: 0,
            decorations: vec![],
            is_ahead: false,
        };
        assert!(!behind_commit.is_ahead);
    }

    #[test]
    fn test_get_upstream_oid_no_upstream() {
        // Repo with no upstream configured should return None
        let (_dir, repo) = create_test_repo();
        let result = get_upstream_oid(&repo);
        assert!(
            result.is_none(),
            "Should return None when no upstream configured"
        );
    }

    #[test]
    fn test_get_upstream_oid_detached_head() {
        // Detached HEAD should return None
        let (_dir, repo) = create_test_repo();
        // Detach HEAD by checking out a commit directly
        let head_oid = repo.head().unwrap().target().unwrap();
        repo.set_head_detached(head_oid).unwrap();
        let result = get_upstream_oid(&repo);
        assert!(result.is_none(), "Should return None for detached HEAD");
    }

    #[test]
    fn test_mark_ahead_commits_no_upstream() {
        // When no upstream exists, all commits should remain is_ahead=false
        let (_dir, repo) = create_test_repo();
        let decorations = build_decoration_map(&repo);
        let mut revwalk = repo.revwalk().unwrap();
        revwalk.push_head().unwrap();
        revwalk.set_sorting(Sort::TIME).unwrap();
        let mut commits = collect_batch(&repo, &mut revwalk, 10, &decorations);

        let ahead = mark_ahead_commits(&repo, &mut commits);
        assert_eq!(ahead, 0, "Should return 0 when no upstream");
        for commit in &commits {
            assert!(
                !commit.is_ahead,
                "No commits should be marked ahead without upstream"
            );
        }
    }

    #[test]
    fn test_mark_ahead_commits_detached_head() {
        // When HEAD is detached, mark_ahead_commits should be a no-op
        let (_dir, repo) = create_test_repo();
        let head_oid = repo.head().unwrap().target().unwrap();
        repo.set_head_detached(head_oid).unwrap();

        let decorations = build_decoration_map(&repo);
        let mut revwalk = repo.revwalk().unwrap();
        revwalk.push(head_oid).unwrap();
        revwalk.set_sorting(Sort::TIME).unwrap();
        let mut commits = collect_batch(&repo, &mut revwalk, 10, &decorations);

        let ahead = mark_ahead_commits(&repo, &mut commits);
        assert_eq!(ahead, 0, "Should return 0 for detached HEAD");
        for commit in &commits {
            assert!(
                !commit.is_ahead,
                "No commits should be marked ahead with detached HEAD"
            );
        }
    }

    #[test]
    fn test_read_blob_from_commit() {
        let (_dir, repo) = create_test_repo();
        let head_oid = repo.head().unwrap().target().unwrap();
        let data = read_blob_from_commit(&repo, head_oid, "hello.txt").unwrap();
        assert_eq!(std::str::from_utf8(&data).unwrap(), "hello world\nline 2\n");
    }

    #[test]
    fn test_read_blob_nonexistent_path() {
        let (_dir, repo) = create_test_repo();
        let head_oid = repo.head().unwrap().target().unwrap();
        let result = read_blob_from_commit(&repo, head_oid, "nonexistent.txt");
        assert!(result.is_err());
    }

    #[test]
    fn test_decode_image_bytes_valid_png() {
        // Create a minimal 2x2 RGBA PNG image in memory
        use image::{ImageBuffer, Rgba};
        let mut img_buf: ImageBuffer<Rgba<u8>, Vec<u8>> = ImageBuffer::new(2, 2);
        img_buf.put_pixel(0, 0, Rgba([255, 0, 0, 255]));
        img_buf.put_pixel(1, 0, Rgba([0, 255, 0, 255]));
        img_buf.put_pixel(0, 1, Rgba([0, 0, 255, 255]));
        img_buf.put_pixel(1, 1, Rgba([255, 255, 255, 255]));

        // Encode as PNG bytes
        let mut png_bytes: Vec<u8> = Vec::new();
        let encoder = image::codecs::png::PngEncoder::new(&mut png_bytes);
        image::ImageEncoder::write_image(
            encoder,
            img_buf.as_raw(),
            2,
            2,
            image::ExtendedColorType::Rgba8,
        )
        .unwrap();

        // Decode and verify it succeeds
        let render_image = decode_image_bytes(&png_bytes);
        assert!(
            render_image.is_ok(),
            "decode_image_bytes should succeed for valid PNG"
        );
    }

    #[test]
    fn test_decode_image_bytes_invalid_data() {
        let garbage = b"this is not an image";
        let result = decode_image_bytes(garbage);
        assert!(
            result.is_err(),
            "decode_image_bytes should fail for non-image data"
        );
    }

    #[test]
    fn test_fetch_blob_via_provider() {
        let (_dir, repo) = create_test_repo();
        let repo_path = repo.workdir().expect("workdir").to_path_buf();
        let head_oid = repo.head().unwrap().target().unwrap().to_string();
        let provider = GitProvider::new(repo_path);
        provider.request_blob(&head_oid, "hello.txt");
        std::thread::sleep(std::time::Duration::from_millis(300));
        let mut got_response = false;
        while let Some(response) = provider.try_recv() {
            match response {
                GitResponse::BlobError {
                    commit_oid,
                    path,
                    error: _,
                } => {
                    // For text files, decode_image_bytes will fail. This is expected.
                    got_response = true;
                    assert_eq!(commit_oid, head_oid);
                    assert_eq!(path, "hello.txt");
                }
                _ => {}
            }
        }
        assert!(
            got_response,
            "Should receive a BlobError response (text file can't decode as image)"
        );
    }

    #[test]
    fn test_fetch_blob_nonexistent_file() {
        let (_dir, repo) = create_test_repo();
        let repo_path = repo.workdir().expect("workdir").to_path_buf();
        let head_oid = repo.head().unwrap().target().unwrap().to_string();
        let provider = GitProvider::new(repo_path);
        provider.request_blob(&head_oid, "nonexistent_file.png");
        std::thread::sleep(std::time::Duration::from_millis(300));
        let mut got_error = false;
        while let Some(response) = provider.try_recv() {
            match response {
                GitResponse::BlobError {
                    commit_oid: _,
                    path: _,
                    error,
                } => {
                    got_error = true;
                    assert!(
                        error.contains("Failed to read blob"),
                        "Error should mention blob read failure"
                    );
                }
                _ => {}
            }
        }
        assert!(got_error, "Should receive a BlobError for nonexistent file");
    }

    #[test]
    fn test_mark_ahead_commits_with_upstream() {
        // Create repo, set up a remote tracking ref, make commits ahead of it
        let (_dir, repo) = create_test_repo(); // Has 2 commits on default branch

        // Get the initial commit OID (the oldest one)
        let mut revwalk = repo.revwalk().unwrap();
        revwalk.push_head().unwrap();
        revwalk.set_sorting(Sort::TIME | Sort::REVERSE).unwrap();
        let initial_oid = revwalk.next().unwrap().unwrap();

        // Get current branch name
        let head = repo.head().unwrap();
        let branch_name = head.shorthand().unwrap().to_string();

        // Create a remote "origin" pointing at the repo itself (self-referential for testing)
        repo.remote("origin", ".").unwrap();

        // Create refs/remotes/origin/<branch> pointing at initial commit
        repo.reference(
            &format!("refs/remotes/origin/{}", branch_name),
            initial_oid,
            true,
            "test",
        )
        .unwrap();

        // Set upstream for the current branch
        let mut local_branch = repo
            .find_branch(&branch_name, git2::BranchType::Local)
            .unwrap();
        local_branch
            .set_upstream(Some(&format!("origin/{}", branch_name)))
            .unwrap();

        // Now HEAD has 1 commit ahead of origin/<branch>
        let decorations = build_decoration_map(&repo);
        let mut rw = repo.revwalk().unwrap();
        rw.push_head().unwrap();
        rw.set_sorting(Sort::TIME).unwrap();
        let mut commits = collect_batch(&repo, &mut rw, 10, &decorations);

        let ahead = mark_ahead_commits(&repo, &mut commits);
        assert_eq!(ahead, 1, "Should have 1 commit ahead");
        assert_eq!(commits.len(), 2, "Should have 2 commits total");
        assert!(commits[0].is_ahead, "First (newest) commit should be ahead");
        assert!(
            !commits[1].is_ahead,
            "Second (oldest) commit should NOT be ahead"
        );
    }
}
