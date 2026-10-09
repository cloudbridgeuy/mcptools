use crate::atlas::query::{QueryKind, DEFAULT_CONTEXT_LINES, DEFAULT_LIMIT};
use crate::prelude::*;

#[derive(Debug, clap::Parser)]
pub struct QueryOptions {
    #[arg(
        help = "Case-insensitive literal text to find in indexed names, paths, signatures and cached descriptions"
    )]
    query: String,
    #[arg(long, value_enum, default_value = "all")]
    kind: QueryKind,
    #[arg(long, default_value_t = DEFAULT_LIMIT, help = "Maximum results (1-1000)")]
    limit: usize,
    #[arg(
        long,
        help = "Read bounded excerpts from current source, not from the index"
    )]
    excerpts: bool,
    #[arg(long, default_value_t = DEFAULT_CONTEXT_LINES, help = "Maximum excerpt lines per hit (0-200); requires --excerpts")]
    context_lines: usize,
    #[arg(long, default_value_t = crate::atlas::query::DEFAULT_CONTEXT_CHARS, help = "Character budget for returned descriptions/signatures and each excerpt (0-32768)")]
    context_chars: usize,
    #[arg(long)]
    json: bool,
}

pub async fn run(opts: QueryOptions, _global: crate::Global) -> Result<()> {
    crate::atlas::query::validate_query(&opts.query, opts.limit, opts.context_lines)?;
    crate::atlas::query::validate_context(opts.context_chars)?;
    let (root, config, db) = crate::atlas::data::open_navigation()?;
    let freshness = crate::atlas::data::query_freshness(&db, &config, &root)?;
    let result = crate::atlas::query::query_data(
        &db,
        &root,
        crate::atlas::query::QueryRequest {
            query: &opts.query,
            kind: opts.kind,
            limit: opts.limit,
            context_lines: if opts.excerpts { opts.context_lines } else { 0 },
            context_chars: opts.context_chars,
        },
        freshness,
    )?;
    if opts.json {
        crate::prelude::println!("{}", serde_json::to_string(&result)?);
    } else {
        crate::prelude::eprintln!("{}", result.freshness);
        for hit in result.hits {
            let location = hit
                .symbol
                .as_ref()
                .map(|s| {
                    format!(
                        ":{}-{} {} ({}) {}",
                        s.start_line,
                        s.end_line,
                        s.name,
                        s.kind,
                        s.signature.as_deref().unwrap_or("")
                    )
                })
                .unwrap_or_default();
            crate::prelude::println!(
                "{}{} [score={}, {}]",
                hit.path.display(),
                location,
                hit.score,
                hit.matched_on
            );
            if let Some(excerpt) = hit.excerpt {
                crate::prelude::println!("{}", excerpt.error.unwrap_or(excerpt.text));
            }
        }
        crate::prelude::eprintln!("bounds={}", serde_json::to_string(&result.bounds)?);
    }
    Ok(())
}
