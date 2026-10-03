use std::collections::HashSet;

use navaja_core::{ToolError, Value, run_single};
use serde_json::json;

use super::TOOL;

fn run(input: Value) -> Result<Vec<String>, ToolError> {
    let out = run_single(TOOL, "generate", input)?;
    let text = out["uuids"].as_str().expect("uuids is text");
    Ok(text.lines().map(str::to_owned).collect())
}

fn parse(text: &str) -> ::uuid::Uuid {
    ::uuid::Uuid::parse_str(text).unwrap_or_else(|e| panic!("{text:?}: {e}"))
}

#[test]
fn defaults_to_one_lowercase_hyphenated_v4() {
    let ids = run(json!({})).unwrap();
    assert_eq!(ids.len(), 1);
    let id = &ids[0];
    assert_eq!(id.len(), 36);
    assert_eq!(*id, id.to_lowercase());
    assert_eq!(parse(id).get_version_num(), 4);
}

#[test]
fn ten_thousand_v7_are_unique_and_sorted() {
    let ids = run(json!({ "version": "v7", "count": 10_000 })).unwrap();
    assert_eq!(ids.len(), 10_000);
    assert!(ids.iter().all(|id| parse(id).get_version_num() == 7));
    let unique: HashSet<&String> = ids.iter().collect();
    assert_eq!(unique.len(), ids.len());
    assert!(ids.windows(2).all(|w| w[0] < w[1]), "v7 batch must be strictly increasing");
}

#[test]
fn v7_stays_ordered_across_runs() {
    let mut previous = String::new();
    for _ in 0..500 {
        let id = run(json!({ "version": "v7" })).unwrap().remove(0);
        assert!(id > previous, "{id} <= {previous}");
        previous = id;
    }
}

#[test]
fn formats_and_case() {
    let one = |format: &str, uppercase: bool| {
        run(json!({ "format": format, "uppercase": uppercase })).unwrap().remove(0)
    };
    assert_eq!(one("simple", false).len(), 32);
    assert!(!one("simple", false).contains('-'));
    let braced = one("braced", false);
    assert!(braced.starts_with('{') && braced.ends_with('}'));
    let urn = one("urn", true);
    assert!(urn.starts_with("urn:uuid:"), "{urn}");
    assert_eq!(urn["urn:uuid:".len()..], urn["urn:uuid:".len()..].to_uppercase());
    let upper = one("hyphenated", true);
    assert_eq!(upper, upper.to_uppercase());
}

#[test]
fn count_is_bounded() {
    for count in [0, 10_001] {
        let error = run(json!({ "count": count })).unwrap_err();
        assert_eq!(error.code.as_str(), "uuid.count_out_of_range");
    }
}

#[test]
fn rejects_unknown_options() {
    let error = run(json!({ "colour": "blue" })).unwrap_err();
    assert_eq!(error.code.as_str(), "core.invalid_input");
}
