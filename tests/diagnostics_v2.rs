use lint_like_rust::diagnostics_v2::*;

fn obligation(status: ProofStatus) -> Obligation {
    Obligation {
        key: ObligationKey {
            scope: ScopeId {
                path: "app.py".into(),
                symbol: "process".into(),
            },
            rule: "LIFE001".into(),
            object_anchor: "parameter:stream".into(),
            operation_anchor: "call:stream.read:0".into(),
        },
        status,
        assumptions: vec!["stdlib-file-model-v1".into()],
        evidence: None,
    }
}
fn report(status: ProofStatus) -> Report {
    Report {
        schema_version: SCHEMA_VERSION,
        context: AnalysisContext {
            semantics_revision: "1".into(),
            configuration_fingerprint: "default".into(),
            environment_fingerprint: "python3.11".into(),
        },
        obligations: vec![obligation(status)],
        gaps: vec![],
    }
}
fn change(after: Report) -> ChangeKind {
    compare_reports(&report(ProofStatus::Violated), &after).issues[0]
        .kind
        .clone()
}

#[test]
fn explicit_verified_obligation_resolves() {
    assert_eq!(change(report(ProofStatus::Verified)), ChangeKind::Resolved);
}
#[test]
fn disappearance_into_unknown_is_never_resolved() {
    let mut after = report(ProofStatus::Verified);
    after.obligations.clear();
    after.gaps.push(Gap {
        scope: None,
        location: None,
        reason: "dynamic call".into(),
    });
    assert_eq!(change(after), ChangeKind::BecameUnverified);
}
#[test]
fn silent_disappearance_is_never_resolved() {
    let mut after = report(ProofStatus::Verified);
    after.obligations.clear();
    assert_eq!(change(after), ChangeKind::BecameUnverified);
}
#[test]
fn scope_gap_prevents_false_resolution() {
    let mut after = report(ProofStatus::Verified);
    after.gaps.push(Gap {
        scope: Some(after.obligations[0].key.scope.clone()),
        location: None,
        reason: "analysis limit".into(),
    });
    assert_eq!(change(after), ChangeKind::BecameUnverified);
}
#[test]
fn unrelated_scope_gap_does_not_invalidate_local_proof() {
    let mut after = report(ProofStatus::Verified);
    after.gaps.push(Gap {
        scope: Some(ScopeId {
            path: "elsewhere.py".into(),
            symbol: "g".into(),
        }),
        location: None,
        reason: "unrelated dynamic call".into(),
    });
    assert_eq!(change(after), ChangeKind::Resolved);
}
#[test]
fn config_changes_cannot_claim_repair() {
    let mut after = report(ProofStatus::Verified);
    after.context.configuration_fingerprint = "disabled-check".into();
    let comparison = compare_reports(&report(ProofStatus::Violated), &after);
    assert!(!comparison.comparable);
    assert_eq!(comparison.issues[0].kind, ChangeKind::BecameUnverified);
}
#[test]
fn changed_assumptions_cannot_claim_repair() {
    let mut after = report(ProofStatus::Verified);
    after.obligations[0]
        .assumptions
        .push("trust arbitrary calls".into());
    assert_eq!(change(after), ChangeKind::BecameUnverified);
}
#[test]
fn unknown_status_cannot_claim_repair() {
    assert_eq!(
        change(report(ProofStatus::Unverified)),
        ChangeKind::BecameUnverified
    );
}
#[test]
fn remaining_violation_is_still_present() {
    assert_eq!(
        change(report(ProofStatus::Violated)),
        ChangeKind::StillPresent
    );
}
#[test]
fn new_violation_is_newly_reported() {
    let result = compare_reports(
        &report(ProofStatus::Verified),
        &report(ProofStatus::Violated),
    );
    assert_eq!(result.issues[0].kind, ChangeKind::New);
}
#[test]
fn duplicate_identity_blocks_resolution() {
    let mut after = report(ProofStatus::Verified);
    after.obligations.push(after.obligations[0].clone());
    assert_eq!(change(after), ChangeKind::BecameUnverified);
}
#[test]
fn identity_encoding_does_not_collide_on_delimiters() {
    let a = obligation(ProofStatus::Violated).key;
    let mut b = a.clone();
    b.scope.path = "app.py:process".into();
    b.scope.symbol = "".into();
    assert_ne!(a.issue_id(), b.issue_id());
}
#[test]
fn cross_file_trace_roundtrips_and_line_movement_preserves_identity() {
    let mut before = report(ProofStatus::Violated);
    before.obligations[0].evidence = Some(Evidence {
        certainty: Certainty::Definite,
        class: RuleClass::LanguageSafety,
        message: "closed resource".into(),
        object_identity: "parameter:stream".into(),
        primary: Location {
            path: "app.py".into(),
            line: 20,
            column: 5,
        },
        trace: vec![TraceStep {
            location: Location {
                path: "helpers.py".into(),
                line: 8,
                column: 3,
            },
            operation: "close".into(),
            explanation: "closes argument 0".into(),
        }],
        violated_constraint: "read requires open".into(),
        repair_constraints: vec!["read before close".into()],
    });
    let encoded = serde_json::to_string(&before).unwrap();
    assert_eq!(serde_json::from_str::<Report>(&encoded).unwrap(), before);
    let mut after = before.clone();
    after.obligations[0].evidence.as_mut().unwrap().primary.line = 80;
    assert_eq!(
        before.obligations[0].key.issue_id(),
        after.obligations[0].key.issue_id()
    );
    assert_eq!(
        compare_reports(&before, &after).issues[0].kind,
        ChangeKind::StillPresent
    );
}
