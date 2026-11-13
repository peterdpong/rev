use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewRequest {
    pub repo: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub base: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mode: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit_range: Option<CommitRange>,
}

impl ReviewRequest {
    pub fn working_tree(repo: impl Into<String>) -> Self {
        Self {
            repo: repo.into(),
            head: Some("HEAD".into()),
            mode: Some("working".into()),
            ..Default::default()
        }
    }

    pub fn for_tab(self) -> Self {
        if self.mode.as_deref() == Some("working")
            || (self.mode.is_none()
                && self.head.is_none()
                && self.base.is_none()
                && self.commit_range.is_none())
        {
            Self::working_tree(self.repo)
        } else {
            Self {
                mode: Some("branch".into()),
                ..self
            }
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
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
#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct File {
    pub path: String,
    pub status: String,
    pub added: usize,
    pub removed: usize,
}
#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    #[serde(flatten)]
    pub request: ReviewRequest,
    pub name: String,
    pub key: String,
    pub files: Vec<File>,
    pub branch: String,
    pub branch_head_sha: String,
    pub scope_key: String,
    #[serde(default)]
    pub commit_count: usize,
    #[serde(default)]
    pub head_sha: String,
    #[serde(default)]
    pub merge_base: String,
    #[serde(default)]
    pub head_commit: Option<CommitDetails>,
    pub pull_request: Option<PullRequest>,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
pub struct CommitDetails {
    pub sha: String,
    pub subject: String,
    pub body: String,
    pub author: String,
    pub date: String,
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PullRequest {
    pub number: usize,
    pub title: String,
    pub body: String,
    pub head_ref_name: String,
    pub base_ref_name: String,
    pub comparison_base_ref_name: Option<String>,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub state: String,
    #[serde(default)]
    pub is_draft: bool,
}

impl Review {
    pub fn comparison_label(&self) -> String {
        let mut base = self.request.base.as_deref().unwrap_or("main");
        let mut head = self.request.head.as_deref().unwrap_or("HEAD");

        if head == "HEAD" && !self.branch.is_empty() {
            head = &self.branch;
        }

        if let Some(pr) = &self.pull_request
            && let Some(namespace) = head
                .strip_prefix("refs/rev/")
                .and_then(|head| head.strip_suffix("/head"))
        {
            // Keep explicit comparison bases, including stacked PR parents.
            if base == format!("refs/rev/{namespace}/base") {
                base = pr
                    .comparison_base_ref_name
                    .as_deref()
                    .unwrap_or(&pr.base_ref_name);
            }
            head = &pr.head_ref_name;
        }
        let short = |name: &str| {
            let name = name
                .strip_prefix("refs/heads/")
                .or_else(|| name.strip_prefix("refs/remotes/"))
                .unwrap_or(name);
            name.strip_prefix("origin/").unwrap_or(name).to_owned()
        };
        format!("{}  ←  {}", short(base), short(head))
    }
}

#[derive(Clone, Debug, Default, PartialEq, Deserialize)]
pub struct ReviewState {
    pub viewed: HashMap<String, String>,
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FilePatch {
    pub patch: String,
    pub digest: String,
    pub too_large: bool,
}
#[derive(Deserialize)]
pub struct ReviewPatch {
    pub path: String,
    #[serde(flatten)]
    pub patch: FilePatch,
    pub error: Option<String>,
}

impl FilePatch {
    /// Shown instead of a diff when the patch can't be displayed inline.
    pub fn notice(&self) -> Option<String> {
        if self.too_large {
            Some("This patch exceeds the 2 MB inline limit.".into())
        } else if self.patch == "Binary file (untracked)" {
            Some("Binary file".into())
        } else {
            None
        }
    }
}

#[derive(Clone, Debug, Deserialize)]
pub struct WorkingTreeState {
    pub repo: String,
    pub branch: String,
    pub sha: String,
    pub revision: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkingReview {
    pub review: Review,
    pub snapshot_id: String,
    pub state: ReviewState,
    pub path: Option<String>,
    #[serde(default)]
    pub patch: FilePatch,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tab_requests_separate_working_trees_from_committed_comparisons() {
        let empty = ReviewRequest {
            repo: "/repo".into(),
            ..Default::default()
        };
        assert_eq!(
            empty.clone().for_tab(),
            ReviewRequest::working_tree("/repo")
        );
        let committed = ReviewRequest {
            mode: Some("branch".into()),
            ..empty.clone()
        }
        .for_tab();
        assert_eq!(committed.mode.as_deref(), Some("branch"));
        let named = ReviewRequest {
            head: Some("feature".into()),
            ..empty
        }
        .for_tab();
        assert_eq!(named.mode.as_deref(), Some("branch"));
        let working = ReviewRequest {
            base: Some("old-base".into()),
            commit_range: Some(CommitRange {
                start: "a".into(),
                end: "b".into(),
            }),
            mode: Some("working".into()),
            ..named
        };
        assert_eq!(working.for_tab(), ReviewRequest::working_tree("/repo"));
    }

    fn imported_review() -> Review {
        serde_json::from_value(serde_json::json!({
            "repo": "/repo", "base": "refs/rev/owner/repo/42/base",
            "head": "refs/rev/owner/repo/42/head", "name": "repo", "key": "review",
            "branches": [], "files": [], "dirty": false, "branch": "main",
            "branchHeadSha": "abc", "scopeKey": "scope",
            "pullRequest": {
                "number": 42, "title": "Improve the header", "body": "Description\nwith details.",
                "headRefName": "feature", "baseRefName": "main",
                "comparisonBaseRefName": "parent-feature"
            }
        }))
        .unwrap()
    }

    #[test]
    fn pr_metadata_and_comparison_use_readable_branch_names() {
        let mut review = imported_review();
        let pr = review.pull_request.as_ref().unwrap();
        assert_eq!(pr.number, 42);
        assert_eq!(pr.title, "Improve the header");
        assert_eq!(pr.body, "Description\nwith details.");
        assert_eq!(review.comparison_label(), "parent-feature  ←  feature");
        review
            .pull_request
            .as_mut()
            .unwrap()
            .comparison_base_ref_name = None;
        assert_eq!(review.comparison_label(), "main  ←  feature");
        review.request.base = Some("refs/remotes/origin/custom-base".into());
        assert_eq!(review.comparison_label(), "custom-base  ←  feature");
    }

    #[test]
    fn local_comparison_keeps_selected_refs_with_or_without_a_pr() {
        let mut review = imported_review();
        review.request.base = Some("origin/main".into());
        review.request.head = Some("refs/heads/local-feature".into());
        assert_eq!(review.comparison_label(), "main  ←  local-feature");
        review.pull_request = None;
        assert_eq!(review.comparison_label(), "main  ←  local-feature");
        review.request.head = Some("HEAD".into());
        review.branch = "checked-out-feature".into();
        assert_eq!(review.comparison_label(), "main  ←  checked-out-feature");
    }

}
