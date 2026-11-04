//! Repeatable CPU benchmarks for layout work, separate from paint/frame timings.
use std::time::Instant;

use gpui::{AppContext, TestAppContext};

use super::{DiffFile, DiffState};

fn median(samples: &mut [f64]) -> f64 {
    samples.sort_by(f64::total_cmp);
    samples[samples.len() / 2]
}

#[gpui::test]
#[ignore = "run explicitly with --ignored --nocapture; reports layout CPU time"]
fn benchmark_layout_updates(cx: &mut TestAppContext) {
    for (file_count, line_count) in [(250, 400), (1000, 100), (20, 5000)] {
        let text = (0..line_count)
            .map(|line| format!("source line {line}\n"))
            .collect::<String>();
        let files = (0..file_count)
            .map(|file| DiffFile::unchanged(format!("file-{file}.txt"), &text))
            .collect::<Vec<_>>();
        let path = format!("file-{}.txt", file_count / 2);
        let replacement = DiffFile::unchanged(path.clone(), &(text.clone() + "one extra line\n"));
        let mut create = Vec::new();
        let mut collapse = Vec::new();
        let mut replace = Vec::new();
        // First iteration warms allocation/code paths. Timed work excludes parsing.
        for iteration in 0..10 {
            let start = Instant::now();
            let state = cx.new(|cx| DiffState::new(files.clone(), cx));
            let create_ms = start.elapsed().as_secs_f64() * 1000.;
            let (collapse_ms, replace_ms) = state.update(cx, |state, cx| {
                let start = Instant::now();
                for _ in 0..10 {
                    state.set_file_collapsed(&path, true, cx);
                    state.set_file_collapsed(&path, false, cx);
                }
                let collapse_ms = start.elapsed().as_secs_f64() * 1000. / 20.;
                let start = Instant::now();
                for _ in 0..10 {
                    assert!(state.replace_file(replacement.clone(), cx));
                }
                let replace_ms = start.elapsed().as_secs_f64() * 1000. / 10.;
                (collapse_ms, replace_ms)
            });
            if iteration > 0 {
                create.push(create_ms);
                collapse.push(collapse_ms);
                replace.push(replace_ms);
            }
        }
        println!(
            "layout files={file_count} lines_per_file={line_count} create_ms={:.3} collapse_ms={:.3} replace_ms={:.3}",
            median(&mut create),
            median(&mut collapse),
            median(&mut replace)
        );
    }
}
