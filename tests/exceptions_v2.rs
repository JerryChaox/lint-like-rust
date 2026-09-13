use lint_like_rust::{
    frontend_v2,
    solver_v2::{self, Analysis, ObligationStatus},
};
fn analyze(s: &str) -> Analysis {
    solver_v2::analyze(&frontend_v2::lower_project(&[("case.py".into(), s.into())]).unwrap())
}
#[test]
fn explicit_raise_reaches_bare_handler_and_skips_else() {
    let a = analyze(
        "try:\n    raise error\nexcept:\n    f=open('x')\n    f.close()\n    f.read()\nelse:\n    g=open('y')\n",
    );
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
    assert_eq!(
        a.obligations
            .iter()
            .filter(|o| o.operation == "acquire")
            .count(),
        1
    );
    assert!(
        a.obligations
            .iter()
            .any(|o| o.status == ObligationStatus::Unverified)
    );
}
#[test]
fn normal_completion_skips_handler_and_runs_else() {
    let a = analyze(
        "try:\n    pass\nexcept:\n    g=open('y')\n    g.close()\n    g.read()\nelse:\n    f=open('x')\n    f.close()\n    f.read()\n",
    );
    assert_eq!(a.findings.len(), 1, "{a:#?}");
    assert_eq!(
        a.obligations
            .iter()
            .filter(|o| o.operation == "acquire")
            .count(),
        1
    );
}
#[test]
fn reraise_in_handler_reaches_outer_handler() {
    let a = analyze(
        "try:\n    try:\n        raise error\n    except:\n        raise\nexcept:\n    f=open('x')\n    f.close()\n    f.read()\n",
    );
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
}
#[test]
fn raise_cleans_only_contexts_inside_protected_region() {
    let a = analyze(
        "with open('outer') as outer:\n    try:\n        with open('inner') as inner:\n            raise error\n    except:\n        pass\n",
    );
    assert_eq!(
        a.obligations
            .iter()
            .filter(|o| o.operation == "close")
            .count(),
        2,
        "{a:#?}"
    );
}
#[test]
fn unknown_implicit_exceptions_stay_visible() {
    let a = analyze("try:\n    f=open('x')\n    opaque(f)\nexcept:\n    pass\n");
    assert!(
        a.obligations
            .iter()
            .any(|o| o.status == ObligationStatus::Unverified),
        "{a:#?}"
    );
}
#[test]
fn normal_body_rebinding_cannot_restore_builtin_identity_after_try() {
    let a = analyze("try:\n    open=opaque\nexcept:\n    pass\nf=open('x')\n");
    assert!(
        !a.obligations.iter().any(|o| o.operation == "acquire"),
        "{a:#?}"
    );
}
#[test]
fn else_exception_bypasses_its_own_handler() {
    let a = analyze(
        "try:\n    try:\n        pass\n    except:\n        ignored=open('ignored')\n    else:\n        raise error\nexcept:\n    f=open('x')\n    f.close()\n    f.read()\n",
    );
    assert_eq!(
        a.obligations
            .iter()
            .filter(|o| o.operation == "acquire")
            .count(),
        1,
        "{a:#?}"
    );
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
}
#[test]
fn source_helper_raise_cannot_fall_through_to_new_resources() {
    let a = analyze("def fail():\n    raise error\nfail()\nf=open('x')\nf.close()\nf.read()\n");
    assert!(
        !a.obligations.iter().any(|o| o.operation == "acquire"),
        "{a:#?}"
    );
    assert!(
        a.obligations
            .iter()
            .any(|o| o.operation == "call" && o.status == ObligationStatus::Unverified),
        "{a:#?}"
    );
}
#[test]
fn source_caught_helper_exception_remains_explicitly_unverified() {
    let a = analyze("def fail():\n    raise error\ntry:\n    fail()\nexcept:\n    pass\n");
    assert!(
        a.obligations
            .iter()
            .any(|o| o.operation == "call" && o.status == ObligationStatus::Unverified),
        "{a:#?}"
    );
}
#[test]
fn helper_raise_now_executes_caller_handler() {
    let a = analyze(
        "def fail():\n    raise error\ntry:\n    fail()\nexcept:\n    f=open('x')\n    f.close()\n    f.read()\n",
    );
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
}
#[test]
fn helper_unwind_closes_inner_context_once() {
    let a = analyze(
        "def fail():\n    raise error\ntry:\n    with open('x') as f:\n        fail()\nexcept:\n    pass\n",
    );
    assert_eq!(
        a.obligations
            .iter()
            .filter(|o| o.operation == "close")
            .count(),
        1,
        "{a:#?}"
    );
}
#[test]
fn open_failure_reaches_handler_without_explicit_raise() {
    let a = analyze(
        "try:\n    source=open('source')\nexcept:\n    f=open('handler')\n    f.close()\n    f.read()\n",
    );
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
}
#[test]
fn read_and_write_failures_reach_handler() {
    for operation in ["f.read()", "f.write('x')", "f.close()"] {
        let a = analyze(&format!(
            "f=open('x')\ntry:\n    {operation}\nexcept:\n    h=open('handler')\n    h.close()\n    h.read()\n"
        ));
        assert!(
            a.findings.iter().any(|f| f.rule == "LIFE001"),
            "{operation}: {a:#?}"
        );
    }
}
#[test]
fn acquisition_failure_edge_precedes_acquire_and_binding() {
    use lint_like_rust::v2_ir::{Kind, Terminator};
    let p = frontend_v2::lower_project(&[(
        "case.py".into(),
        "try:\n    f=open('x')\nexcept:\n    pass\n".into(),
    )])
    .unwrap();
    let f = &p.functions[0];
    let entry = &f.blocks[f.entry];
    assert!(
        !entry
            .operations
            .iter()
            .any(|o| matches!(o.kind, Kind::Acquire { .. })),
        "{f:#?}"
    );
    let Terminator::Branch {
        then_target,
        else_target,
    } = entry.terminator
    else {
        panic!("{f:#?}")
    };
    assert!(
        f.blocks[then_target]
            .operations
            .iter()
            .any(|o| matches!(o.kind, Kind::Acquire { .. })),
        "{f:#?}"
    );
    assert!(
        !f.blocks[else_target]
            .operations
            .iter()
            .any(|o| matches!(o.kind, Kind::Acquire { .. } | Kind::Assign { .. })),
        "{f:#?}"
    );
}
#[test]
fn typed_handler_body_is_analyzed_with_matching_uncertainty() {
    let a = analyze(
        "try:\n    f=open('x')\nexcept CustomError:\n    h=open('handler')\n    h.close()\n    h.read()\n",
    );
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
    assert!(
        a.obligations
            .iter()
            .any(|o| o.status == ObligationStatus::Unverified),
        "{a:#?}"
    );
}
#[test]
fn unmatched_typed_handler_propagates_to_outer_handler() {
    let a = analyze(
        "try:\n    try:\n        raise error\n    except ValueError:\n        pass\nexcept:\n    f=open('x')\n    f.close()\n    f.read()\n",
    );
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
}
#[test]
fn later_handlers_remain_reachable_after_possible_nonmatch() {
    let a = analyze(
        "try:\n    raise error\nexcept ValueError:\n    pass\nexcept TypeError:\n    f=open('x')\n    f.close()\n    f.read()\n",
    );
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
}
#[test]
fn handler_alias_cannot_restore_builtin_open() {
    let a = analyze("try:\n    raise error\nexcept Exception as open:\n    f=open('x')\n");
    assert!(
        !a.obligations.iter().any(|o| o.operation == "acquire"),
        "{a:#?}"
    );
}
#[test]
fn baseexception_bypasses_exception_handler() {
    let a = analyze(
        "try:\n    raise KeyboardInterrupt\nexcept Exception:\n    f=open('unreachable')\n",
    );
    assert!(
        !a.obligations.iter().any(|o| o.operation == "acquire"),
        "{a:#?}"
    );
}
#[test]
fn ordinary_exception_reaches_exception_handler() {
    let a = analyze(
        "try:\n    raise ValueError\nexcept Exception:\n    f=open('x')\n    f.close()\n    f.read()\n",
    );
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
}
#[test]
fn baseexception_reaches_outer_bare_handler() {
    let a = analyze(
        "try:\n    try:\n        raise KeyboardInterrupt\n    except Exception:\n        pass\nexcept:\n    f=open('x')\n    f.close()\n    f.read()\n",
    );
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
}
#[test]
fn handler_class_rebinding_keeps_matching_unknown() {
    let a =
        analyze("try:\n    Exception=custom\n    raise ValueError\nexcept Exception:\n    pass\n");
    assert!(
        a.obligations
            .iter()
            .any(|o| o.status == ObligationStatus::Unverified),
        "{a:#?}"
    );
}
#[test]
fn return_cleanup_failure_can_reach_handler() {
    let a = analyze(
        "def run():\n    try:\n        with open('x') as f:\n            return 1\n    except Exception:\n        h=open('handler')\n        h.close()\n        h.read()\n",
    );
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
}
#[test]
fn unknown_context_suppression_stays_unverified() {
    let a = analyze("with custom as f:\n    pass\n");
    assert!(
        a.obligations
            .iter()
            .any(|o| o.status == ObligationStatus::Unverified),
        "{a:#?}"
    );
}
#[test]
fn file_argument_protocol_cannot_hide_effects() {
    let a = analyze("f=open('x')\nf.read(custom_size)\nf.read()\n");
    assert!(
        a.obligations
            .iter()
            .any(|o| o.operation == "read" && o.status == ObligationStatus::Unverified),
        "{a:#?}"
    );
}
#[test]
fn exact_standard_with_read_has_no_coverage_gap() {
    let a = analyze(
        "def run(path):\n    try:\n        with open(path) as f:\n            return f.read()\n    except Exception:\n        return ''\n",
    );
    assert!(a.findings.is_empty(), "{a:#?}");
    assert!(
        a.obligations
            .iter()
            .all(|o| o.status == ObligationStatus::Verified),
        "{a:#?}"
    );
}
