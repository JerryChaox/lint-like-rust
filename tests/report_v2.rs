use lint_like_rust::{
    diagnostics_v2 as d, ir::Span, report_v2::build_report, solver_v2 as s, v2_ir::*,
};
fn context() -> d::AnalysisContext {
    d::AnalysisContext {
        semantics_revision: "v2-test".into(),
        configuration_fingerprint: "same".into(),
        environment_fingerprint: "same".into(),
    }
}
fn instruction(anchor: &str, line: usize, kind: Kind) -> Instruction {
    Instruction {
        site: Site {
            path: "app.py".into(),
            span: Span {
                line,
                column: 1,
                ..Default::default()
            },
            anchor: anchor.into(),
        },
        kind,
    }
}
fn program(close: bool, unknown: bool) -> Program {
    let mut operations = vec![instruction(
        "acquire:f:0",
        1,
        Kind::Acquire {
            target: Place::local("f"),
        },
    )];
    if close {
        operations.push(instruction(
            "close:f:0",
            2,
            Kind::Close {
                value: Place::local("f"),
            },
        ));
    }
    if unknown {
        operations.push(instruction(
            "unknown:0",
            3,
            Kind::Unknown {
                affected: vec![],
                reason: "unresolved call".into(),
            },
        ));
    }
    operations.push(instruction(
        "read:f:0",
        if close { 4 } else { 2 },
        Kind::Read {
            value: Place::local("f"),
        },
    ));
    Program {
        functions: vec![Function {
            id: "app.main".into(),
            params: vec![],
            entry: 0,
            blocks: vec![Block {
                operations,
                terminator: Terminator::Stop,
            }],
        }],
        roots: vec!["app.main".into()],
    }
}
#[test]
fn closing_then_removing_close_resolves_matching_read_obligation() {
    let before = build_report(&s::analyze(&program(true, false)), context());
    let after = build_report(&s::analyze(&program(false, false)), context());
    let diff = d::compare_reports(&before, &after);
    assert_eq!(diff.issues.len(), 1);
    assert_eq!(diff.issues[0].kind, d::ChangeKind::Resolved);
}
#[test]
fn unknown_call_cannot_look_like_repair() {
    let before = build_report(&s::analyze(&program(true, false)), context());
    let after = build_report(&s::analyze(&program(true, true)), context());
    assert!(!after.gaps.is_empty());
    assert_eq!(
        d::compare_reports(&before, &after).issues[0].kind,
        d::ChangeKind::BecameUnverified
    );
}
#[test]
fn joined_evidence_is_labeled_and_objects_are_preserved() {
    let report = build_report(&s::analyze(&program(true, false)), context());
    let finding = report
        .obligations
        .iter()
        .find_map(|o| o.evidence.as_ref())
        .unwrap();
    assert!(finding.object_identity.contains("acquire:f:0"));
    assert!(
        finding
            .trace
            .iter()
            .all(|t| t.operation == "joined_evidence"
                && t.explanation.contains("not an ordered feasible path"))
    );
}
#[test]
fn unsupported_ownership_produces_scope_gap() {
    let mut input = program(false, false);
    input.functions[0].blocks[0].operations.push(instruction(
        "transfer:f:0",
        5,
        Kind::Transfer {
            value: Place::local("f"),
        },
    ));
    let report = build_report(&s::analyze(&input), context());
    assert!(
        report.obligations.iter().any(
            |o| o.key.rule == "OWNERSHIP_UNSUPPORTED" && o.status == d::ProofStatus::Unverified
        )
    );
    assert!(!report.gaps.is_empty());
}
#[test]
fn call_context_keeps_distinct_obligation_ids() {
    let helper = Function {
        id: "helper.consume".into(),
        params: vec![Place::local("arg")],
        entry: 0,
        blocks: vec![Block {
            operations: vec![instruction(
                "read:arg:0",
                8,
                Kind::Read {
                    value: Place::local("arg"),
                },
            )],
            terminator: Terminator::Stop,
        }],
    };
    let mut input = program(true, false);
    input.functions[0].blocks[0].operations.pop();
    for n in 0..2 {
        input.functions[0].blocks[0].operations.push(instruction(
            &format!("call:consume:{n}"),
            n + 9,
            Kind::Call {
                target: None,
                callee: "helper.consume".into(),
                args: vec![Place::local("f")],
            },
        ));
    }
    input.functions.push(helper);
    let report = build_report(&s::analyze(&input), context());
    let violations: Vec<_> = report
        .obligations
        .iter()
        .filter(|o| o.status == d::ProofStatus::Violated)
        .collect();
    assert_eq!(violations.len(), 2);
    assert_ne!(violations[0].key.issue_id(), violations[1].key.issue_id());
}
