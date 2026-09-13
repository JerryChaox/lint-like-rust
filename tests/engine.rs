use lint_like_rust::{engine::analyze, ir::*};
fn op(line: usize, kind: OpKind) -> Op {
    Op {
        span: Span {
            line,
            column: 1,
            end_line: line,
            end_column: 2,
        },
        kind,
    }
}
fn new(n: &str, resource: bool) -> OpKind {
    OpKind::New {
        target: n.into(),
        resource,
    }
}
fn use_(n: &str) -> OpKind {
    OpKind::Use { value: n.into() }
}
fn move_(n: &str) -> OpKind {
    OpKind::Move { value: n.into() }
}
fn close(n: &str) -> OpKind {
    OpKind::Close { value: n.into() }
}
fn alias(t: &str, s: &str) -> OpKind {
    OpKind::Alias {
        target: t.into(),
        source: s.into(),
    }
}
fn borrow(t: &str, s: &str, mutable: bool) -> OpKind {
    OpKind::Borrow {
        target: t.into(),
        source: s.into(),
        mutable,
    }
}
fn write(n: &str) -> OpKind {
    OpKind::Write {
        value: n.into(),
        structural: true,
    }
}
fn run(ops: Vec<OpKind>) -> Analysis {
    run_ops(
        ops.into_iter()
            .enumerate()
            .map(|(i, k)| op(i + 1, k))
            .collect(),
    )
}
fn run_ops(body: Vec<Op>) -> Analysis {
    analyze(&Unit {
        name: "test".into(),
        path: "test.py".into(),
        body,
    })
}
fn has(a: &Analysis, rule: &str) -> bool {
    a.diagnostics.iter().any(|d| d.rule == rule)
}
#[test]
fn move_invalidates_aliases() {
    let a = run(vec![
        new("x", false),
        alias("a", "x"),
        move_("x"),
        use_("a"),
        move_("a"),
    ]);
    assert!(has(&a, "OWN001"));
    assert!(has(&a, "OWN002"));
}
#[test]
fn reassignment_restores_binding_not_old_alias() {
    let a = run(vec![
        new("x", false),
        alias("a", "x"),
        move_("x"),
        new("x", false),
        use_("x"),
        use_("a"),
    ]);
    assert_eq!(a.diagnostics.len(), 1);
    assert_eq!(a.diagnostics[0].span.line, 6);
}
#[test]
fn regular_aliases_are_not_borrows() {
    let a = run(vec![
        new("x", false),
        alias("a", "x"),
        write("x"),
        use_("a"),
    ]);
    assert!(a.diagnostics.is_empty());
    assert!(a.coverage.is_empty());
}
#[test]
fn close_invalidates_aliases() {
    let a = run(vec![new("f", true), alias("a", "f"), close("f"), use_("a")]);
    assert!(has(&a, "LIFE001"));
}
#[test]
fn shared_blocks_write_when_live() {
    let a = run(vec![
        new("x", false),
        borrow("r", "x", false),
        write("x"),
        use_("r"),
    ]);
    assert!(has(&a, "BOR002"));
}
#[test]
fn shared_allows_read() {
    let a = run(vec![
        new("x", false),
        borrow("r", "x", false),
        use_("x"),
        use_("r"),
    ]);
    assert!(a.diagnostics.is_empty());
}
#[test]
fn nll_releases_after_last_use() {
    let a = run(vec![
        new("x", false),
        borrow("r", "x", false),
        use_("r"),
        write("x"),
    ]);
    assert!(a.diagnostics.is_empty());
}
#[test]
fn unused_borrow_does_not_hold_loan() {
    let a = run(vec![new("x", false), borrow("r", "x", false), write("x")]);
    assert!(a.diagnostics.is_empty());
}
#[test]
fn handle_alias_keeps_loan_alive() {
    let a = run(vec![
        new("x", false),
        borrow("r", "x", false),
        alias("q", "r"),
        write("x"),
        use_("q"),
    ]);
    assert!(has(&a, "BOR002"));
}
#[test]
fn readonly_handle_cannot_write() {
    let a = run(vec![new("x", false), borrow("r", "x", false), write("r")]);
    assert!(has(&a, "BOR002"));
}
#[test]
fn exclusive_handle_can_write() {
    let a = run(vec![new("x", false), borrow("r", "x", true), write("r")]);
    assert!(a.diagnostics.is_empty());
}
#[test]
fn exclusive_blocks_original_read() {
    let a = run(vec![
        new("x", false),
        borrow("r", "x", true),
        use_("x"),
        use_("r"),
    ]);
    assert!(has(&a, "BOR002"));
}
#[test]
fn overlapping_shared_loans_allowed() {
    let a = run(vec![
        new("x", false),
        borrow("r", "x", false),
        borrow("q", "x", false),
        use_("r"),
        use_("q"),
    ]);
    assert!(a.diagnostics.is_empty());
}
#[test]
fn overlapping_exclusive_loans_rejected() {
    let a = run(vec![
        new("x", false),
        borrow("r", "x", false),
        borrow("q", "x", true),
        use_("r"),
        use_("q"),
    ]);
    assert!(has(&a, "BOR001"));
}
#[test]
fn cannot_escalate_shared_to_mutable() {
    let a = run(vec![
        new("x", false),
        borrow("r", "x", false),
        borrow("q", "r", true),
        write("q"),
    ]);
    assert!(has(&a, "BOR001"));
}
#[test]
fn close_or_move_borrowed_rejected() {
    for action in [close("x"), move_("x")] {
        let a = run(vec![
            new("x", true),
            borrow("r", "x", false),
            action,
            use_("r"),
        ]);
        assert!(has(&a, "BOR003"));
    }
}
#[test]
fn borrowed_resource_escape_reported() {
    let a = run(vec![
        new("f", true),
        borrow("r", "f", false),
        OpKind::Return {
            value: Some("r".into()),
        },
    ]);
    assert!(
        a.coverage
            .iter()
            .any(|g| g.reason.contains("caller contract"))
    );
}
#[test]
fn conditional_move_is_possible() {
    let a = run(vec![
        new("x", false),
        OpKind::Branch {
            then_body: vec![op(20, move_("x"))],
            else_body: vec![],
        },
        use_("x"),
    ]);
    let d = a.diagnostics.iter().find(|d| d.rule == "OWN001").unwrap();
    assert_eq!(d.confidence, Confidence::Possible);
}
#[test]
fn move_both_paths_is_definite() {
    let a = run(vec![
        new("x", false),
        OpKind::Branch {
            then_body: vec![op(20, move_("x"))],
            else_body: vec![op(21, move_("x"))],
        },
        use_("x"),
    ]);
    let d = a.diagnostics.iter().find(|d| d.rule == "OWN001").unwrap();
    assert_eq!(d.confidence, Confidence::Definite);
}
#[test]
fn terminated_branch_does_not_poison_continuation() {
    let a = run(vec![
        new("x", false),
        OpKind::Branch {
            then_body: vec![op(20, move_("x")), op(21, OpKind::Return { value: None })],
            else_body: vec![],
        },
        use_("x"),
    ]);
    assert!(!has(&a, "OWN001"));
}
#[test]
fn return_cleanup_closes_returned_resource() {
    let a = run(vec![
        new("f", true),
        OpKind::Scope {
            body: vec![op(
                10,
                OpKind::Return {
                    value: Some("f".into()),
                },
            )],
            cleanup: vec![op(11, close("f"))],
        },
    ]);
    assert!(has(&a, "LIFE002"));
}
#[test]
fn cleanup_runs_after_raise() {
    let a = run(vec![
        new("f", true),
        OpKind::Try {
            body: vec![op(
                5,
                OpKind::Scope {
                    body: vec![op(6, OpKind::Raise)],
                    cleanup: vec![op(7, close("f"))],
                },
            )],
            handlers: vec![vec![op(8, use_("f"))]],
            else_body: vec![],
            finally_body: vec![],
        },
    ]);
    assert!(has(&a, "LIFE001"));
}
#[test]
fn finally_runs_after_return() {
    let a = run(vec![
        new("f", true),
        OpKind::Try {
            body: vec![op(
                5,
                OpKind::Return {
                    value: Some("f".into()),
                },
            )],
            handlers: vec![],
            else_body: vec![],
            finally_body: vec![op(8, close("f"))],
        },
    ]);
    assert!(has(&a, "LIFE002"));
}
#[test]
fn unreachable_after_return_not_checked() {
    let a = run(vec![
        OpKind::Return { value: None },
        OpKind::MustUse { callee: "f".into() },
    ]);
    assert!(a.diagnostics.is_empty());
}
#[test]
fn must_use_and_unknown_visible() {
    let a = run(vec![
        OpKind::MustUse { callee: "f".into() },
        OpKind::Unknown {
            reason: "dynamic call".into(),
        },
        use_("unknown"),
    ]);
    assert!(has(&a, "ERR001"));
    assert_eq!(a.coverage.len(), 2);
}
#[test]
fn loop_detects_repeated_move() {
    let a = run(vec![
        new("x", false),
        OpKind::Loop {
            body: vec![op(5, move_("x"))],
            else_body: vec![],
        },
    ]);
    assert!(has(&a, "OWN002"));
}
#[test]
fn loop_break_skips_else() {
    let a = run(vec![
        new("x", false),
        OpKind::Loop {
            body: vec![op(5, move_("x")), op(6, OpKind::Break)],
            else_body: vec![op(7, use_("x"))],
        },
    ]);
    assert!(!has(&a, "OWN001"));
}
#[test]
fn overwritten_loop_allocations_converge() {
    let a = run(vec![OpKind::Loop {
        body: vec![op(5, new("x", false))],
        else_body: vec![],
    }]);
    assert!(a.coverage.is_empty());
}
#[test]
fn partial_branch_alias_unknown_is_visible() {
    let a = run(vec![
        new("x", false),
        OpKind::Branch {
            then_body: vec![op(5, alias("y", "x"))],
            else_body: vec![],
        },
        use_("y"),
    ]);
    assert!(!a.coverage.is_empty());
}

#[test]
fn ended_borrow_handle_is_invalid() {
    let a = run(vec![
        new("x", false),
        borrow("r", "x", false),
        OpKind::EndBorrow { target: "r".into() },
        use_("r"),
    ]);
    assert!(has(&a, "LIFE001"));
}
#[test]
fn allocation_generations_stay_distinct() {
    let a = run(vec![
        new("x", false),
        alias("old", "x"),
        OpKind::Loop {
            body: vec![op(5, new("x", false)), op(6, move_("x"))],
            else_body: vec![],
        },
        use_("old"),
    ]);
    assert!(!a.diagnostics.iter().any(|d| d.span.line == 4));
}

#[test]
fn overwritten_handle_ends_prior_loan() {
    let a = run(vec![
        new("x", false),
        borrow("r", "x", false),
        write("x"),
        new("r", false),
        use_("r"),
    ]);
    assert!(a.diagnostics.is_empty());
}

#[test]
fn cleanup_return_identity_survives_rebinding() {
    let a = run(vec![
        new("z", true),
        OpKind::Scope {
            body: vec![op(
                10,
                OpKind::Return {
                    value: Some("z".into()),
                },
            )],
            cleanup: vec![op(11, new("a", false)), op(12, close("z"))],
        },
    ]);
    assert!(has(&a, "LIFE002"));
}

#[test]
fn global_step_budget_is_visible() {
    let mut body = vec![op(1, new("f", true))];
    for i in 0..160 {
        body.push(op(
            2 + i * 2,
            OpKind::Branch {
                then_body: vec![op(3 + i * 2, close("f"))],
                else_body: vec![],
            },
        ));
    }
    let a = run_ops(body);
    assert!(a.coverage.iter().any(|g| g.reason.contains("step budget")));
}

#[test]
fn exclusive_reborrow_can_write_and_parent_resumes() {
    let a = run(vec![
        new("x", false),
        borrow("r", "x", true),
        borrow("q", "r", true),
        write("q"),
        write("r"),
    ]);
    assert!(a.diagnostics.is_empty(), "{:?}", a.diagnostics);
}
#[test]
fn reborrow_suspends_parent_until_last_use() {
    let a = run(vec![
        new("x", false),
        borrow("r", "x", true),
        borrow("q", "r", false),
        write("r"),
        use_("q"),
    ]);
    assert!(has(&a, "BOR002"));
}
#[test]
fn shared_reborrow_from_exclusive_can_read() {
    let a = run(vec![
        new("x", false),
        borrow("r", "x", true),
        borrow("q", "r", false),
        use_("q"),
        write("r"),
    ]);
    assert!(a.diagnostics.is_empty(), "{:?}", a.diagnostics);
}

#[test]
fn identity_inspection_allows_closed_resource() {
    let a = run(vec![
        new("f", true),
        close("f"),
        OpKind::Inspect { value: "f".into() },
    ]);
    assert!(a.diagnostics.is_empty());
}
#[test]
fn identity_inspection_rejects_moved_resource() {
    let a = run(vec![
        new("f", true),
        move_("f"),
        OpKind::Inspect { value: "f".into() },
    ]);
    assert!(has(&a, "OWN001"));
}
#[test]
fn identity_inspection_respects_exclusive_borrow() {
    let a = run(vec![
        new("f", true),
        borrow("r", "f", true),
        OpKind::Inspect { value: "f".into() },
        use_("r"),
    ]);
    assert!(has(&a, "BOR002"));
}
