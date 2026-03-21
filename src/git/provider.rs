use std::cell::RefCell;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;

use git2::{Oid, Repository, Sort, StatusOptions};

use super::types::*;

/// Requests that can be sent to the git background thread.
pub enum GitRequest {
    FetchLog { count: usize },
    FetchMoreLog { batch_size: usize },
    FetchDiff { commit_oid: String },
    FetchStatus,
}

/// Responses from the git background thread.
pub enum GitResponse {
    Log(Vec<CommitInfo>),
    MoreLog {
        commits: Vec<CommitInfo>,
        exhausted: bool,
    },
    Diff(DiffData),
    Status(BranchStatus),
    Error(String),
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

            while let Ok(request) = request_rx.recv() {
                let response = match request {
                    GitRequest::FetchLog { count } => {
                        // Fresh load: reset revwalk state
                        active_revwalk = None;
                        revwalk_exhausted = false;
                        let decorations = build_decoration_map(&repo);
                        match repo.revwalk() {
                            Ok(mut revwalk) => {
                                revwalk.push_head().ok();
                                revwalk.set_sorting(Sort::TIME).ok();
                                let commits =
                                    collect_batch(&repo, &mut revwalk, count, &decorations);
                                revwalk_exhausted = commits.len() < count;
                                active_revwalk = Some(revwalk);
                                GitResponse::Log(commits)
                            }
                            Err(e) => GitResponse::Error(e.to_string()),
                        }
                    }
                    GitRequest::FetchMoreLog { batch_size } => {
                        if revwalk_exhausted {
                            GitResponse::MoreLog {
                                commits: vec![],
                                exhausted: true,
                            }
                        } else {
                            let decorations = build_decoration_map(&repo);
                            match active_revwalk.as_mut() {
                                Some(revwalk) => {
                                    let commits =
                                        collect_batch(&repo, revwalk, batch_size, &decorations);
                                    let exhausted = commits.len() < batch_size;
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
                    GitRequest::FetchStatus => match get_branch_status(&repo) {
                        Ok(status) => GitResponse::Status(status),
                        Err(e) => GitResponse::Error(e.to_string()),
                    },
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
        let _ = self.request_tx.send(GitRequest::FetchLog { count });
    }

    /// Request the diff for the given commit OID (hex string).
    pub fn request_diff(&self, oid_hex: &str) {
        let _ = self.request_tx.send(GitRequest::FetchDiff {
            commit_oid: oid_hex.to_string(),
        });
    }

    /// Request the current branch status (name + dirty flag).
    pub fn request_status(&self) {
        let _ = self.request_tx.send(GitRequest::FetchStatus);
    }

    /// Request more commits from the persistent revwalk (incremental batch).
    pub fn request_more_log(&self, batch_size: usize) {
        let _ = self.request_tx.send(GitRequest::FetchMoreLog { batch_size });
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
    let mut commits = Vec::with_capacity(count);
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
                        summary: commit.summary().unwrap_or("").to_string(),
                        body: commit.body().map(|b| b.to_string()),
                        author_name: author.name().unwrap_or("").to_string(),
                        author_email: author.email().unwrap_or("").to_string(),
                        time_seconds: time.seconds(),
                        time_offset: time.offset_minutes(),
                        decorations: commit_decorations,
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


/// Build a map from commit OID to branch/tag decorations.
fn build_decoration_map(repo: &Repository) -> HashMap<Oid, Vec<Decoration>> {
    let mut map: HashMap<Oid, Vec<Decoration>> = HashMap::new();

    // Branches
    if let Ok(branches) = repo.branches(None) {
        for branch_result in branches {
            if let Ok((branch, _)) = branch_result {
                let name = match branch.name() {
                    Ok(Some(n)) => n.to_string(),
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
                        name: tag_name.to_string(),
                    });
                }
            }
        }
    }

    map
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

    // Collect file changes from deltas
    let mut files: Vec<FileChange> = Vec::new();
    for delta in diff.deltas() {
        let path = delta
            .new_file()
            .path()
            .unwrap_or(std::path::Path::new(""))
            .to_string_lossy()
            .to_string();
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
        });
    }

    // Collect per-file diffs with hunks and lines using RefCell
    // to share mutable state across the four diff.foreach closures.
    let file_diffs: RefCell<Vec<FileDiff>> = RefCell::new(Vec::new());
    let current_file: RefCell<Option<FileDiff>> = RefCell::new(None);
    let current_hunk: RefCell<Option<DiffHunk>> = RefCell::new(None);

    diff.foreach(
        &mut |delta, _progress| {
            // File callback: start a new file
            if let Some(mut file) = current_file.borrow_mut().take() {
                if let Some(hunk) = current_hunk.borrow_mut().take() {
                    file.hunks.push(hunk);
                }
                file_diffs.borrow_mut().push(file);
            }
            let path = delta
                .new_file()
                .path()
                .unwrap_or(std::path::Path::new(""))
                .to_string_lossy()
                .to_string();
            *current_file.borrow_mut() = Some(FileDiff {
                path,
                additions: 0,
                deletions: 0,
                hunks: Vec::new(),
            });
            true
        },
        None, // binary callback
        Some(&mut |_delta, hunk| {
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
            // Line callback: add line to current hunk
            let line_type = match line.origin() {
                '+' => DiffLineType::Add,
                '-' => DiffLineType::Remove,
                ' ' => DiffLineType::Context,
                'H' | 'F' => DiffLineType::HunkHeader,
                _ => DiffLineType::Context,
            };

            let content = std::str::from_utf8(line.content())
                .unwrap_or("")
                .to_string();

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
        assert_eq!(batch3.len(), 1, "Third batch should have 1 remaining commit");

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

        assert_eq!(commits.len(), 2, "Should return 2 commits from 2-commit repo");
        assert!(
            commits.len() < 10,
            "Returned fewer than requested signals exhaustion"
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
}
