use base64::Engine;

use crate::prelude::{println, *};

#[derive(Debug, clap::Parser)]
#[command(name = "pdf")]
#[command(about = "PDF document navigation and extraction")]
pub struct App {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, clap::Subcommand)]
pub enum Commands {
    /// Print document tree / table of contents
    Toc {
        /// Path to the PDF file
        path: std::path::PathBuf,
    },
    /// Read a section's content as Markdown
    Read {
        /// Path to the PDF file
        path: std::path::PathBuf,
        /// Section ID (e.g., "s-1-0")
        section_id: String,
    },
    /// Extract an image from the document
    Image {
        /// Path to the PDF file
        path: std::path::PathBuf,
        /// Image ID (XObject name)
        image_id: String,
        /// Output file path (if omitted, prints base64 to stdout)
        #[arg(short, long)]
        output: Option<std::path::PathBuf>,
    },
    /// Print document metadata
    Info {
        /// Path to the PDF file
        path: std::path::PathBuf,
    },
}

pub async fn run(app: App, _global: crate::Global) -> Result<()> {
    match app.command {
        Commands::Toc { path } => {
            let bytes = std::fs::read(&path)?;
            let tree = pdf::parse(&bytes).map_err(|e| eyre!(e))?;
            println!("{}", serde_json::to_string_pretty(&tree)?);
            Ok(())
        }
        Commands::Read { path, section_id } => {
            let bytes = std::fs::read(&path)?;
            let id = pdf::SectionId::parse(&section_id).map_err(|e| eyre!(e))?;
            let content = pdf::read_section(&bytes, &id).map_err(|e| eyre!(e))?;
            println!("{}", serde_json::to_string_pretty(&content)?);
            Ok(())
        }
        Commands::Image {
            path,
            image_id,
            output,
        } => {
            let bytes = std::fs::read(&path)?;
            let id = pdf::ImageId::new(image_id);
            let img = pdf::get_image(&bytes, &id).map_err(|e| eyre!(e))?;
            if let Some(out) = output {
                std::fs::write(out, &img.bytes)?;
            } else {
                println!(
                    "{}",
                    base64::engine::general_purpose::STANDARD.encode(&img.bytes)
                );
            }
            Ok(())
        }
        Commands::Info { path } => {
            let bytes = std::fs::read(&path)?;
            let meta = pdf::info(&bytes).map_err(|e| eyre!(e))?;
            println!("{}", serde_json::to_string_pretty(&meta)?);
            Ok(())
        }
    }
}
