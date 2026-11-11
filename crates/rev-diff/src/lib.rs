//! Review-specific diff data, independent of a UI toolkit or diff algorithm.
//!
//! Git computes changes. This crate reads its unified patches and indexes the
//! source they actually contain. Screen rows are disposable projections; source
//! positions remain stable across folding, mode changes and per-file loading.

mod document;
mod index;
mod lexer;
mod parser;

pub use document::{File, Hunk, Line, LineKind, Position, Selection, Side, Status};
pub use index::{Anchor, Mode, Review, Row, RowEdit};
pub use lexer::{Token, TokenKind, tokens};
pub use parser::ParseError;
