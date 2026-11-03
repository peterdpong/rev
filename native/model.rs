use crate::diff::{DiffLineRange, DiffSide};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
pub struct ReviewRequest {
    pub repo: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(
        rename = "commitRange",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub commit_range: Option<CommitRange>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct CommitRange {
    pub start: String,
    pub end: String,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Commit {
    pub sha: String,
    pub subject: String,
    pub author: String,
    pub date: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CommitPage {
    pub commits: Vec<Commit>,
    pub has_more: bool,
}
#[derive(Clone, Debug, Deserialize)]
pub struct File {
    pub path: String,
    pub status: String,
    pub added: usize,
    pub removed: usize,
    #[serde(default)]
    pub ignored: bool,
}
#[derive(Clone, Debug, Deserialize)]
pub struct Review {
    #[serde(flatten)]
    pub request: ReviewRequest,
    pub name: String,
    #[serde(default)]
    pub branch: String,
    pub key: String,
    pub files: Vec<File>,
    pub dirty: bool,
    #[serde(rename = "branchHeadSha")]
    pub branch_head_sha: String,
    #[serde(rename = "commitCount", default)]
    pub commit_count: usize,
    #[serde(rename = "scopeKey")]
    pub scope_key: String,
    #[serde(rename = "headSha", default)]
    pub head_sha: String,
    #[serde(rename = "mergeBase", default)]
    pub merge_base: String,
    #[serde(rename = "headCommit", default)]
    pub head_commit: Option<CommitDetails>,
    #[serde(rename = "pullRequest", default)]
    pub pull_request: Option<PullRequest>,
}
impl Review {
    pub fn base_label(&self) -> &str {
        let base = self.request.base.as_deref().unwrap_or_default();
        let automatic_pr = self.request.head.as_ref().is_some_and(|head| {
            head.strip_suffix("/head")
                .is_some_and(|prefix| base == format!("{prefix}/base"))
        });
        if automatic_pr && let Some(pr) = &self.pull_request {
            return pr
                .comparison_base_ref_name
                .as_deref()
                .unwrap_or(&pr.base_ref_name);
        }
        let base = base
            .strip_prefix("refs/rev/bases/")
            .and_then(|value| value.split_once('/'))
            .map_or(base, |(_, branch)| branch);
        base.trim_start_matches("refs/heads/")
            .trim_start_matches("refs/remotes/")
    }

    pub fn pin_current_request(&mut self) {
        if self
            .request
            .head
            .as_deref()
            .is_none_or(|head| head == "HEAD")
        {
            self.request.head = Some(if self.branch == "detached HEAD" {
                self.branch_head_sha.clone()
            } else {
                self.branch.clone()
            });
        }
    }
}
#[derive(Clone, Debug, Deserialize)]
pub struct CommitDetails {
    pub sha: String,
    pub subject: String,
    pub body: String,
    pub author: String,
    pub date: String,
}
#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    pub number: usize,
    pub title: String,
    pub body: String,
    pub url: String,
    pub author: String,
    pub state: String,
    pub is_draft: bool,
    pub head_ref_name: String,
    pub base_ref_name: String,
    #[serde(default)]
    pub comparison_base_ref_name: Option<String>,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Selection {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope: Option<String>,
    pub path: String,
    pub side: String,
    pub start: usize,
    pub end: usize,
}
impl Selection {
    pub fn from_range(range: &DiffLineRange) -> Option<Self> {
        // A cross-side unified selection has two coordinate systems. Never
        // silently turn it into an incorrect source range for the selected range.
        if range.side() != range.end_side() {
            return None;
        }
        Some(Self {
            scope: None,
            path: range.path().to_string(),
            side: match range.side() {
                DiffSide::Original => "deletions",
                DiffSide::Modified => "additions",
            }
            .into(),
            start: range.start().min(range.end()),
            end: range.start().max(range.end()),
        })
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ReviewState {
    pub viewed: HashMap<String, String>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct ReviewTab {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub kind: String,
    #[serde(rename = "currentHeadSha", default)]
    pub current_head_sha: Option<String>,
    pub request: ReviewRequest,
    pub file: Option<String>,
}
impl ReviewTab {
    pub fn is_current(&self) -> bool {
        self.kind == "current"
    }
}
#[derive(Clone, Debug, Deserialize)]
pub struct Project {
    pub repo: String,
    pub name: String,
    pub collapsed: bool,
    pub tabs: Vec<ReviewTab>,
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub projects: Vec<Project>,
    pub active_tab_id: Option<String>,
}
impl Workspace {
    pub fn active(&self) -> Option<&ReviewTab> {
        self.projects
            .iter()
            .flat_map(|p| &p.tabs)
            .find(|t| Some(&t.id) == self.active_tab_id.as_ref())
    }
}
#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePatch {
    pub patch: String,
    pub digest: String,
    pub too_large: bool,
    #[serde(default)]
    pub deferred: bool,
}
#[derive(Deserialize)]
pub struct ReviewPatch {
    pub path: String,
    #[serde(flatten)]
    pub patch: FilePatch,
    pub error: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn range_keeps_source_side_and_normalizes_reverse_drag() {
        let range = DiffLineRange::new("file.rs", DiffSide::Original, 12, 3);
        let selection = Selection::from_range(&range).unwrap();
        assert_eq!(
            (selection.start, selection.end, selection.side.as_str()),
            (3, 12, "deletions")
        );
        assert!(Selection::from_range(&range.with_end_side(DiffSide::Modified)).is_none());
    }
}
