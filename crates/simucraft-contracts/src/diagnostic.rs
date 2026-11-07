//! Diagnostics: the one shape every component uses to report problems.
//!
//! Wire form (JSON): `{code, severity, message, file?, path?, id?, span?, fix?}`.

use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
    Info,
}

/// 1-based position in a text file.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub line: u32,
    pub col: u32,
}

/// A suggested repair.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Fix {
    pub message: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Diagnostic {
    /// Stable code: `F…` files, `C…` compiler, `R…` runtime, `A…` API.
    pub code: String,
    pub severity: Severity,
    pub message: String,
    /// File the problem is in, relative to the project root.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<String>,
    /// Key path in a file (`blocks.b_x.inputs.u1`) or model path (`guidance/pid.y`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Stable ID of the item concerned.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub span: Option<Span>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fix: Option<Fix>,
}

impl Diagnostic {
    pub fn new(code: &str, severity: Severity, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            severity,
            message: message.into(),
            file: None,
            path: None,
            id: None,
            span: None,
            fix: None,
        }
    }
    pub fn error(code: &str, message: impl Into<String>) -> Self {
        Self::new(code, Severity::Error, message)
    }
    pub fn warning(code: &str, message: impl Into<String>) -> Self {
        Self::new(code, Severity::Warning, message)
    }
    pub fn at(mut self, path: impl Into<String>) -> Self {
        self.path = Some(path.into());
        self
    }
    pub fn id(mut self, id: impl Into<String>) -> Self {
        self.id = Some(id.into());
        self
    }
    pub fn span(mut self, span: Option<Span>) -> Self {
        self.span = span;
        self
    }
    pub fn fix(mut self, message: impl Into<String>) -> Self {
        self.fix = Some(Fix {
            message: message.into(),
        });
        self
    }
    pub fn is_error(&self) -> bool {
        self.severity == Severity::Error
    }
}
