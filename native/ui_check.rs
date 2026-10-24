extern crate gpui_kit as gpui;
mod app;
mod backend;
mod model;
fn main() -> anyhow::Result<()> {
    app::smoke::run()
}
