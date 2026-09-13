//! Converts solver facts to the agent protocol without inventing path witnesses.
use crate::{diagnostics_v2 as d, solver_v2 as s};

const ASSUMPTION: &str = "Local resource-validity proof under the supplied frontend models and analysis context; joined evidence is not a single feasible execution trace";
fn location(site: &crate::v2_ir::Site) -> d::Location {
    d::Location {
        path: site.path.clone(),
        line: site.span.line,
        column: site.span.column,
    }
}
fn rule(operation: &str) -> &str {
    match operation {
        "read" | "write" | "borrow_mut" | "borrow_shared" => "LIFE001",
        "acquire" => "RESOURCE_ACQUIRE",
        "close" => "RESOURCE_CLOSE",
        "call" => "CALL_EFFECT",
        "library_effect" => "LIBRARY_EFFECT",
        "ownership" => "OWNERSHIP_UNSUPPORTED",
        op if op.starts_with("loan_") => "BORROW001",
        op if op.starts_with("permission_") => "OWN001",
        _ => "ANALYSIS_INCOMPLETE",
    }
}

/// Preserves every solver obligation. Unknown obligations invalidate their entire
/// function scope, conservatively including other call contexts of that function.
/// Context fingerprints must include frontend model and solver bound revisions.
pub fn build_report(analysis: &s::Analysis, context: d::AnalysisContext) -> d::Report {
    let mut report = d::Report {
        schema_version: d::SCHEMA_VERSION,
        context,
        obligations: vec![],
        gaps: vec![],
    };
    for fact in &analysis.obligations {
        let scope = d::ScopeId {
            path: fact.site.path.clone(),
            symbol: fact.function.clone(),
        };
        let finding = analysis
            .findings
            .iter()
            .find(|f| f.obligation_id == fact.id);
        let status = match fact.status {
            s::ObligationStatus::Verified => d::ProofStatus::Verified,
            s::ObligationStatus::Violated => d::ProofStatus::Violated,
            s::ObligationStatus::Unverified => d::ProofStatus::Unverified,
        };
        if status == d::ProofStatus::Unverified {
            report.gaps.push(d::Gap {
                scope: Some(scope.clone()),
                location: Some(location(&fact.site)),
                reason: fact.reason.clone(),
            });
        }
        let evidence = finding.map(|f| d::Evidence {
            certainty: match f.certainty { s::Certainty::Definite => d::Certainty::Definite, s::Certainty::Possible => d::Certainty::Possible },
            class: if matches!(f.rule.as_str(), "OWN001" | "BORROW001") { d::RuleClass::SafetyPolicy } else { d::RuleClass::LanguageSafety },
            message: f.message.clone(),
            object_identity: if fact.object_anchor.is_empty() { "unresolved object identity".into() } else { fact.object_anchor.clone() },
            primary: location(&f.site),
            trace: f.trace.iter().map(|t| d::TraceStep { location: location(&t.site), operation: "joined_evidence".into(), explanation: format!("Joined analysis evidence (not an ordered feasible path): {}", t.message) }).collect(),
            violated_constraint: if f.rule=="BORROW001" { "A view must be live; shared loans prohibit writes, exclusive loans prohibit competing access, and owner close/transfer requires released views" } else if f.rule=="OWN001" { "Access must use the current owner capability; ordinary Python aliases preserve the old capability" } else { "Reading or writing this resource requires it to be open on every relevant execution path" }.into(),
            repair_constraints: vec![if f.rule=="BORROW001" { "Release conflicting views before owner access; access a view before its release; preserve readonly restrictions and independently derived views. Copying an alias does not create a new loan or end one" } else if f.rule=="OWN001" { "Use the transfer recipient, move the access before transfer, or use the capability returned by a modeled detach; copying the old alias does not restore ownership" } else { "Perform the access before resource closure, or obtain a valid resource for the later access; preserve required cleanup" }.into()],
        });
        report.obligations.push(d::Obligation {
            key: d::ObligationKey {
                scope,
                rule: finding.map_or_else(|| rule(&fact.operation).into(), |f| f.rule.clone()),
                object_anchor: if fact.object_anchor.is_empty() {
                    "no-object-proof".into()
                } else {
                    fact.object_anchor.clone()
                },
                // Solver IDs contain the root, call context, structural site and
                // operation. They exclude line/column and distinguish call sites.
                operation_anchor: fact.id.clone(),
            },
            status,
            assumptions: vec![
                ASSUMPTION.into(),
                "Verification covers modeled normal and exceptional CFG paths; unresolved effects remain explicit gaps".into(),
                "Resource validity and explicitly modeled ownership/borrowing policy only: no resource-leak freedom or global Rust-equivalent safety guarantee".into(),
                "Modeled builtins and library APIs have not been monkeypatched".into(),
                "Project class bindings start from modeled module definitions; unmodeled prior entry invocations are excluded".into(),
            ],
            evidence,
        });
    }
    for finding in &analysis.findings {
        if !analysis
            .obligations
            .iter()
            .any(|o| o.id == finding.obligation_id)
        {
            report.gaps.push(d::Gap {
                scope: None,
                location: Some(location(&finding.site)),
                reason: format!(
                    "Solver finding {} has no matching proof obligation",
                    finding.obligation_id
                ),
            });
        }
    }
    report
}
