use gpui_kit::component::diff::{DiffLineRange, DiffSide};
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
}
#[derive(Clone, Debug, Deserialize)]
pub struct Review {
    #[serde(flatten)]
    pub request: ReviewRequest,
    pub name: String,
    pub key: String,
    pub branches: Vec<String>,
    pub files: Vec<File>,
    pub dirty: bool,
    #[serde(rename = "branchHeadSha")]
    pub branch_head_sha: String,
    #[serde(rename = "commitCount", default)]
    pub commit_count: usize,
    #[serde(rename = "scopeKey")]
    pub scope_key: String,
}
#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
pub struct Selection {
    pub path: String,
    pub side: String,
    pub start: usize,
    pub end: usize,
}
impl Selection {
    pub fn from_range(range: &DiffLineRange) -> Option<Self> {
        // A cross-side unified selection has two coordinate systems. Never
        // silently turn it into an incorrect source range for the agent.
        if range.side() != range.end_side() {
            return None;
        }
        Some(Self {
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
    pub fn diff_side(&self) -> DiffSide {
        if self.side == "deletions" {
            DiffSide::Original
        } else {
            DiffSide::Modified
        }
    }
    pub fn label(&self) -> String {
        format!("{}:{}–{} · {}", self.path, self.start, self.end, self.side)
    }
}
#[derive(Clone, Debug, Deserialize)]
pub struct Comment {
    #[serde(flatten)]
    pub selection: Selection,
    pub id: String,
    pub body: String,
    pub status: String,
    pub digest: String,
}
#[derive(Clone, Debug, Default, Deserialize)]
pub struct ReviewState {
    pub comments: Vec<Comment>,
    pub viewed: HashMap<String, String>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct ReviewTab {
    pub id: String,
    #[serde(rename = "scopeKey", default)]
    pub scope_key: Option<String>,
    pub label: String,
    pub request: ReviewRequest,
    pub file: Option<String>,
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
}
#[derive(Clone, Debug, Deserialize)]
pub struct Provider {
    pub id: String,
    pub available: bool,
}
#[derive(Clone, Debug, Deserialize)]
pub struct AgentEvent {
    pub kind: String,
    pub text: String,
}
#[derive(Clone, Debug, Deserialize)]
pub struct AgentRun {
    pub id: String,
    pub status: String,
    pub events: Vec<AgentEvent>,
    pub mode: String,
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
