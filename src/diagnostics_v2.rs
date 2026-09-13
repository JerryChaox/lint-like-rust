//! Agent-facing proof obligations. An absent diagnostic is never proof of repair.
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Location {
    /// Workspace-relative normalized path. Locations may cross file boundaries.
    pub path: String,
    pub line: usize,
    pub column: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ScopeId {
    pub path: String,
    /// Qualified symbol, not its current line number.
    pub symbol: String,
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct ObligationKey {
    pub scope: ScopeId,
    pub rule: String,
    /// Semantic allocation/parameter/field anchor supplied by the frontend.
    pub object_anchor: String,
    /// Structural operation anchor with an explicit sibling disambiguator.
    pub operation_anchor: String,
}
impl ObligationKey {
    /// Lossless deterministic identity, deliberately not a platform-dependent hash.
    /// Stability covers location-only changes; arbitrary refactors need explicit matching.
    pub fn issue_id(&self) -> String {
        let parts = [
            &self.scope.path,
            &self.scope.symbol,
            &self.rule,
            &self.object_anchor,
            &self.operation_anchor,
        ];
        let mut id = String::from("llr:v2:");
        for part in parts {
            id.push_str(&format!("{}:{}", part.len(), part));
        }
        id
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleClass {
    LanguageSafety,
    SafetyPolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProofStatus {
    Verified,
    Violated,
    Unverified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceStep {
    pub location: Location,
    pub operation: String,
    pub explanation: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Certainty {
    Definite,
    Possible,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Evidence {
    pub certainty: Certainty,
    pub class: RuleClass,
    pub message: String,
    pub object_identity: String,
    pub primary: Location,
    pub trace: Vec<TraceStep>,
    pub violated_constraint: String,
    /// Suggestions express conditions; these are not certified automatic edits.
    pub repair_constraints: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Obligation {
    pub key: ObligationKey,
    pub status: ProofStatus,
    /// Conditions under which this local result holds, including external contracts.
    pub assumptions: Vec<String>,
    pub evidence: Option<Evidence>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Gap {
    /// None means project-wide uncertainty (e.g. incomplete module discovery).
    pub scope: Option<ScopeId>,
    pub location: Option<Location>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AnalysisContext {
    /// Change whenever rule meaning or the abstract semantics changes.
    pub semantics_revision: String,
    /// Canonical effective config, contracts, enabled rules, and analysis bounds digest.
    pub configuration_fingerprint: String,
    /// Language version, dependency/stub model revision and frontend version digest.
    pub environment_fingerprint: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Report {
    pub schema_version: u32,
    pub context: AnalysisContext,
    /// Explicit inventory: no obligation means no claim, even with zero gaps.
    pub obligations: Vec<Obligation>,
    pub gaps: Vec<Gap>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    Resolved,
    StillPresent,
    New,
    BecameUnverified,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IssueChange {
    pub issue_id: String,
    pub kind: ChangeKind,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Comparison {
    pub comparable: bool,
    pub issues: Vec<IssueChange>,
    pub warnings: Vec<String>,
}

fn duplicate_keys(report: &Report) -> bool {
    let mut seen = BTreeSet::new();
    report.obligations.iter().any(|o| !seen.insert(&o.key))
}
fn affected_by_gap(report: &Report, scope: &ScopeId) -> bool {
    report
        .gaps
        .iter()
        .any(|g| g.scope.as_ref().is_none_or(|s| s == scope))
}
fn assumptions_equal(a: &Obligation, b: &Obligation) -> bool {
    a.assumptions.iter().collect::<BTreeSet<_>>() == b.assumptions.iter().collect::<BTreeSet<_>>()
}

/// Conservative comparison. Missing keys, changed assumptions/config, ambiguous keys,
/// or incomplete proof scopes cannot establish that a former violation was repaired.
/// This intentionally does not infer that deleting a function fixes its callers.
pub fn compare_reports(before: &Report, after: &Report) -> Comparison {
    let mut warnings = Vec::new();
    if before.schema_version != SCHEMA_VERSION || after.schema_version != SCHEMA_VERSION {
        warnings.push("Unsupported report schema; proof results are not comparable".into());
    }
    if before.context != after.context {
        warnings.push("Analysis semantics, configuration, or environment changed".into());
    }
    if duplicate_keys(before) || duplicate_keys(after) {
        warnings.push("Duplicate obligation identities; structural anchors are ambiguous".into());
    }
    let comparable = warnings.is_empty();
    let previous: BTreeMap<_, _> = before.obligations.iter().map(|o| (&o.key, o)).collect();
    let current: BTreeMap<_, _> = after.obligations.iter().map(|o| (&o.key, o)).collect();
    let mut issues = Vec::new();
    for old in before
        .obligations
        .iter()
        .filter(|o| o.status == ProofStatus::Violated)
    {
        let (kind, reason) = match current.get(&old.key) {
            Some(new) if new.status == ProofStatus::Violated => {
                (ChangeKind::StillPresent, "Matching violation remains")
            }
            _ if !comparable => (ChangeKind::BecameUnverified, "Reports are not comparable"),
            Some(new) if !assumptions_equal(old, new) => {
                (ChangeKind::BecameUnverified, "Proof assumptions changed")
            }
            Some(new)
                if new.status == ProofStatus::Verified
                    && !affected_by_gap(after, &old.key.scope) =>
            {
                (
                    ChangeKind::Resolved,
                    "Matching obligation explicitly verified in a complete proof scope",
                )
            }
            Some(_) => (
                ChangeKind::BecameUnverified,
                "Matching obligation or its proof scope is unverified",
            ),
            None => (
                ChangeKind::BecameUnverified,
                "Obligation disappeared; absence is not proof of repair",
            ),
        };
        issues.push(IssueChange {
            issue_id: old.key.issue_id(),
            kind,
            reason: reason.into(),
        });
    }
    for new in after
        .obligations
        .iter()
        .filter(|o| o.status == ProofStatus::Violated)
    {
        if !previous
            .get(&new.key)
            .is_some_and(|o| o.status == ProofStatus::Violated)
        {
            issues.push(IssueChange {
                issue_id: new.key.issue_id(),
                kind: ChangeKind::New,
                reason: "Violation newly reported; not necessarily newly introduced".into(),
            });
        }
    }
    issues.sort_by(|a, b| a.issue_id.cmp(&b.issue_id));
    Comparison {
        comparable,
        issues,
        warnings,
    }
}
