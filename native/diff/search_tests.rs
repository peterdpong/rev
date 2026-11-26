use gpui::{AppContext, TestAppContext};

use super::{DiffFile, DiffMode, DiffSide, DiffState};

const PATCH: &str = "diff --git a/first.txt b/first.txt\n--- a/first.txt\n+++ b/first.txt\n@@ -1,3 +1,3 @@\n same Needle\n-old needle\n+new NEEDLE\n tail\ndiff --git a/second.txt b/second.txt\n--- a/second.txt\n+++ b/second.txt\n@@ -10 +10 @@\n-ten\n+needle needle\n";

fn found(state: &DiffState) -> Vec<(usize, DiffSide, usize, std::ops::Range<usize>)> {
    state
        .search()
        .matches()
        .iter()
        .map(|found| (found.file, found.side, found.line, found.range.clone()))
        .collect()
}

#[gpui::test]
fn search_finds_shown_lines_in_order_ignoring_case(cx: &mut TestAppContext) {
    let state = cx.new(|cx| DiffState::new(DiffFile::parse(PATCH).unwrap(), cx));
    state.update(cx, |state, cx| {
        state.set_search_query("needle", cx);
        assert_eq!(
            found(state),
            [
                (0, DiffSide::Modified, 0, 5..11),
                (0, DiffSide::Original, 1, 4..10),
                (0, DiffSide::Modified, 1, 4..10),
                (1, DiffSide::Modified, 0, 0..6),
                (1, DiffSide::Modified, 0, 7..13),
            ]
        );
        assert_eq!(state.current_search_match(), Some(0));

        state.set_mode(DiffMode::Split, cx);
        assert_eq!(
            state.search_match_count(),
            6,
            "split shows the unchanged line twice"
        );

        state.set_search_query("", cx);
        assert_eq!(state.search_match_count(), 0);
        assert_eq!(state.current_search_match(), None);
    });
}

#[gpui::test]
fn moving_between_matches_wraps_at_either_end(cx: &mut TestAppContext) {
    let state = cx.new(|cx| DiffState::new(DiffFile::parse(PATCH).unwrap(), cx));
    state.update(cx, |state, cx| {
        state.set_search_query("needle", cx);
        state.select_search_match(false, cx);
        assert_eq!(state.current_search_match(), Some(4));
        state.select_search_match(true, cx);
        assert_eq!(state.current_search_match(), Some(0));
        state.select_search_match(true, cx);
        assert_eq!(state.current_search_match(), Some(1));
    });
}

#[gpui::test]
fn matches_follow_collapsed_files_and_keep_the_current_one(cx: &mut TestAppContext) {
    let state = cx.new(|cx| DiffState::new(DiffFile::parse(PATCH).unwrap(), cx));
    state.update(cx, |state, cx| {
        state.set_search_query("needle", cx);
        state.select_search_match(false, cx);
        assert_eq!(state.current_search_match(), Some(4));

        state.set_file_collapsed("first.txt", true, cx);
        assert_eq!(state.search_match_count(), 2);
        assert_eq!(
            state.current_search_match(),
            Some(1),
            "still the last match"
        );

        state.set_file_collapsed("first.txt", false, cx);
        assert_eq!(state.search_match_count(), 5);
        assert_eq!(state.current_search_match(), Some(4));
    });
}
