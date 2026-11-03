#![allow(dead_code)]
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
        // silently turn it into an incorrect source range for the selected range.
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

#[derive(Clone, Debug, Default, Deserialize)]
pub struct ReviewState {
    pub viewed: HashMap<String, String>,
}
#[derive(Clone, Debug, Deserialize)]
pub struct ReviewTab {
    pub id: String,
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

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewPatch {
    pub path: String,
    pub patch: String,
    pub too_large: bool,
    pub error: Option<String>,
}
#[derive(Deserialize)]
pub struct ReviewResponse {
    pub review: Review,
    pub patches: Vec<ReviewPatch>,
}
