# Rev diff component

Adapted from the diff module in `gpui-component` 0.8.0, published from
upstream commit `1676826d9d49b90f4d7b6808b69924f58796f6f1` in
https://github.com/longbridge/gpui-kit (`crates/component/src/diff`).
The upstream Apache-2.0 license is included in LICENSE-APACHE.

Rev owns this module and its tests. Shared controls, themes, text selection,
and syntax grammars use the published GPUI Kit APIs. A small Tree-sitter
adapter replaces the upstream private capture API without copying the highlighter. The extracted
language-name helper and diff-only translations retain their upstream behavior.
Diff actions use the RevDiff context to avoid conflicting with the library's
own diff bindings. Cargo uses the registry dependency without a path override.

Run `cargo test --locked --bin rev diff::` to check parser, source coordinates,
selection, copying, folds, highlighting, and scrolling behavior.
