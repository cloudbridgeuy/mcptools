pub mod parse;
pub mod types;

pub use parse::{parse_rg_commands, parse_rg_output};
pub use types::{MergedSnippet, RankedSnippet, Snippet};
