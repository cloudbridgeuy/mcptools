pub use crate::error::Error;

pub use anstream::eprintln;
pub use anstream::println;
pub use color_eyre::eyre::{eyre, Context, OptionExt, Result};
pub use std::format as f;
pub fn new_table() -> prettytable::Table {
    let mut table = prettytable::Table::new();

    let format = prettytable::format::FormatBuilder::new()
        .padding(1, 1)
        .build();

    table.set_format(format);

    table
}
