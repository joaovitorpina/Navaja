//! SPDX licence expressions, as npm packages declare them in `license`.
//!
//! The grammar is SPDX 2.3, annex D: `WITH` binds tighter than `AND`, which
//! binds tighter than `OR`, and parentheses group. Operators must be upper
//! case; licence and exception ids compare case-insensitively.

use std::fmt;

/// One licence, possibly "or later" (`GPL-2.0+`) and with an exception
/// (`Apache-2.0 WITH LLVM-exception`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct License {
    pub id: String,
    pub or_later: bool,
    pub exception: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    License(License),
    And(Box<Expr>, Box<Expr>),
    Or(Box<Expr>, Box<Expr>),
}

impl fmt::Display for License {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.id)?;
        if self.or_later {
            f.write_str("+")?;
        }
        if let Some(exception) = &self.exception {
            write!(f, " WITH {exception}")?;
        }
        Ok(())
    }
}

impl Expr {
    /// Parses a whole expression; anything left over is an error.
    pub fn parse(text: &str) -> Result<Self, String> {
        let tokens = tokenize(text);
        if tokens.is_empty() {
            return Err("empty expression".to_owned());
        }
        let mut parser = Parser { tokens, next: 0 };
        let expr = parser.or_expr()?;
        match parser.peek() {
            None => Ok(expr),
            Some(token) => Err(format!("unexpected {token} after a complete expression")),
        }
    }

    /// Whether the licensee can comply using allowed licences only: one side
    /// of an `OR` is enough, both sides of an `AND` are needed.
    pub fn satisfied_by(&self, allowed: &dyn Fn(&License) -> bool) -> bool {
        match self {
            Self::License(license) => allowed(license),
            Self::And(a, b) => a.satisfied_by(allowed) && b.satisfied_by(allowed),
            Self::Or(a, b) => a.satisfied_by(allowed) || b.satisfied_by(allowed),
        }
    }

    /// Every licence the expression names, left to right.
    pub fn licenses(&self) -> Vec<&License> {
        let mut out = Vec::new();
        let mut stack = vec![self];
        while let Some(expr) = stack.pop() {
            match expr {
                Self::License(license) => out.push(license),
                Self::And(a, b) | Self::Or(a, b) => {
                    stack.push(b);
                    stack.push(a);
                }
            }
        }
        out
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Token<'a> {
    Open,
    Close,
    And,
    Or,
    With,
    Word(&'a str),
}

impl fmt::Display for Token<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Open => f.write_str("`(`"),
            Self::Close => f.write_str("`)`"),
            Self::And => f.write_str("`AND`"),
            Self::Or => f.write_str("`OR`"),
            Self::With => f.write_str("`WITH`"),
            Self::Word(word) => write!(f, "`{word}`"),
        }
    }
}

fn tokenize(text: &str) -> Vec<Token<'_>> {
    let mut tokens = Vec::new();
    let mut rest = text.trim_start();
    while let Some(c) = rest.chars().next() {
        let len = match c {
            '(' => {
                tokens.push(Token::Open);
                1
            }
            ')' => {
                tokens.push(Token::Close);
                1
            }
            _ => {
                let len = rest
                    .find(|c: char| c.is_whitespace() || c == '(' || c == ')')
                    .unwrap_or(rest.len());
                tokens.push(match &rest[..len] {
                    "AND" => Token::And,
                    "OR" => Token::Or,
                    "WITH" => Token::With,
                    word => Token::Word(word),
                });
                len
            }
        };
        rest = rest[len..].trim_start();
    }
    tokens
}

struct Parser<'a> {
    tokens: Vec<Token<'a>>,
    next: usize,
}

impl<'a> Parser<'a> {
    fn peek(&self) -> Option<Token<'a>> {
        self.tokens.get(self.next).copied()
    }

    fn bump(&mut self) -> Option<Token<'a>> {
        let token = self.peek();
        self.next += 1;
        token
    }

    fn or_expr(&mut self) -> Result<Expr, String> {
        let mut left = self.and_expr()?;
        while self.peek() == Some(Token::Or) {
            self.bump();
            left = Expr::Or(Box::new(left), Box::new(self.and_expr()?));
        }
        Ok(left)
    }

    fn and_expr(&mut self) -> Result<Expr, String> {
        let mut left = self.primary()?;
        while self.peek() == Some(Token::And) {
            self.bump();
            left = Expr::And(Box::new(left), Box::new(self.primary()?));
        }
        Ok(left)
    }

    fn primary(&mut self) -> Result<Expr, String> {
        match self.bump() {
            Some(Token::Open) => {
                let inner = self.or_expr()?;
                match self.bump() {
                    Some(Token::Close) => Ok(inner),
                    Some(token) => Err(format!("expected `)`, found {token}")),
                    None => Err("unclosed `(`".to_owned()),
                }
            }
            Some(Token::Word(word)) => {
                let (id, or_later) = match word.strip_suffix('+') {
                    Some(base) => (base, true),
                    None => (word, false),
                };
                if !is_license_id(id) {
                    return Err(format!("`{word}` is not a licence id"));
                }
                Ok(Expr::License(License {
                    id: id.to_owned(),
                    or_later,
                    exception: self.exception()?,
                }))
            }
            Some(token) => Err(format!("expected a licence or `(`, found {token}")),
            None => Err("the expression ends where a licence should be".to_owned()),
        }
    }

    /// An optional `WITH <exception-id>` after a licence.
    fn exception(&mut self) -> Result<Option<String>, String> {
        if self.peek() != Some(Token::With) {
            return Ok(None);
        }
        self.bump();
        match self.bump() {
            Some(Token::Word(id)) if is_idstring(id) => Ok(Some(id.to_owned())),
            Some(token) => Err(format!(
                "expected an exception id after `WITH`, found {token}"
            )),
            None => Err("expected an exception id after `WITH`".to_owned()),
        }
    }
}

/// SPDX `idstring`: letters, digits, `-` and `.`.
fn is_idstring(text: &str) -> bool {
    !text.is_empty()
        && text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '.')
}

/// A listed id such as `MIT`, or a custom `LicenseRef-…`, optionally from
/// another document (`DocumentRef-…:LicenseRef-…`).
fn is_license_id(text: &str) -> bool {
    match text.split_once(':') {
        Some((document, license)) => {
            document.starts_with("DocumentRef-")
                && is_idstring(document)
                && license.starts_with("LicenseRef-")
                && is_idstring(license)
        }
        None => is_idstring(text),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn lic(id: &str) -> Expr {
        Expr::License(License {
            id: id.to_owned(),
            or_later: false,
            exception: None,
        })
    }

    fn and(a: Expr, b: Expr) -> Expr {
        Expr::And(Box::new(a), Box::new(b))
    }

    fn or(a: Expr, b: Expr) -> Expr {
        Expr::Or(Box::new(a), Box::new(b))
    }

    /// Allows MIT, ISC and `Apache-2.0 WITH LLVM-exception`, like an
    /// allowlist would (case-insensitive ids, exact exception).
    fn allowed(license: &License) -> bool {
        let id = license.id.to_ascii_lowercase();
        let exception = license.exception.as_deref().map(str::to_ascii_lowercase);
        matches!(
            (id.as_str(), exception.as_deref()),
            ("mit" | "isc", None) | ("apache-2.0", Some("llvm-exception"))
        )
    }

    fn ok(text: &str) -> bool {
        match Expr::parse(text) {
            Ok(expr) => expr.satisfied_by(&allowed),
            Err(error) => panic!("{text}: {error}"),
        }
    }

    #[test]
    fn parses_single_licences() {
        assert_eq!(Expr::parse("MIT"), Ok(lic("MIT")));
        assert_eq!(Expr::parse("  MIT  "), Ok(lic("MIT")));
        assert_eq!(
            Expr::parse("GPL-2.0+"),
            Ok(Expr::License(License {
                id: "GPL-2.0".to_owned(),
                or_later: true,
                exception: None,
            }))
        );
        assert_eq!(
            Expr::parse("Apache-2.0 WITH LLVM-exception"),
            Ok(Expr::License(License {
                id: "Apache-2.0".to_owned(),
                or_later: false,
                exception: Some("LLVM-exception".to_owned()),
            }))
        );
        assert_eq!(
            Expr::parse("LicenseRef-Custom"),
            Ok(lic("LicenseRef-Custom"))
        );
        assert_eq!(
            Expr::parse("DocumentRef-spdx-tool-1.2:LicenseRef-MIT-Style-2"),
            Ok(lic("DocumentRef-spdx-tool-1.2:LicenseRef-MIT-Style-2"))
        );
    }

    #[test]
    fn and_binds_tighter_than_or() {
        assert_eq!(
            Expr::parse("MIT OR ISC AND Zlib"),
            Ok(or(lic("MIT"), and(lic("ISC"), lic("Zlib"))))
        );
        assert_eq!(
            Expr::parse("MIT AND ISC OR Zlib"),
            Ok(or(and(lic("MIT"), lic("ISC")), lic("Zlib")))
        );
        assert_eq!(
            Expr::parse("(MIT OR ISC) AND Zlib"),
            Ok(and(or(lic("MIT"), lic("ISC")), lic("Zlib")))
        );
        assert_eq!(
            Expr::parse("((MIT))AND(ISC)"),
            Ok(and(lic("MIT"), lic("ISC")))
        );
    }

    #[test]
    fn with_binds_to_one_licence() {
        let expr = Expr::parse("MIT OR Apache-2.0 WITH LLVM-exception AND ISC");
        let llvm = Expr::License(License {
            id: "Apache-2.0".to_owned(),
            or_later: false,
            exception: Some("LLVM-exception".to_owned()),
        });
        assert_eq!(expr, Ok(or(lic("MIT"), and(llvm, lic("ISC")))));
    }

    #[test]
    fn rejects_malformed_expressions() {
        for bad in [
            "",
            "   ",
            "MIT OR",
            "OR MIT",
            "MIT AND AND ISC",
            "(MIT",
            "MIT)",
            "()",
            "MIT ISC",
            // Operators are upper case in SPDX.
            "MIT or ISC",
            "SEE LICENSE IN LICENSE.md",
            "MIT/X11",
            "MIT WITH",
            "MIT WITH (Foo)",
            "MIT WITH Foo+",
            "WITH Foo",
            "(MIT) WITH Foo",
            "Custom:LicenseRef-x",
            "DocumentRef-x:Custom",
            "+",
        ] {
            assert!(Expr::parse(bad).is_err(), "{bad:?} should not parse");
        }
    }

    #[test]
    fn or_needs_one_side_and_needs_both() {
        assert!(ok("MIT"));
        assert!(ok("mit"));
        assert!(ok("MIT OR GPL-3.0-or-later"));
        assert!(ok("GPL-3.0-or-later OR MIT"));
        assert!(ok("MIT AND ISC"));
        assert!(ok("(MIT AND ISC) OR GPL-3.0-only"));
        assert!(ok("(MIT OR GPL-3.0-only) AND (ISC OR GPL-3.0-only)"));

        assert!(!ok("GPL-3.0-only"));
        assert!(!ok("MIT AND GPL-3.0-only"));
        assert!(!ok("GPL-3.0-only AND MIT"));
        assert!(!ok("(MIT OR ISC) AND GPL-3.0-only"));
        assert!(!ok("LicenseRef-Custom"));
    }

    #[test]
    fn exceptions_must_match_exactly() {
        assert!(ok("Apache-2.0 WITH LLVM-exception"));
        assert!(ok("apache-2.0 WITH llvm-exception"));
        // The allowlist names Apache-2.0 only with LLVM-exception.
        assert!(!ok("Apache-2.0"));
        assert!(!ok("Apache-2.0 WITH Classpath-exception-2.0"));
        assert!(!ok("MIT WITH Classpath-exception-2.0"));
    }

    #[test]
    fn lists_every_licence() {
        let expr = Expr::parse("(MIT OR GPL-2.0+) AND Apache-2.0 WITH LLVM-exception");
        let names: Vec<String> = expr
            .as_ref()
            .map(|e| e.licenses().iter().map(ToString::to_string).collect())
            .unwrap_or_default();
        assert_eq!(names, ["MIT", "GPL-2.0+", "Apache-2.0 WITH LLVM-exception"]);
    }
}
