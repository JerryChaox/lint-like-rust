use lint_like_rust::{
    frontend_v2,
    solver_v2::{self, Analysis, Certainty, ObligationStatus},
};
fn analyze(body: &str) -> Analysis {
    let source =
        format!("import io\ndef run(flag):\n{body}\ndef driver():\n    return run(True)\n");
    let mut p = frontend_v2::lower_project(&[("case.py".into(), source)]).unwrap();
    p.roots = vec!["case::driver".into()];
    solver_v2::analyze(&p)
}
fn analyze_source(source: &str) -> Analysis {
    let mut p = frontend_v2::lower_project(&[("case.py".into(), source.into())]).unwrap();
    p.roots = vec!["case::run".into()];
    solver_v2::analyze(&p)
}
fn check(body: &str, conflict: bool) -> Analysis {
    let a = analyze(body);
    assert_eq!(
        a.findings.iter().any(|f| f.rule == "BORROW001"),
        conflict,
        "{a:#?}"
    );
    assert!(
        !a.obligations
            .iter()
            .any(|o| o.status == ObligationStatus::Unverified),
        "{a:#?}"
    );
    a
}
#[test]
fn exclusive_view_blocks_owner_but_release_restores_access() {
    check(
        "    raw=io.BytesIO(b'abc')\n    view=raw.getbuffer()\n    data=raw.read()\n    view.release()\n    return data",
        true,
    );
    check(
        "    raw=io.BytesIO(b'abc')\n    view=raw.getbuffer()\n    view.release()\n    return raw.read()",
        false,
    );
}
#[test]
fn alias_release_ends_same_view_and_is_idempotent() {
    check(
        "    raw=io.BytesIO()\n    view=raw.getbuffer()\n    other=view\n    other.release()\n    view.release()\n    return view.tobytes()",
        true,
    );
    check(
        "    raw=io.BytesIO()\n    view=raw.getbuffer()\n    other=view\n    other.release()\n    view.release()\n    return raw.read()",
        false,
    );
}
#[test]
fn view_read_write_and_readonly_restriction() {
    check(
        "    raw=io.BytesIO(b'abc')\n    view=raw.getbuffer()\n    view[0]=65\n    return view[0]",
        false,
    );
    check("    view=memoryview(b'abc')\n    view[0]=65", true);
}
#[test]
fn readonly_child_outlives_released_parent() {
    check(
        "    raw=io.BytesIO(b'abc')\n    view=raw.getbuffer()\n    child=view.toreadonly()\n    view.release()\n    data=child.tobytes()\n    return raw.read()",
        false,
    );
    check(
        "    raw=io.BytesIO(b'abc')\n    view=raw.getbuffer()\n    child=view.toreadonly()\n    view.release()\n    raw.write(b'x')",
        true,
    );
    check(
        "    raw=io.BytesIO(b'abc')\n    view=raw.getbuffer()\n    child=view.toreadonly()\n    view.release()\n    child.release()\n    raw.write(b'x')",
        false,
    );
}
#[test]
fn sibling_shared_views_coexist_and_freeze_parent_write() {
    check(
        "    raw=io.BytesIO(b'abc')\n    view=raw.getbuffer()\n    a=view.toreadonly()\n    b=view.toreadonly()\n    data=a.tobytes()\n    return b.tobytes()",
        false,
    );
    check(
        "    raw=io.BytesIO(b'abc')\n    view=raw.getbuffer()\n    a=view.toreadonly()\n    view[0]=65",
        true,
    );
}
#[test]
fn branch_creation_and_release_are_possible_conflicts() {
    for body in [
        "    raw=io.BytesIO()\n    if flag:\n        view=raw.getbuffer()\n    return raw.read()",
        "    raw=io.BytesIO()\n    view=raw.getbuffer()\n    if flag:\n        view.release()\n    return raw.read()",
    ] {
        let a = check(body, true);
        assert!(
            a.findings
                .iter()
                .any(|f| f.rule == "BORROW001" && f.certainty == Certainty::Possible),
            "{a:#?}"
        );
    }
}
#[test]
fn independent_resource_unaffected_and_alias_owner_conflicts() {
    check(
        "    raw=io.BytesIO()\n    other=io.BytesIO()\n    view=raw.getbuffer()\n    return other.read()",
        false,
    );
    check(
        "    raw=io.BytesIO()\n    old=raw\n    view=raw.getbuffer()\n    return old.read()",
        true,
    );
}
#[test]
fn unknown_effects_and_shadowed_api_stay_unverified() {
    for body in [
        "    raw=io.BytesIO()\n    view=raw.getbuffer()\n    external(view)\n    view.release()\n    return raw.read()",
        "    memoryview=external\n    view=memoryview(b'abc')\n    return view.tobytes()",
        "    raw=external()\n    view=raw.getbuffer()\n    return view.tobytes()",
        "    raw=io.BytesIO()\n    view=raw.getbuffer()\n    sliced=view[:]\n    view.release()\n    raw.close()",
    ] {
        let a = analyze(body);
        assert!(
            a.obligations
                .iter()
                .any(|o| o.status == ObligationStatus::Unverified),
            "{a:#?}"
        );
    }
}
#[test]
fn view_context_releases_on_exit_and_return() {
    check(
        "    raw=io.BytesIO()\n    with raw.getbuffer() as view:\n        data=view.tobytes()\n    return raw.read()",
        false,
    );
    check(
        "    raw=io.BytesIO()\n    with raw.getbuffer() as view:\n        data=view.tobytes()\n    return view.tobytes()",
        true,
    );
    let a = analyze_source(
        "import io\ndef scoped(raw):\n    with raw.getbuffer() as v:\n        return v\ndef run(flag):\n    raw=io.BytesIO()\n    v=scoped(raw)\n    return v.tobytes()\n",
    );
    assert!(a.findings.iter().any(|f| f.rule == "BORROW001"), "{a:#?}");
}
#[test]
fn buffer_close_with_export_does_not_close_resource() {
    let a = check(
        "    raw=io.BytesIO()\n    view=raw.getbuffer()\n    raw.close()\n    view.release()\n    return raw.read()",
        true,
    );
    assert!(!a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
}
#[test]
fn call_release_and_return_propagate_loan_identity() {
    let a = analyze_source(
        "import io\ndef get(raw):\n    return raw.getbuffer()\ndef release(v):\n    v.release()\ndef run(flag):\n    raw=io.BytesIO()\n    view=get(raw)\n    release(view)\n    return view.tobytes()\n",
    );
    assert!(a.findings.iter().any(|f| f.rule == "BORROW001"), "{a:#?}");
    assert!(
        !a.obligations
            .iter()
            .any(|o| o.status == ObligationStatus::Unverified),
        "{a:#?}"
    );
}
#[test]
fn fields_preserve_borrow_aliases() {
    let a = analyze_source(
        "import io\nclass Holder:\n    def __init__(self, view):\n        self.view=view\n    def release(self):\n        self.view.release()\ndef run(flag):\n    raw=io.BytesIO()\n    view=raw.getbuffer()\n    h=Holder(view)\n    h.release()\n    return view.tobytes()\n",
    );
    assert!(a.findings.iter().any(|f| f.rule == "BORROW001"), "{a:#?}");
    assert!(
        !a.obligations
            .iter()
            .any(|o| o.status == ObligationStatus::Unverified),
        "{a:#?}"
    );
}
#[test]
fn exception_unwind_preserves_release_and_context_cleanup() {
    let a = analyze_source(
        "import io\ndef release(v):\n    v.release()\n    raise ValueError()\ndef run(flag):\n    raw=io.BytesIO()\n    v=raw.getbuffer()\n    try:\n        release(v)\n    except Exception:\n        return v.tobytes()\n",
    );
    assert!(a.findings.iter().any(|f| f.rule == "BORROW001"), "{a:#?}");
    check(
        "    raw=io.BytesIO()\n    view=raw.getbuffer()\n    try:\n        with view:\n            raise ValueError()\n    except Exception:\n        return raw.read()",
        false,
    );
}
#[test]
fn new_exclusive_view_conflicts_with_existing_export() {
    check(
        "    raw=io.BytesIO()\n    a=raw.getbuffer()\n    b=raw.getbuffer()\n    return b.tobytes()",
        true,
    );
}
#[test]
fn released_view_enter_keeps_real_exception_handler_effects() {
    let a = analyze(
        "    raw=io.BytesIO()\n    view=raw.getbuffer()\n    view.release()\n    try:\n        with view:\n            pass\n    except Exception:\n        raw.close()\n    return raw.read()",
    );
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
}
#[test]
fn policy_report_requires_explicit_same_obligation_repair() {
    use lint_like_rust::{diagnostics_v2 as d, report_v2};
    let make = |source: &str| {
        report_v2::build_report(
            &analyze_source(source),
            d::AnalysisContext {
                semantics_revision: "21".into(),
                configuration_fingerprint: "default".into(),
                environment_fingerprint: "test".into(),
            },
        )
    };
    let before = make(include_str!("../examples/v2/borrowing/before/case.py"));
    let after = make(include_str!("../examples/v2/borrowing/after/case.py"));
    let unknown = make(include_str!("../examples/v2/borrowing/unknown/case.py"));
    let evidence = before
        .obligations
        .iter()
        .find_map(|o| o.evidence.as_ref())
        .unwrap();
    assert_eq!(evidence.class, d::RuleClass::SafetyPolicy);
    assert!(
        evidence
            .trace
            .iter()
            .any(|t| t.explanation.contains("exclusive loan"))
    );
    assert!(
        d::compare_reports(&before, &after)
            .issues
            .iter()
            .any(|i| i.kind == d::ChangeKind::Resolved)
    );
    assert!(
        d::compare_reports(&before, &unknown)
            .issues
            .iter()
            .all(|i| i.kind == d::ChangeKind::BecameUnverified)
    );
}
