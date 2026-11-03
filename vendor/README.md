# GPUI Component adapters

`gpui-component` 0.8.0 is vendored from crates.io (upstream commit `1676826d9d49b90f4d7b6808b69924f58796f6f1`, `crates/component`). The Apache-2.0 license is included.

In `src/diff/state.rs`, two read-only accessors expose the native text selection as source lines and report whether the pointer gesture is still active. This avoids reconstructing source coordinates from hover state, text matching, or screen geometry. Original text highlighting, clipboard contents, wrapping, virtualization, and selection scrolling remain owned by the component.

The `replace_file` adapter lets the viewer replace a large-diff placeholder after loading it on demand. It retains other files, their collapsed state, and the source location at the top of the viewport. Diff component tests cover those guarantees. Remove these adapters when upstream APIs supply the same behavior.
