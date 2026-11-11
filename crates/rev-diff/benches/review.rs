//! CPU work only; excludes GPUI layout/paint, Git I/O and GPU frame time.
use rev_diff::{File, Mode, Review};
use std::{hint::black_box, time::Instant};

fn fixture(path: &str, lines: usize) -> File {
    let mut text = format!(
        "diff --git a/{path} b/{path}\n--- /dev/null\n+++ b/{path}\n@@ -0,0 +1,{lines} @@\n"
    );
    for i in 0..lines {
        text.push_str(&format!("+let value_{i} = {i};\n"));
    }
    File::parse(&text).unwrap().remove(0)
}

fn median(mut values: Vec<f64>) -> f64 {
    values.sort_by(f64::total_cmp);
    values[values.len() / 2]
}

fn main() {
    for (files, lines) in [(250, 400), (1000, 100), (20, 5000)] {
        let input: Vec<_> = (0..files)
            .map(|i| fixture(&format!("{i}.rs"), lines))
            .collect();
        let path = format!("{}.rs", files / 2);
        let replacement = fixture(&path, lines + 1);
        let (mut create, mut collapse, mut replace, mut lookup) = (vec![], vec![], vec![], vec![]);
        for run in 0..10 {
            let cloned = input.clone();
            let now = Instant::now();
            let mut review = Review::new(cloned, Mode::Unified);
            let created = now.elapsed().as_secs_f64() * 1000.;
            let now = Instant::now();
            for _ in 0..100 {
                review.collapse(&path, true);
                review.collapse(&path, false);
            }
            let collapsed = now.elapsed().as_secs_f64() * 1000. / 200.;
            let now = Instant::now();
            for _ in 0..100 {
                review.replace(replacement.clone());
            }
            let replaced = now.elapsed().as_secs_f64() * 1000. / 100.;
            let now = Instant::now();
            for row in 0..100_000 {
                black_box(review.row(row * 7919 % review.len()));
            }
            let looked_up = now.elapsed().as_secs_f64() * 1e9 / 100_000.;
            if run > 0 {
                create.push(created);
                collapse.push(collapsed);
                replace.push(replaced);
                lookup.push(looked_up);
            }
        }
        println!(
            "files={files} lines_per_file={lines} create_ms={:.3} collapse_ms={:.4} replace_ms={:.4} lookup_ns={:.1}",
            median(create),
            median(collapse),
            median(replace),
            median(lookup)
        );
    }
}
