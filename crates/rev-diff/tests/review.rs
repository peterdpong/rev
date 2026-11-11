use rev_diff::{File, LineKind, Mode, Position, Review, Row, Selection, Side, Status, tokens};
use std::sync::Arc;

fn added(path: &str, count: usize) -> File {
    let mut patch = format!(
        "diff --git a/{path} b/{path}\nnew file mode 100644\n--- /dev/null\n+++ b/{path}\n@@ -0,0 +1,{count} @@\n"
    );
    for i in 0..count {
        patch.push_str(&format!("+line {i}\n"));
    }
    File::parse(&patch).unwrap().remove(0)
}

#[test]
fn source_coordinates_and_exact_copy_survive_split_alignment() {
    let patch = "diff --git a/file b/file\n--- a/file\n+++ b/file\n@@ -20,4 +30,3 @@ scope\n keep\r\n-old one\n-old two\n+new\tλ\n tail\n\\ No newline at end of file\n";
    let file = File::parse(patch).unwrap().remove(0);
    assert_eq!((file.additions(), file.deletions()), (1, 2));
    assert_eq!(
        file.copy(&Selection::new("file", Side::Modified, 32, 30))
            .unwrap(),
        "keep\r\nnew\tλ\ntail"
    );
    assert_eq!(
        file.copy(&Selection::new("file", Side::Original, 21, 22))
            .unwrap(),
        "old one\nold two\n"
    );
    let mut review = Review::new([file], Mode::Unified);
    let source = Position::new("file", Side::Modified, 31);
    let anchor = review.anchor(review.source_row(&source).unwrap()).unwrap();
    review.set_mode(Mode::Split);
    let row = review.resolve(&anchor).unwrap();
    assert_eq!(review.position(row, Side::Modified), Some(source));
    assert_eq!(review.position(row, Side::Original).unwrap().line(), 21);
    let remainder = review
        .source_row(&Position::new("file", Side::Original, 22))
        .unwrap();
    assert_eq!(review.position(remainder, Side::Modified), None);
}

#[test]
fn quoted_paths_add_delete_rename_and_binary_files() {
    let patch = "diff --git \"a/line\\012caf\\303\\251.ts\" \"b/line\\012caf\\303\\251.ts\"\nnew file mode 100644\n--- /dev/null\n+++ \"b/line\\012caf\\303\\251.ts\"\n@@ -0,0 +1 @@\n+hello\ndiff --git a/gone b/gone\ndeleted file mode 100644\n--- a/gone\n+++ /dev/null\n@@ -1 +0,0 @@\n-goodbye\ndiff --git a/old name b/new name\nsimilarity index 100%\nrename from old name\nrename to new name\ndiff --git a/image b/image\nBinary files a/image and b/image differ\n";
    let files = File::parse(patch).unwrap();
    assert_eq!(
        files.iter().map(|f| f.path().as_str()).collect::<Vec<_>>(),
        ["line\ncafé.ts", "gone", "new name", "image"]
    );
    assert_eq!(
        files.iter().map(File::status).collect::<Vec<_>>(),
        [
            Status::Added,
            Status::Deleted,
            Status::Renamed,
            Status::Binary
        ]
    );
    assert_eq!(files[3].message(), Some("Binary file changed"));
}

#[test]
fn sparse_ranges_never_copy_invented_context() {
    let file = File::parse("--- file\n+++ file\n@@ -1 +1 @@\n first\n@@ -10 +10 @@\n tenth\n")
        .unwrap()
        .remove(0);
    assert!(
        file.copy(&Selection::new("file", Side::Modified, 1, 10))
            .is_err()
    );
    assert_eq!(
        file.copy(&Selection::new("file", Side::Modified, 10, 10))
            .unwrap(),
        "tenth\n"
    );
    assert!(
        file.copy(&Selection::new("file", Side::Original, 1, 1).with_end_side(Side::Modified))
            .is_err()
    );
}

#[test]
fn malformed_patches_fail_without_partial_documents() {
    for patch in [
        "--- a/x\n+++ b/x\n@@ -1,2 +1,2 @@\n one\n",
        "--- a/x\n+++ b/x\n@@ -1 +1 @@\n-one\n+one\n+extra\n",
        "--- a/x\n+++ b/x\n@@ -0,1 +1 @@\n one\n",
        "--- a/x\n+++ b/x\n@@ -1 +1 @@\n one\n@@ -1 +1 @@\n duplicate\n",
        "diff --cc x\n@@@ -1 -1 +1 @@@\n",
        "\\ No newline at end of file\n",
    ] {
        assert!(File::parse(patch).is_err(), "accepted {patch:?}");
    }
    let overflow = format!("--- a/x\n+++ b/x\n@@ -{},2 +1 @@\n one\n", usize::MAX);
    assert!(File::parse(&overflow).is_err());
}

#[test]
fn header_like_source_is_not_interpreted_as_metadata() {
    let file = File::parse("diff --git a/x b/x\n--- a/x\n+++ b/x\n@@ -1,2 +1,2 @@\n--- a/not-a-path\n+++ b/not-a-path\n diff --git a/y b/y\n").unwrap().remove(0);
    assert_eq!(file.path(), "x");
    assert_eq!(file.lines()[0].kind, LineKind::Removed);
    assert_eq!(file.text(1), "++ b/not-a-path");
}

#[test]
fn folds_keep_source_mapping_and_selection() {
    let mut patch = "--- x\n+++ x\n@@ -1,40 +1,40 @@\n".to_string();
    for i in 0..40 {
        patch.push_str(&format!(" line {i}\n"));
    }
    let mut review = Review::new(File::parse(&patch).unwrap(), Mode::Unified);
    let source = Position::new("x", Side::Modified, 20);
    let row = review.source_row(&source).unwrap();
    let Row::Fold(range) = review.row(row).unwrap().1 else {
        panic!("context should fold")
    };
    let start = range.start;
    review.expand_fold(0, start);
    let row = review.source_row(&source).unwrap();
    assert_eq!(review.position(row, Side::Modified), Some(source));
    assert_eq!(
        review
            .file(0)
            .copy(&Selection::new("x", Side::Modified, 3, 4))
            .unwrap(),
        "line 2\nline 3\n"
    );
}

#[test]
fn changes_to_other_files_preserve_rows_and_source_anchors() {
    let mut review = Review::new([added("a", 12), added("b", 20)], Mode::Unified);
    let b = review.rows(1).clone();
    let source = Position::new("b", Side::Modified, 8);
    let anchor = review.anchor(review.source_row(&source).unwrap()).unwrap();
    let before = review.resolve(&anchor).unwrap();
    review.replace(added("a", 60)).unwrap();
    assert!(Arc::ptr_eq(&b, review.rows(1)));
    assert_eq!(review.resolve(&anchor), Some(before + 48));
    review.collapse("a", true).unwrap();
    assert_eq!(
        review.position(review.resolve(&anchor).unwrap(), Side::Modified),
        Some(source)
    );
    assert!(Arc::ptr_eq(&b, review.rows(1)));
}

#[test]
fn drag_stays_in_the_anchor_file_and_side() {
    let review = Review::new([added("a", 12), added("b", 20)], Mode::Split);
    let anchor = Position::new("a", Side::Modified, 5);
    assert_eq!(review.drag_target(&anchor, usize::MAX).unwrap().line(), 12);
    assert_eq!(review.drag_target(&anchor, 0).unwrap().line(), 1);
    assert_eq!(
        review.drag_target(&anchor, 3).unwrap().side(),
        Side::Modified
    );
}

#[test]
fn row_lookup_matches_the_file_order_after_repeated_growth_and_collapse() {
    let mut review = Review::new(
        (0..51).map(|i| added(&format!("{i}.rs"), i + 1)),
        Mode::Unified,
    );
    for step in 0..150 {
        let file = step * 17 % 51;
        let path = format!("{file}.rs");
        if step % 3 == 0 {
            review.collapse(&path, !review.is_collapsed(&path));
        } else {
            review.replace(added(&path, step % 30 + 1));
        }
        if step % 11 == 0 {
            review.set_mode(if step % 2 == 0 {
                Mode::Split
            } else {
                Mode::Unified
            });
        }
        let mut row = 0;
        for file in 0..review.file_count() {
            assert_eq!(review.file_start(file), row);
            for expected in review.rows(file).iter() {
                assert_eq!(review.row(row), Some((file, expected)));
                row += 1;
            }
        }
        assert_eq!(review.len(), row);
        assert!(review.row(row).is_none());
    }
}

#[test]
fn tinting_only_returns_ordered_valid_utf8_ranges() {
    for source in [
        "let 世界 = \"hello λ\"; // 👍",
        "'\\é'",
        "fn r#hello() -> u8 { 42 }",
        "🦀\tconst name = `é`;",
        "\"unterminated…",
    ] {
        let mut end = 0;
        for token in tokens(source) {
            assert!(token.bytes.start >= end);
            assert!(source.get(token.bytes.clone()).is_some());
            end = token.bytes.end;
        }
    }
}
