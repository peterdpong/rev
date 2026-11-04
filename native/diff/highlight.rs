//! Theme-independent captures using the public grammar registry.
use std::ops::Range;

use gpui::SharedString;
use gpui_kit::component::highlighter::LanguageRegistry;
use tree_sitter_highlight::{HighlightConfiguration, HighlightEvent, Highlighter};

pub(crate) struct CaptureHighlighter {
    config: Option<HighlightConfiguration>,
    names: Vec<SharedString>,
    highlighter: Highlighter,
}

impl CaptureHighlighter {
    pub fn new(language: &str) -> Self {
        let config = LanguageRegistry::singleton()
            .language(language)
            .and_then(|grammar| {
                // Patch hunks are independent fragments. Match the original diff
                // behavior by omitting injected languages and parsing each hunk afresh.
                HighlightConfiguration::new(
                    grammar.language?,
                    language,
                    &grammar.highlights,
                    "",
                    &grammar.locals,
                )
                .ok()
            });
        let mut names = Vec::new();
        let config = config.map(|mut config| {
            names = config
                .query
                .capture_names()
                .iter()
                .map(|name| SharedString::from((*name).to_owned()))
                .collect();
            config.configure(&names);
            config
        });
        Self {
            config,
            names,
            highlighter: Highlighter::new(),
        }
    }

    pub fn capture_names(&mut self, text: &str) -> Vec<(Range<usize>, SharedString)> {
        let Some(config) = &self.config else {
            return Vec::new();
        };
        let Ok(events) = self
            .highlighter
            .highlight(config, text.as_bytes(), None, |_| None)
        else {
            return Vec::new();
        };
        let mut stack = Vec::new();
        let mut captures = Vec::new();
        for event in events {
            match event {
                Ok(HighlightEvent::HighlightStart(highlight)) => stack.push(highlight.0),
                Ok(HighlightEvent::HighlightEnd) => {
                    stack.pop();
                }
                Ok(HighlightEvent::Source { start, end }) => {
                    if let Some(name) = stack.last().and_then(|ix| self.names.get(*ix)) {
                        captures.push((start..end, name.clone()));
                    }
                }
                Err(_) => break,
            }
        }
        captures
    }
}
