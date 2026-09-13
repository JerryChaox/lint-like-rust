//! Language-independent structured control-flow IR. No Python/parser types belong here.
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Span {
    pub line: usize,
    pub column: usize,
    pub end_line: usize,
    pub end_column: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Unit {
    pub name: String,
    pub path: String,
    pub body: Vec<Op>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Op {
    pub span: Span,
    pub kind: OpKind,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OpKind {
    New {
        target: String,
        resource: bool,
    },
    Alias {
        target: String,
        source: String,
    },
    Use {
        value: String,
    },
    /// Read object identity/metadata that remains valid after resource closure.
    Inspect {
        value: String,
    },
    Write {
        value: String,
        structural: bool,
    },
    Move {
        value: String,
    },
    Close {
        value: String,
    },
    Borrow {
        target: String,
        source: String,
        mutable: bool,
    },
    EndBorrow {
        target: String,
    },
    Escape {
        value: String,
    },
    MustUse {
        callee: String,
    },
    Unknown {
        reason: String,
    },
    Branch {
        then_body: Vec<Op>,
        else_body: Vec<Op>,
    },
    Loop {
        body: Vec<Op>,
        else_body: Vec<Op>,
    },
    Try {
        body: Vec<Op>,
        handlers: Vec<Vec<Op>>,
        else_body: Vec<Op>,
        finally_body: Vec<Op>,
    },
    Scope {
        body: Vec<Op>,
        cleanup: Vec<Op>,
    },
    Return {
        value: Option<String>,
    },
    Raise,
    Break,
    Continue,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    Definite,
    Possible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Note {
    pub message: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Diagnostic {
    pub path: String,
    pub rule: String,
    pub message: String,
    pub span: Span,
    pub confidence: Confidence,
    pub notes: Vec<Note>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Analysis {
    pub diagnostics: Vec<Diagnostic>,
    pub coverage: Vec<CoverageGap>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoverageGap {
    pub path: String,
    pub span: Span,
    pub reason: String,
}
