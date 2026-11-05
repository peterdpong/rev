use serde::{Deserialize, Serialize};

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

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct CommitRange {
    pub start: String,
    pub end: String,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Review {
    pub repo: String,
    pub name: String,
    pub base: String,
    pub head: String,
    pub mode: String,
    pub branch: String, /// The checked-out branch, or `detached HEAD`.
    pub branch_head_sha: String,
    pub scope_key: String,
    pub commit_range: Option<CommitRange>,
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
