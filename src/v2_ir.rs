//! V2 language-neutral control-flow input. The frontend supplies resolved identities;
//! unresolved behavior remains explicit, never silently omitted.
use crate::ir::Span;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Site {
    pub path: String,
    pub span: Span,
    /// Structural anchor within a function, independent of line number.
    pub anchor: String,
}
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct Place {
    pub root: String,
    pub projections: Vec<String>,
}
impl Place {
    pub fn local(name: impl Into<String>) -> Self {
        Self {
            root: name.into(),
            projections: vec![],
        }
    }
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Program {
    pub functions: Vec<Function>,
    pub roots: Vec<String>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Function {
    pub id: String,
    pub params: Vec<Place>,
    pub blocks: Vec<Block>,
    pub entry: usize,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Block {
    pub operations: Vec<Instruction>,
    pub terminator: Terminator,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Instruction {
    pub site: Site,
    pub kind: Kind,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Kind {
    /// Resolved model guarantees no tracked-resource mutation on success.
    LibraryEffect {
        model: String,
    },
    /// Bitset: 1 = Exception family, 2 = other BaseException, 4 = unknown.
    SetException {
        categories: u8,
    },
    /// Allocate a plain identity-bearing object with direct data fields.
    /// Frontends must rule out descriptors and custom attribute protocols.
    AllocateObject {
        target: Place,
    },
    Acquire {
        target: Place,
    },
    Assign {
        target: Place,
        source: Place,
    },
    Read {
        value: Place,
    },
    Write {
        value: Place,
    },
    /// Protocol-specific close: a transferred wrapper is detached and cannot close its former resource.
    CloseIfOwned {
        value: Place,
    },
    /// Buffer close cannot succeed while exported views exist.
    CloseIfUnborrowed {
        value: Place,
    },
    Close {
        value: Place,
    },
    /// Explicit modeled ownership policy: revoke source capability, mint recipient capability.
    TransferTo {
        source: Place,
        target: Place,
    },
    Transfer {
        value: Place,
    },
    Borrow {
        target: Place,
        source: Place,
        mutable: bool,
    },
    /// A protocol failure edge establishes an already-ended view, without calling release.
    AssumeBorrowEnded {
        value: Place,
    },
    EndBorrow {
        value: Place,
    },
    Call {
        target: Option<Place>,
        callee: String,
        args: Vec<Place>,
    },
    /// A resolved call with a distinct exceptional successor. Its return
    /// target is assigned only on normal completion; unwind preserves caller locals.
    Invoke {
        target: Option<Place>,
        callee: String,
        args: Vec<Place>,
        unwind: usize,
    },
    /// Explicit write through an unresolved attribute may replace module APIs.
    GlobalMutationUnknown {
        reason: String,
    },
    /// Affected objects may change. Empty means no reliable effect footprint,
    /// so every reachable tracked resource must conservatively be considered.
    Unknown {
        affected: Vec<Place>,
        reason: String,
    },
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Terminator {
    ExceptionMatch {
        categories: u8,
        matched: usize,
        unmatched: usize,
        site: Site,
    },
    Return {
        value: Option<Place>,
        site: Site,
    },
    Jump {
        target: usize,
    },
    Branch {
        then_target: usize,
        else_target: usize,
    },
    Raise {
        site: Site,
    },
    Stop,
}
