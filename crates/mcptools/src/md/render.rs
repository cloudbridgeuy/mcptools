use crate::prelude::{println, *};
use mcptools_core::md::{
    render_filename, render_markdown_html, RenderArgs, RenderAssets, RenderOutput,
};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

const MERMAID_JS: &str = include_str!("../../assets/mermaid-11.12.0/mermaid.min.js");
const CSS: &str = include_str!("../../assets/markdown.css");

#[derive(Debug, clap::Args)]
pub struct RenderOptions {
    pub input: String,
    #[arg(short = 'o', long = "output")]
    pub output: Option<String>,
    #[arg(long)]
    pub title: Option<String>,
    #[arg(long)]
    pub open: bool,
    #[arg(long)]
    pub json: bool,
}

pub fn write(args: RenderArgs) -> Result<RenderOutput> {
    let title = args.title.as_deref().unwrap_or("Markdown");
    let html = render_markdown_html(
        &args.markdown,
        title,
        RenderAssets {
            css: CSS,
            mermaid_js: MERMAID_JS,
        },
    );
    let path = match args.path {
        Some(path) => PathBuf::from(path),
        None => crate::plan_store::data_dir()?
            .join("renders")
            .join(render_filename(title, &html)),
    };
    let path = std::path::absolute(path)?;
    let parent = path
        .parent()
        .ok_or_else(|| eyre!("Output has no parent directory"))?;
    std::fs::create_dir_all(parent)?;
    let parent = parent.canonicalize()?;
    let path = parent.join(
        path.file_name()
            .ok_or_else(|| eyre!("Output needs a file name"))?,
    );
    let url = reqwest::Url::from_file_path(&path)
        .map_err(|()| eyre!("Cannot create file URL"))?
        .to_string();
    let output_path = path
        .to_str()
        .ok_or_else(|| eyre!("Output path must be UTF-8"))?
        .to_owned();
    let mut temporary = tempfile::NamedTempFile::new_in(&parent)?;
    temporary.write_all(html.as_bytes())?;
    temporary.flush()?;
    temporary.as_file().sync_all()?;
    temporary.persist(&path)?;
    Ok(RenderOutput {
        path: output_path,
        url,
    })
}

pub fn run(options: RenderOptions) -> Result<()> {
    let markdown = if options.input == "-" {
        let mut markdown = String::new();
        std::io::stdin().read_to_string(&mut markdown)?;
        markdown
    } else {
        std::fs::read_to_string(Path::new(&options.input))?
    };
    let output = write(RenderArgs {
        markdown,
        path: options.output,
        title: options.title,
    })?;
    if options.json {
        println!("{}", serde_json::to_string(&output)?);
    } else {
        println!("{}", output.path);
    }
    crate::open::maybe_open(&[output.url], options.open);
    Ok(())
}
