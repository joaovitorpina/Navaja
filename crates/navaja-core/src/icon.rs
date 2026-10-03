//! Tool-icon validation (assets/brand/GUIDELINES.md, "Tool icons").
//!
//! Icons are rendered through a CSS mask, never as markup, but they are still
//! checked against a strict allowlist so a later extension can't smuggle in
//! scripts, external references or styles.

use std::fmt;

const MAX_BYTES: usize = 8 * 1024;
const MAX_NODES: u32 = 128;
const VIEW_BOX: &str = "0 0 24 24";

const ELEMENTS: &[&str] = &[
    "svg", "g", "path", "circle", "ellipse", "rect", "line", "polyline", "polygon",
];

const ATTRIBUTES: &[&str] = &[
    "viewBox",
    "width",
    "height",
    "fill",
    "fill-rule",
    "fill-opacity",
    "stroke",
    "stroke-width",
    "stroke-linecap",
    "stroke-linejoin",
    "stroke-miterlimit",
    "stroke-opacity",
    "clip-rule",
    "opacity",
    "transform",
    "d",
    "cx",
    "cy",
    "r",
    "rx",
    "ry",
    "x",
    "y",
    "x1",
    "y1",
    "x2",
    "y2",
    "points",
];

/// Colours are themed by the shell, so paint is limited to these values.
const PAINT: &[&str] = &["none", "currentColor"];

const SVG_NS: &str = "http://www.w3.org/2000/svg";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IconError {
    TooLarge(usize),
    Parse(String),
    NotSvg,
    ViewBox,
    Element(String),
    Attribute { element: String, attribute: String },
    Paint { attribute: String, value: String },
    Text,
    ProcessingInstruction,
}

impl fmt::Display for IconError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooLarge(n) => write!(f, "icon is {n} bytes; the limit is {MAX_BYTES}"),
            Self::Parse(e) => write!(f, "icon is not well-formed XML: {e}"),
            Self::NotSvg => write!(f, "icon root must be an SVG <svg> element"),
            Self::ViewBox => write!(f, "icon must use viewBox=\"{VIEW_BOX}\""),
            Self::Element(name) => write!(f, "element <{name}> is not allowed in icons"),
            Self::Attribute { element, attribute } => {
                write!(
                    f,
                    "attribute {attribute} on <{element}> is not allowed in icons"
                )
            }
            Self::Paint { attribute, value } => write!(
                f,
                "{attribute}=\"{value}\" is not allowed; use \"none\" or \"currentColor\""
            ),
            Self::Text => write!(f, "icons must not contain text"),
            Self::ProcessingInstruction => {
                write!(f, "icons must not contain processing instructions")
            }
        }
    }
}

impl std::error::Error for IconError {}

pub fn validate_icon(svg: &str) -> Result<(), IconError> {
    if svg.len() > MAX_BYTES {
        return Err(IconError::TooLarge(svg.len()));
    }
    // DTDs stay rejected (the default); the node limit also bounds nesting
    // depth, so a small but deeply nested file can't exhaust the stack.
    let options = roxmltree::ParsingOptions {
        nodes_limit: MAX_NODES,
        ..roxmltree::ParsingOptions::default()
    };
    let doc = roxmltree::Document::parse_with_options(svg, options)
        .map_err(|e| IconError::Parse(e.to_string()))?;
    // Anywhere, including the prolog: <?xml-stylesheet href=…?> is an external reference.
    if doc.root().descendants().any(|n| n.is_pi()) {
        return Err(IconError::ProcessingInstruction);
    }
    let root = doc.root_element();
    if root.tag_name().name() != "svg" || root.tag_name().namespace() != Some(SVG_NS) {
        return Err(IconError::NotSvg);
    }
    if root.attribute("viewBox") != Some(VIEW_BOX) {
        return Err(IconError::ViewBox);
    }

    for node in root.descendants() {
        if node.is_text() {
            if node.text().is_some_and(|t| !t.trim().is_empty()) {
                return Err(IconError::Text);
            }
            continue;
        }
        if !node.is_element() {
            continue; // comments; processing instructions were rejected above
        }
        let element = node.tag_name();
        if element.namespace() != Some(SVG_NS) || !ELEMENTS.contains(&element.name()) {
            return Err(IconError::Element(element.name().to_owned()));
        }
        for attribute in node.attributes() {
            let name = attribute.name();
            if attribute.namespace().is_some() || !ATTRIBUTES.contains(&name) {
                return Err(IconError::Attribute {
                    element: element.name().to_owned(),
                    attribute: name.to_owned(),
                });
            }
            if (name == "fill" || name == "stroke") && !PAINT.contains(&attribute.value()) {
                return Err(IconError::Paint {
                    attribute: name.to_owned(),
                    value: attribute.value().to_owned(),
                });
            }
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const GOOD: &str = r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="1.5"><!-- ok --><rect x="3" y="4" width="18" height="16" rx="2"/><path d="M9 8l-1 8"/></svg>"#;

    #[test]
    fn accepts_allowlisted_icon() {
        assert_eq!(validate_icon(GOOD), Ok(()));
        // An XML declaration is not a processing instruction.
        let declared = format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>{GOOD}");
        assert_eq!(validate_icon(&declared), Ok(()));
    }

    #[test]
    fn rejects_dangerous_or_off_spec_icons() {
        let ns = r#"xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24""#;
        let cases = [
            (format!("<svg {ns}><script>alert(1)</script></svg>"), "script"),
            (format!(r#"<svg {ns}><path onload="x()" d="M0 0"/></svg>"#), "onload"),
            (format!(r##"<svg {ns} xmlns:xlink="http://www.w3.org/1999/xlink"><use xlink:href="#a"/></svg>"##), "use"),
            (format!(r#"<svg {ns}><path style="fill:red" d="M0 0"/></svg>"#), "style"),
            (format!(r#"<svg {ns}><path fill="red" d="M0 0"/></svg>"#), "paint"),
            (format!("<svg {ns}><foreignObject/></svg>"), "foreignObject"),
            (format!("<svg {ns}>hi</svg>"), "text"),
            (r#"<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 16 16"/>"#.to_owned(), "viewBox"),
            ("<svg viewBox=\"0 0 24 24\"/>".to_owned(), "no namespace"),
            ("<html/>".to_owned(), "not svg"),
            (r#"<!DOCTYPE svg [<!ENTITY x "y">]><svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24"/>"#.to_owned(), "dtd"),
            (format!("<svg {ns}>{}</svg>", "<g/>".repeat(3000)), "size"),
            (format!("<svg {ns}><?xml-stylesheet href=\"https://x\"?></svg>"), "inner PI"),
            (format!("<?xml-stylesheet href=\"https://x\"?><svg {ns}/>"), "prolog PI"),
            (format!(r#"<svg {ns}><path stroke="red" d="M0 0"/></svg>"#), "stroke paint"),
            (format!("<svg {ns}>{}{}</svg>", "<g>".repeat(1000), "</g>".repeat(1000)), "deep nesting"),
        ];
        for (svg, why) in cases {
            assert!(validate_icon(&svg).is_err(), "accepted icon with {why}");
        }
    }
}
