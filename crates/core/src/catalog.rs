#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct CatalogEntry {
    pub name: String,
    pub domain: String,
    pub summary: String,
}

pub const SUMMARY_MAX_CHARS: usize = 80;

pub fn build_catalog<'a>(tools: impl IntoIterator<Item = (&'a str, &'a str)>) -> Vec<CatalogEntry> {
    tools
        .into_iter()
        .map(|(name, summary)| {
            let domain = name
                .split_once('_')
                .map_or(name.to_string(), |(prefix, _)| prefix.to_string());
            CatalogEntry {
                name: name.to_string(),
                domain,
                summary: summary.to_string(),
            }
        })
        .collect()
}

pub fn render_catalog(entries: &[CatalogEntry]) -> String {
    entries
        .iter()
        .map(|entry| format!("{}\t{}\t{}\n", entry.domain, entry.name, entry.summary))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn build_catalog_derives_domain_from_prefix() {
        let result = build_catalog([("ui_annotations_list", "List")]);
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "ui_annotations_list");
        assert_eq!(result[0].domain, "ui");
        assert_eq!(result[0].summary, "List");
    }

    #[test]
    fn build_catalog_keeps_name_without_underscore_as_domain() {
        let result = build_catalog([("ping", "Ping")]);
        assert_eq!(result[0].domain, "ping");
    }

    #[test]
    fn build_catalog_keeps_input_order() {
        let result = build_catalog([("a_b", "A"), ("c_d", "C")]);
        assert_eq!(
            result.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(),
            vec!["a_b", "c_d"]
        );
    }

    #[test]
    fn render_catalog_emits_one_tab_separated_line_per_entry() {
        let entries = vec![
            CatalogEntry {
                name: "jira_get".to_string(),
                domain: "jira".to_string(),
                summary: "Get".to_string(),
            },
            CatalogEntry {
                name: "hn_read_item".to_string(),
                domain: "hn".to_string(),
                summary: "Read".to_string(),
            },
        ];
        let output = render_catalog(&entries);
        assert_eq!(output, "jira\tjira_get\tGet\nhn\thn_read_item\tRead\n");
        assert_eq!(render_catalog(&[]), "");
    }
}
