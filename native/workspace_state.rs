use crate::model::{Review, ReviewRequest};
use serde::{Deserialize, Serialize};

const DETACHED: &str = "detached HEAD";

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceState {
    #[serde(default)]
    pub projects: Vec<Project>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active_tab_id: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub repo: String,
    pub name: String,
    #[serde(default)]
    pub collapsed: bool,
    #[serde(default)]
    pub tabs: Vec<ReviewTab>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TabKind {
    /// Follows whatever the project has checked out.
    Current,
    Branch,
    Pr,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReviewTab {
    pub id: String,
    pub kind: TabKind,
    pub label: String,
    pub request: ReviewRequest,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub scope_key: Option<String>,
    /// The checkout's HEAD when a current tab was last refreshed.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub head_sha: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
}

impl WorkspaceState {
    pub fn active_tab(&self) -> Option<&ReviewTab> {
        let id = self.active_tab_id.as_ref()?;
        self.tabs().find(|tab| &tab.id == id)
    }

    fn tabs(&self) -> impl Iterator<Item = &ReviewTab> {
        self.projects.iter().flat_map(|p| p.tabs.iter())
    }

    /// Create or update the review's tab, make it active.
    /// With no head or `HEAD`, use the Current tab, which follows the checked-out
    /// branch. Otherwise, use a tab for the requested base and head.
    /// Switching between branch and working-tree review modes reuses the same tab.
    pub fn remember_review(&mut self, review: &Review, request: &ReviewRequest) -> String {
        let index = match self.projects.iter().position(|p| p.repo == review.repo) {
            Some(index) => index,
            None => {
                self.projects.push(Project {
                    repo: review.repo.clone(),
                    name: review.name.clone(),
                    collapsed: false,
                    tabs: vec![],
                });
                self.projects.len() - 1
            }
        };

        let project = &mut self.projects[index];
        project.collapsed = false;

        let follows_checkout = request.head.as_deref().is_none_or(|head| head == "HEAD");
        let id = if follows_checkout {
            let tab = current_tab(project, &review.branch, &review.branch_head_sha);
            tab.scope_key = Some(review.scope_key.clone());
            tab.request = ReviewRequest {
                repo: review.repo.clone(),
                // Keep the base unset unless chosen so it's resolved for each branch.
                base: request.base.clone(),
                head: Some("HEAD".into()),
                mode: Some(review.mode.clone()),
                commit_range: review.commit_range.clone(),
            };
            tab.id.clone()
        } else {
            let head = &review.head;
            let pr = extract_pr_number(head);
            let tab = ReviewTab {
                id: format!("{}#{}...{}", review.repo, review.base, head),
                kind: if pr.is_some() {
                    TabKind::Pr
                } else {
                    TabKind::Branch
                },
                label: match pr {
                    Some(number) => format!("PR #{number}"),
                    None => head
                        .strip_prefix("refs/heads/")
                        .or_else(|| head.strip_prefix("refs/remotes/"))
                        .unwrap_or(head)
                        .to_string(),
                },
                request: ReviewRequest {
                    repo: review.repo.clone(),
                    base: Some(review.base.clone()),
                    head: Some(head.clone()),
                    mode: Some(review.mode.clone()),
                    commit_range: review.commit_range.clone(),
                },
                scope_key: Some(review.scope_key.clone()),
                head_sha: None,
                file: None,
            };
            let id = tab.id.clone();
            match project.tabs.iter_mut().find(|t| t.id == id) {
                Some(existing) => {
                    let file = existing.file.take();
                    *existing = ReviewTab { file, ..tab };
                }
                None => project.tabs.push(tab),
            }
            id
        };

        self.active_tab_id = Some(id.clone());
        id
    }

    #[allow(dead_code)]
    pub fn close_tab(&mut self, id: &str) {
        let Some(index) = self.tabs().position(|t| t.id == id) else {
            return;
        };

        if self
            .tabs()
            .any(|t| t.id == id && t.kind == TabKind::Current)
        {
            return;
        }

        for project in &mut self.projects {
            project.tabs.retain(|t| t.id != id);
        }

        // If the closed tab is active set the new active tab to the nearest neighbour.
        if self.active_tab_id.as_deref() == Some(id) {
            let remaining: Vec<_> = self.tabs().map(|t| t.id.clone()).collect();
            self.active_tab_id = remaining
                .get(
                    index
                        .saturating_sub(1)
                        .min(remaining.len().saturating_sub(1)),
                )
                .cloned();
        }
    }

    #[allow(dead_code)]
    pub fn remove_project(&mut self, repo: &str) {
        self.projects.retain(|p| p.repo != repo);
        if self.active_tab().is_none() {
            let first = self.tabs().next().map(|t| t.id.clone());
            self.active_tab_id = first;
        }
    }

    #[allow(dead_code)]
    pub fn collapse_project(&mut self, repo: &str, collapsed: bool) {
        if let Some(project) = self.projects.iter_mut().find(|p| p.repo == repo) {
            project.collapsed = collapsed;
        }
    }

    #[allow(dead_code)]
    pub fn set_file(&mut self, id: &str, file: String) {
        if let Some(tab) = self
            .projects
            .iter_mut()
            .flat_map(|p| p.tabs.iter_mut())
            .find(|t| t.id == id)
        {
            tab.file = Some(file);
        }
    }
}

fn current_tab<'a>(project: &'a mut Project, branch: &str, sha: &str) -> &'a mut ReviewTab {
    let label = if branch == DETACHED {
        format!("Detached {}", &sha[..sha.len().min(7)])
    } else {
        branch.to_string()
    };

    let index = match project.tabs.iter().position(|t| t.kind == TabKind::Current) {
        Some(index) => {
            let tab = &mut project.tabs[index];

            // The tab now shows a different branch or commit.
            // Clear selections that belonged to the previous one.
            if tab.label != label {
                tab.request.commit_range = None;
                tab.scope_key = None;
                tab.file = None;
            }

            tab.label = label;
            tab.head_sha = Some(sha.to_string());
            index
        }
        None => {
            project.tabs.insert(
                0,
                ReviewTab {
                    id: format!("{}#current", project.repo),
                    kind: TabKind::Current,
                    label,
                    request: ReviewRequest {
                        repo: project.repo.clone(),
                        head: Some("HEAD".into()),
                        ..Default::default()
                    },
                    scope_key: None,
                    head_sha: Some(sha.to_string()),
                    file: None,
                },
            );
            0
        }
    };

    &mut project.tabs[index]
}

/// The PR number in a ref imported as `refs/rev/<owner>/<repo>/<number>/head`.
fn extract_pr_number(head: &str) -> Option<&str> {
    let parts: Vec<_> = head.strip_prefix("refs/rev/")?.split('/').collect();
    match parts.as_slice() {
        [owner, repo, number, "head"]
            if !owner.is_empty()
                && !repo.is_empty()
                && !number.is_empty()
                && number.bytes().all(|b| b.is_ascii_digit()) =>
        {
            Some(number)
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn review(repo: &str, head: &str, mode: &str) -> Review {
        Review {
            repo: repo.into(),
            name: repo.trim_start_matches('/').into(),
            base: "main".into(),
            head: head.into(),
            mode: mode.into(),
            branch: "feature".into(),
            branch_head_sha: "0123456789abcdef".into(),
            scope_key: format!("{repo}:{head}:{mode}"),
            commit_range: None,
        }
    }

    fn open(repo: &str, head: Option<&str>) -> ReviewRequest {
        ReviewRequest {
            repo: repo.into(),
            head: head.map(Into::into),
            ..Default::default()
        }
    }

    #[test]
    fn tabs_group_by_repo_and_keep_the_selected_file_across_modes() {
        let mut workspace = WorkspaceState::default();
        let id = workspace.remember_review(
            &review("/a", "feature", "branch"),
            &open("/a", Some("feature")),
        );
        workspace.set_file(&id, "file.ts".into());

        let reopened = workspace.remember_review(
            &review("/a", "feature", "working"),
            &open("/a", Some("feature")),
        );
        assert_eq!(reopened, id);
        let tab = workspace.active_tab().unwrap();
        assert_eq!(tab.file.as_deref(), Some("file.ts"));
        assert_eq!(tab.request.mode.as_deref(), Some("working"));
        assert_eq!(workspace.projects[0].tabs.len(), 1);

        workspace.remember_review(
            &review("/b", "feature", "branch"),
            &open("/b", Some("feature")),
        );
        assert_eq!(workspace.projects.len(), 2);
    }

    #[test]
    fn reviews_of_the_checkout_share_one_current_tab() {
        let mut workspace = WorkspaceState::default();
        let first = workspace.remember_review(&review("/a", "HEAD", "branch"), &open("/a", None));
        let second =
            workspace.remember_review(&review("/a", "HEAD", "working"), &open("/a", Some("HEAD")));
        assert_eq!(first, second);
        let tab = workspace.active_tab().unwrap();
        assert_eq!(tab.kind, TabKind::Current);
        assert_eq!(tab.label, "feature");
        assert_eq!(tab.request.base, None);
    }

    #[test]
    fn switching_branches_resets_the_current_tab() {
        let mut workspace = WorkspaceState::default();
        let id = workspace.remember_review(&review("/a", "HEAD", "branch"), &open("/a", None));
        workspace.set_file(&id, "file.ts".into());

        let mut detached = review("/a", "HEAD", "branch");
        detached.branch = DETACHED.into();
        workspace.remember_review(&detached, &open("/a", None));
        let tab = workspace.active_tab().unwrap();
        assert_eq!(tab.label, "Detached 0123456");
        assert_eq!(tab.file, None);
    }

    #[test]
    fn pr_tabs_are_labeled_and_closing_the_active_tab_selects_a_neighbour() {
        let mut workspace = WorkspaceState::default();
        let current = workspace.remember_review(&review("/a", "HEAD", "branch"), &open("/a", None));
        let branch = workspace.remember_review(
            &review("/a", "feature", "branch"),
            &open("/a", Some("feature")),
        );
        let head = "refs/rev/owner/project/42/head";
        let pr = workspace.remember_review(&review("/a", head, "branch"), &open("/a", Some(head)));
        assert_eq!(workspace.active_tab().unwrap().label, "PR #42");
        assert_eq!(workspace.active_tab().unwrap().kind, TabKind::Pr);

        workspace.close_tab(&pr);
        assert_eq!(workspace.active_tab_id.as_deref(), Some(branch.as_str()));
        workspace.close_tab(&current);
        assert_eq!(
            workspace.projects[0].tabs.len(),
            2,
            "current tabs stay open"
        );

        workspace.remove_project("/a");
        assert!(workspace.projects.is_empty());
        assert_eq!(workspace.active_tab_id, None);
    }

    #[test]
    fn only_imported_pr_refs_get_pr_labels() {
        assert_eq!(extract_pr_number("refs/rev/o/r/7/head"), Some("7"));
        assert_eq!(extract_pr_number("refs/rev/o/r/x/head"), None);
        assert_eq!(extract_pr_number("refs/heads/7/head"), None);
    }
}
