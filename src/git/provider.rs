use std::path::PathBuf;
use std::sync::mpsc;

use super::types::*;

/// Requests that can be sent to the git background thread.
pub enum GitRequest {
    FetchLog { count: usize },
    FetchDiff { commit_oid: String },
    FetchStatus,
}

/// Responses from the git background thread.
pub enum GitResponse {
    Log(Vec<CommitInfo>),
    Diff(DiffData),
    Status(BranchStatus),
    Error(String),
}

/// Git data provider that runs operations on a background thread.
pub struct GitProvider {
    request_tx: mpsc::Sender<GitRequest>,
    response_rx: mpsc::Receiver<GitResponse>,
}

impl GitProvider {
    pub fn new(_repo_path: PathBuf) -> Self {
        let (request_tx, _request_rx) = mpsc::channel();
        let (_response_tx, response_rx) = mpsc::channel();
        // Stub -- will be implemented in Task 2
        Self { request_tx, response_rx }
    }

    pub fn request_log(&self, count: usize) {
        let _ = self.request_tx.send(GitRequest::FetchLog { count });
    }

    pub fn request_diff(&self, oid_hex: &str) {
        let _ = self.request_tx.send(GitRequest::FetchDiff { commit_oid: oid_hex.to_string() });
    }

    pub fn request_status(&self) {
        let _ = self.request_tx.send(GitRequest::FetchStatus);
    }

    pub fn try_recv(&self) -> Option<GitResponse> {
        self.response_rx.try_recv().ok()
    }
}
