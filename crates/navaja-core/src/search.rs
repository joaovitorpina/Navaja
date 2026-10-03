//! Tool search for the sidebar filter and the command palette.
//!
//! Every query term must match the tool's name, id, keywords or category.
//! A term scores by how it matches (prefix > word start > substring >
//! subsequence) and where (name > keywords and id > category). Ties keep the
//! sidebar order: category position, category id, then name.

use serde::{Deserialize, Serialize};

use crate::id::ToolId;
use crate::meta::ToolMeta;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "ts", derive(ts_rs::TS), ts(export))]
pub struct SearchHit {
    pub id: ToolId,
    pub score: u32,
}

/// Ranks `tools` for `query`. An empty query returns every tool in sidebar order.
pub fn rank<'a>(query: &str, tools: impl IntoIterator<Item = &'a ToolMeta>) -> Vec<SearchHit> {
    let terms: Vec<String> = query.split_whitespace().map(str::to_lowercase).collect();
    let mut hits: Vec<(&ToolMeta, u32)> = tools
        .into_iter()
        .filter_map(|meta| score_tool(meta, &terms).map(|score| (meta, score)))
        .collect();
    hits.sort_by(|(a, sa), (b, sb)| {
        sb.cmp(sa)
            .then_with(|| a.category.info().order.cmp(&b.category.info().order))
            .then_with(|| a.category.cmp(&b.category))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
            .then_with(|| a.id.cmp(&b.id))
    });
    hits.into_iter()
        .map(|(meta, score)| SearchHit {
            id: meta.id.clone(),
            score,
        })
        .collect()
}

const NAME: u32 = 3;
const KEYWORD: u32 = 2;
const CATEGORY: u32 = 1;

fn score_tool(meta: &ToolMeta, terms: &[String]) -> Option<u32> {
    if terms.is_empty() {
        return Some(0);
    }
    let name = meta.name.to_lowercase();
    let category = meta.category.info().label.to_lowercase();
    let mut fields: Vec<(&str, u32)> = vec![(&name, NAME), (meta.id.as_str(), KEYWORD)];
    fields.extend(meta.keywords.iter().map(|k| (k.as_str(), KEYWORD)));
    fields.push((&category, CATEGORY));

    let mut total = 0;
    for term in terms {
        let best = fields
            .iter()
            .map(|(text, weight)| match match_level(text, term) {
                0 => 0,
                level => level * 10 + weight,
            })
            .max()
            .unwrap_or(0);
        if best == 0 {
            return None;
        }
        total += best;
    }
    Some(total)
}

/// 4 prefix, 3 word start, 2 substring, 1 subsequence, 0 no match.
fn match_level(text: &str, term: &str) -> u32 {
    if text.starts_with(term) {
        4
    } else if text
        .split(|c: char| !c.is_alphanumeric())
        .any(|word| word.starts_with(term))
    {
        3
    } else if text.contains(term) {
        2
    } else if is_subsequence(term, text) {
        1
    } else {
        0
    }
}

fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut rest = haystack.chars();
    needle.chars().all(|c| rest.any(|h| h == c))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::id::Category;
    use crate::meta::{GeneratorSpec, UiSpec};

    fn tool(id: &'static str, name: &str, category: Category, keywords: &[&str]) -> ToolMeta {
        ToolMeta {
            spec_version: crate::SPEC_VERSION,
            id: ToolId::from_static(id),
            name: name.into(),
            description: String::new(),
            category,
            keywords: keywords.iter().map(|k| (*k).to_owned()).collect(),
            icon: String::new(),
            capabilities: vec![],
            actions: vec![],
            tray: false,
            ui: UiSpec::Generator(GeneratorSpec {
                action: "x".into(),
                options: vec![],
                outputs: vec![],
                run_on_open: false,
            }),
        }
    }

    fn catalog() -> Vec<ToolMeta> {
        vec![
            tool(
                "json",
                "JSON formatter",
                Category::FORMATTERS,
                &["format", "minify", "validate"],
            ),
            tool(
                "uuid",
                "UUID generator",
                Category::GENERATORS,
                &["guid", "unique", "identifier"],
            ),
            tool(
                "base64",
                "Base64",
                Category::ENCODERS,
                &["encode", "decode"],
            ),
            tool(
                "ports",
                "Port inspector",
                Category::SYSTEM,
                &["port", "kill", "listen"],
            ),
        ]
    }

    fn ids(hits: &[SearchHit]) -> Vec<&str> {
        hits.iter().map(|h| h.id.as_str()).collect()
    }

    #[test]
    fn empty_query_returns_sidebar_order() {
        let tools = catalog();
        assert_eq!(
            ids(&rank("  ", &tools)),
            ["base64", "json", "uuid", "ports"]
        );
    }

    #[test]
    fn uid_ranks_uuid_first() {
        let tools = catalog();
        assert_eq!(ids(&rank("uid", &tools)).first(), Some(&"uuid"));
    }

    #[test]
    fn prefix_beats_substring_and_all_terms_must_match() {
        let tools = catalog();
        assert_eq!(ids(&rank("port", &tools)), ["ports"]);
        assert_eq!(ids(&rank("json min", &tools)), ["json"]);
        assert!(rank("json kill", &tools).is_empty());
    }

    #[test]
    fn case_insensitive_and_category_matches() {
        let tools = catalog();
        assert_eq!(ids(&rank("BASE", &tools)), ["base64"]);
        assert_eq!(ids(&rank("system", &tools)), ["ports"]);
    }

    #[test]
    fn relative_ranking() {
        let tools = catalog();
        // word start (ports) > substring in a keyword (json) > subsequence (uuid)
        assert_eq!(ids(&rank("in", &tools)), ["ports", "json", "uuid"]);
        // substring in a keyword (base64) > subsequence in the name (ports)
        assert_eq!(ids(&rank("co", &tools)), ["base64", "ports"]);
    }

    #[test]
    fn name_outweighs_keyword_and_ties_sort_by_name() {
        let tools = vec![
            tool("beta", "Beta", Category::ENCODERS, &["zeta"]),
            tool("zeta", "Zeta", Category::ENCODERS, &[]),
        ];
        assert_eq!(ids(&rank("zeta", &tools)), ["zeta", "beta"]);

        let tools = vec![
            tool("bravo", "Bravo", Category::ENCODERS, &[]),
            tool("alpha", "Alpha", Category::ENCODERS, &[]),
        ];
        assert_eq!(ids(&rank("enc", &tools)), ["alpha", "bravo"]);
    }

    #[test]
    fn unknown_categories_sort_after_known_ones_by_id() {
        let tools = vec![
            tool("b", "B", Category::from_static("zzz"), &[]),
            tool("a", "A", Category::from_static("aaa"), &[]),
            tool("c", "C", Category::SYSTEM, &[]),
        ];
        assert_eq!(ids(&rank("", &tools)), ["c", "a", "b"]);
    }

    #[test]
    fn subsequence_matches_last() {
        let tools = catalog();
        let hits = rank("jfr", &tools); // J-son F-o-R-matter
        assert_eq!(ids(&hits), ["json"]);
        assert_eq!(hits[0].score, 10 + NAME);
    }
}
