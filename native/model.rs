use serde::{Deserialize, Serialize};

#[derive(Clone, Default, Serialize)]
pub struct ReviewRequest {
    pub repo: String,
    pub base: Option<String>,
    pub head: Option<String>,
    pub mode: Option<String>,
}
#[derive(Deserialize)]
pub struct Review {
    pub name: String,
    pub base: String,
    pub head: String,
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
