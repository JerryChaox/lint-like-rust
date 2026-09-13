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
fn check(body: &str, own: bool, life: bool) -> Analysis {
    let a = analyze(body);
    assert_eq!(a.findings.iter().any(|f| f.rule == "OWN001"), own, "{a:#?}");
    assert_eq!(
        a.findings.iter().any(|f| f.rule == "LIFE001"),
        life,
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
fn transfer_revokes_old_alias_but_not_resource() {
    check(
        "    raw=io.BytesIO(b'payload')\n    old=raw\n    text=io.TextIOWrapper(raw,'utf-8')\n    return old.read()",
        true,
        false,
    );
    check(
        "    raw=io.BytesIO(b'payload')\n    text=io.TextIOWrapper(raw,'utf-8')\n    return text.read()",
        false,
        false,
    );
}
#[test]
fn ordinary_alias_is_not_move() {
    check(
        "    raw=io.BytesIO()\n    other=raw\n    return raw.read()",
        false,
        false,
    );
}
#[test]
fn detach_returns_new_capability_without_restoring_old_alias() {
    check(
        "    raw=io.BytesIO()\n    text=io.TextIOWrapper(raw,'utf-8')\n    recovered=text.detach()\n    return recovered.read()",
        false,
        false,
    );
    check(
        "    raw=io.BytesIO()\n    text=io.TextIOWrapper(raw,'utf-8')\n    recovered=text.detach()\n    return raw.read()",
        true,
        false,
    );
    check(
        "    raw=io.BytesIO()\n    text=io.TextIOWrapper(raw,'utf-8')\n    recovered=text.detach()\n    return text.read()",
        true,
        false,
    );
}
#[test]
fn recipient_close_preserves_underlying_resource_relation() {
    check(
        "    raw=io.BytesIO()\n    text=io.TextIOWrapper(raw,'utf-8')\n    text.close()\n    return text.read()",
        false,
        true,
    );
}
#[test]
fn branch_transfer_is_possible_and_rebinding_restores_new_resource() {
    let a = check(
        "    raw=io.BytesIO()\n    if flag:\n        text=io.TextIOWrapper(raw,'utf-8')\n    return raw.read()",
        true,
        false,
    );
    assert!(
        a.findings
            .iter()
            .any(|f| f.rule == "OWN001" && f.certainty == Certainty::Possible)
    );
    check(
        "    raw=io.BytesIO()\n    text=io.TextIOWrapper(raw,'utf-8')\n    raw=io.BytesIO()\n    return raw.read()",
        false,
        false,
    );
}
#[test]
fn unknown_effects_never_restore_capability() {
    let a = analyze(
        "    raw=io.BytesIO()\n    text=io.TextIOWrapper(raw,'utf-8')\n    external(text)\n    return raw.read()",
    );
    assert!(
        a.obligations
            .iter()
            .any(|o| o.status == ObligationStatus::Unverified)
    );
}
#[test]
fn transfer_in_callee_reaches_caller_alias_and_recipient_return() {
    for access in ["raw.read()", "text.read()"] {
        let source = format!(
            "import io\ndef wrap(raw):\n    return io.TextIOWrapper(raw,'utf-8')\ndef run():\n    raw=io.BytesIO()\n    text=wrap(raw)\n    return {access}\n"
        );
        let mut p = frontend_v2::lower_project(&[("case.py".into(), source)]).unwrap();
        p.roots = vec!["case::run".into()];
        let a = solver_v2::analyze(&p);
        assert_eq!(
            a.findings.iter().any(|f| f.rule == "OWN001"),
            access.starts_with("raw"),
            "{a:#?}"
        );
        assert!(
            !a.obligations
                .iter()
                .any(|o| o.status == ObligationStatus::Unverified),
            "{a:#?}"
        );
    }
}
#[test]
fn exception_after_transfer_preserves_revocation() {
    let a = check(
        "    raw=io.BytesIO()\n    try:\n        text=io.TextIOWrapper(raw,'utf-8')\n        raise ValueError()\n    except Exception:\n        return raw.read()",
        true,
        false,
    );
    assert!(
        a.findings
            .iter()
            .any(|f| f.rule == "OWN001" && f.certainty == Certainty::Possible)
    );
}

#[test]
fn field_stored_old_capability_and_cross_module_recipient_are_distinct() {
    let source = "import io\nfrom helper import wrap\nclass Box:\n    def __init__(self,f):\n        self.f=f\ndef run():\n    raw=io.BytesIO()\n    box=Box(raw)\n    text=wrap(box.f)\n    return box.f.read()\n";
    let helper = "import io\ndef wrap(f):\n    return io.TextIOWrapper(f,'utf-8')\n";
    let mut p = frontend_v2::lower_project(&[
        ("case.py".into(), source.into()),
        ("helper.py".into(), helper.into()),
    ])
    .unwrap();
    p.roots = vec!["case::run".into()];
    let a = solver_v2::analyze(&p);
    assert!(a.findings.iter().any(|f| f.rule == "OWN001"), "{a:#?}");
    assert!(
        !a.obligations
            .iter()
            .any(|o| o.status == ObligationStatus::Unverified),
        "{a:#?}"
    );
}
#[test]
fn callee_exception_writes_back_transfer_to_caller() {
    let source = "import io\ndef wrap(f):\n    text=io.TextIOWrapper(f,'utf-8')\n    raise ValueError()\ndef run():\n    raw=io.BytesIO()\n    try:\n        wrap(raw)\n    except Exception:\n        return raw.read()\n";
    let mut p = frontend_v2::lower_project(&[("case.py".into(), source.into())]).unwrap();
    p.roots = vec!["case::run".into()];
    let a = solver_v2::analyze(&p);
    assert!(
        a.findings
            .iter()
            .any(|f| f.rule == "OWN001" && f.certainty == Certainty::Possible),
        "{a:#?}"
    );
}
#[test]
fn unknown_buffer_and_shadowed_wrapper_are_not_ownership_models() {
    for source in [
        "import io\ndef run(raw):\n    text=io.TextIOWrapper(raw,'utf-8')\n    return raw.read()\n",
        "import io\ndef run():\n    io.TextIOWrapper=external\n    raw=io.BytesIO()\n    text=io.TextIOWrapper(raw,'utf-8')\n    return raw.read()\n",
    ] {
        let mut p = frontend_v2::lower_project(&[("case.py".into(), source.into())]).unwrap();
        p.roots = vec!["case::run".into()];
        let a = solver_v2::analyze(&p);
        assert!(
            a.obligations
                .iter()
                .any(|o| o.status == ObligationStatus::Unverified),
            "{a:#?}"
        );
        assert!(!a.findings.iter().any(|f| f.rule == "OWN001"), "{a:#?}");
    }
}

#[test]
fn multiple_possible_transfer_sources_use_weak_revocation() {
    let a = check(
        "    a=io.BytesIO()\n    b=io.BytesIO()\n    if flag:\n        source=a\n    else:\n        source=b\n    text=io.TextIOWrapper(source,'utf-8')\n    return a.read()",
        true,
        false,
    );
    assert!(
        a.findings
            .iter()
            .any(|f| f.rule == "OWN001" && f.certainty == Certainty::Possible),
        "{a:#?}"
    );
}
#[test]
fn disjoint_resource_permissions_are_not_revoked() {
    check(
        "    a=io.BytesIO()\n    b=io.BytesIO()\n    text=io.TextIOWrapper(a,'utf-8')\n    return b.read()",
        false,
        false,
    );
}
#[test]
fn illegal_old_close_still_mutates_underlying_resource() {
    check(
        "    raw=io.BytesIO()\n    text=io.TextIOWrapper(raw,'utf-8')\n    raw.close()\n    return text.read()",
        true,
        true,
    );
}
#[test]
fn copying_revoked_alias_cannot_restore_ownership() {
    check(
        "    raw=io.BytesIO()\n    text=io.TextIOWrapper(raw,'utf-8')\n    copied=raw\n    return copied.read()",
        true,
        false,
    );
}

#[test]
fn agent_report_proves_reordered_repair_and_never_counts_unknown_as_fixed() {
    use lint_like_rust::{diagnostics_v2 as d, report_v2};
    let make = |source: &str| {
        let mut p = frontend_v2::lower_project(&[("case.py".into(), source.into())]).unwrap();
        p.roots = vec!["case::run".into()];
        report_v2::build_report(
            &solver_v2::analyze(&p),
            d::AnalysisContext {
                semantics_revision: "20".into(),
                configuration_fingerprint: "policy-v1".into(),
                environment_fingerprint: "test".into(),
            },
        )
    };
    let before = make(include_str!(
        "../examples/v2/ownership-transfer/before/case.py"
    ));
    let after = make(include_str!(
        "../examples/v2/ownership-transfer/after/case.py"
    ));
    let unknown = make(include_str!(
        "../examples/v2/ownership-transfer/unknown/case.py"
    ));
    let violation = before
        .obligations
        .iter()
        .find(|o| o.status == d::ProofStatus::Violated)
        .unwrap();
    let e = violation.evidence.as_ref().unwrap();
    assert_eq!(e.class, d::RuleClass::SafetyPolicy);
    assert!(
        e.trace
            .iter()
            .any(|t| t.explanation.contains("alias preserves"))
    );
    assert!(
        e.trace
            .iter()
            .any(|t| t.explanation.contains("transferred"))
    );
    assert_eq!(
        d::compare_reports(&before, &after).issues[0].kind,
        d::ChangeKind::Resolved
    );
    assert_eq!(
        d::compare_reports(&before, &unknown).issues[0].kind,
        d::ChangeKind::BecameUnverified
    );
}

#[test]
fn explicit_dynamic_library_mutations_disable_models() {
    for mutation in [
        "del io.TextIOWrapper",
        "setattr(io,'TextIOWrapper',external)",
        "delattr(io,'TextIOWrapper')",
        "io.__dict__['TextIOWrapper']=external",
        "io.TextIOWrapper += external",
    ] {
        let a = analyze(&format!(
            "    {mutation}\n    raw=io.BytesIO()\n    text=io.TextIOWrapper(raw,'utf-8')\n    return raw.read()"
        ));
        assert!(
            a.obligations
                .iter()
                .any(|o| o.status == ObligationStatus::Unverified),
            "{mutation}: {a:#?}"
        );
        assert!(
            !a.findings.iter().any(|f| f.rule == "OWN001"),
            "{mutation}: {a:#?}"
        );
    }
}

#[test]
fn detached_wrapper_close_does_not_close_recovered_buffer() {
    check(
        "    raw=io.BytesIO()\n    text=io.TextIOWrapper(raw,'utf-8')\n    recovered=text.detach()\n    try:\n        text.close()\n    except Exception:\n        return recovered.read()",
        true,
        false,
    );
}
#[test]
fn detach_during_context_cleanup_is_explicitly_unverified() {
    let a = analyze(
        "    raw=io.BytesIO()\n    with io.TextIOWrapper(raw,'utf-8') as text:\n        recovered=text.detach()\n    return recovered.read()",
    );
    assert!(
        a.obligations
            .iter()
            .any(|o| o.status == ObligationStatus::Unverified),
        "{a:#?}"
    );
}
