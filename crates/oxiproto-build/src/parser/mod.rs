#![forbid(unsafe_code)]
//! Native `.proto` file lexer, outline parser, and full body parser.
//!
//! This module provides Phase 2 Slice 1 (lexer + file outline) and Slice P1
//! (full recursive-descent body parser) of the native `.proto` parsing
//! pipeline.  The lexer tokenizes proto source text; the outline parser
//! identifies top-level structures; the full parser produces a complete AST.

pub mod ast;
pub mod error;
pub mod lexer;
pub mod outline;
pub mod parse;
pub mod span;
pub mod token;

#[cfg(feature = "native-parser")]
pub mod comments;
#[cfg(feature = "native-parser")]
pub mod descriptor;
#[cfg(feature = "native-parser")]
pub mod loader;
#[cfg(feature = "native-parser")]
pub mod resolve;

pub use ast::{
    Edition, Enum, EnumValue, ExtendBlock, ExtensionRange, Field, FieldLabel, FieldType, Import,
    ImportModifier, Message, Method, Oneof, OptionValue, ProtoFile, ProtoOption, Reserved,
    ReservedRange, ReservedRangeTo, ScalarType, Service,
};
pub use error::{LexError, ParseError};
pub use lexer::Lexer;
pub use outline::{parse_outline, FileOutline, TopLevelItem};
pub use parse::parse_file;
pub use span::{Span, Spanned};
pub use token::Token;

#[cfg(feature = "native-parser")]
pub use descriptor::build_file_descriptor_set;
#[cfg(feature = "native-parser")]
pub use resolve::resolve;
