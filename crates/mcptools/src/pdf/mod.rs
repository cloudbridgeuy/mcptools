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
        /// Section ID (e.g., "s-1-0"). Omit for whole document.
        section_id: Option<String>,
    },
    /// Peek into a section's content (sample a text snippet)
    Peek {
        /// Path to the PDF file
        path: std::path::PathBuf,
        /// Section ID. Omit for whole document.
        section_id: Option<String>,
        /// Where to sample from (beginning, middle, ending, random)
        #[arg(short, long, default_value = "beginning")]
        position: String,
        /// Maximum characters to return
        #[arg(short, long, default_value_t = 500)]
        limit: usize,
    },
    /// List all images in a section or the whole document
    Images {
        /// Path to the PDF file
        path: std::path::PathBuf,
        /// Section ID. Omit for all document images.
        section_id: Option<String>,
    },
    /// Extract an image from the document
    Image {
        /// Path to the PDF file
        path: std::path::PathBuf,
        /// Image ID (XObject name). Required unless --random is specified.
        image_id: Option<String>,
        /// Output file path (if omitted, prints base64 to stdout)
        #[arg(short, long)]
        output: Option<std::path::PathBuf>,
        /// Scope image selection to a specific section
        #[arg(short, long)]
        section: Option<String>,
        /// Pick a random image (cannot be used with image_id)
        #[arg(short, long)]
        random: bool,
    },
    /// Print document metadata
    Info {
        /// Path to the PDF file
        path: std::path::PathBuf,
    },
}

/// Parse an optional section ID string into Option<SectionId>.
fn parse_section_id(s: Option<&str>) -> Result<Option<pdf::SectionId>> {
    s.map(|id| pdf::SectionId::parse(id).map_err(|e| eyre!(e)))
        .transpose()
}

/// Write image data to file or stdout as base64.
fn output_image(img: &pdf::ImageData, output: Option<&std::path::Path>) -> Result<()> {
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
            let id = parse_section_id(section_id.as_deref())?;
            let content = pdf::read_section(&bytes, id.as_ref()).map_err(|e| eyre!(e))?;
            println!("{}", serde_json::to_string_pretty(&content)?);
            Ok(())
        }
        Commands::Peek {
            path,
            section_id,
            position,
            limit,
        } => {
            let bytes = std::fs::read(&path)?;
            let pos: pdf::PeekPosition = position
                .parse()
                .map_err(|e: pdf::InvalidPeekPosition| eyre!(e))?;
            let id = parse_section_id(section_id.as_deref())?;
            let peek = pdf::peek_section(&bytes, id.as_ref(), pos, limit).map_err(|e| eyre!(e))?;
            println!("{}", serde_json::to_string_pretty(&peek)?);
            Ok(())
        }
        Commands::Images { path, section_id } => {
            let bytes = std::fs::read(&path)?;
            let id = parse_section_id(section_id.as_deref())?;
            let images = pdf::list_section_images(&bytes, id.as_ref()).map_err(|e| eyre!(e))?;
            println!("{}", serde_json::to_string_pretty(&images)?);
            Ok(())
        }
        Commands::Image {
            path,
            image_id,
            output,
            section,
            random,
        } => {
            if image_id.is_some() && random {
                return Err(eyre!("Cannot specify both an image ID and --random"));
            }
            if image_id.is_none() && !random {
                return Err(eyre!("Either provide an image ID or use --random"));
            }

            let bytes = std::fs::read(&path)?;

            if let Some(id_str) = image_id {
                let id = pdf::ImageId::new(id_str);
                let img = pdf::get_image(&bytes, &id).map_err(|e| eyre!(e))?;
                output_image(&img, output.as_deref())?;
            } else {
                let section_id = parse_section_id(section.as_deref())?;
                let images =
                    pdf::list_section_images(&bytes, section_id.as_ref()).map_err(|e| eyre!(e))?;
                if images.is_empty() {
                    return Err(eyre!("No images found"));
                }
                use rand::Rng;
                let idx = rand::thread_rng().gen_range(0..images.len());
                let img = pdf::get_image(&bytes, &images[idx].id).map_err(|e| eyre!(e))?;
                output_image(&img, output.as_deref())?;
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
