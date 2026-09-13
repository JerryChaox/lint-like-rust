use lint_like_rust::{
    frontend_v2,
    solver_v2::{self, Analysis, Certainty, ObligationStatus},
};
fn analyze(source: &str) -> Analysis {
    let p = frontend_v2::lower_project(&[("case.py".into(), source.into())]).unwrap();
    solver_v2::analyze(&p)
}
#[test]
fn loop_condition_is_rechecked_after_body_close() {
    let a = analyze("f=open('x')\nwhile f.read(1):\n    f.close()\n");
    assert!(
        a.findings
            .iter()
            .any(|f| f.rule == "LIFE001" && f.certainty == Certainty::Possible),
        "{a:#?}"
    );
}
#[test]
fn false_loop_does_not_execute_close() {
    let a = analyze("f=open('x')\nwhile False:\n    f.close()\nf.read()\n");
    assert!(a.findings.is_empty(), "{a:#?}");
    assert!(
        a.obligations
            .iter()
            .any(|o| o.operation == "read" && o.status == ObligationStatus::Verified)
    );
}
#[test]
fn break_skips_else_and_continue_skips_following_statement() {
    let a = analyze("f=open('x')\nwhile True:\n    break\nelse:\n    f.close()\nf.read()\n");
    assert!(a.findings.is_empty(), "{a:#?}");
    let b = analyze("f=open('x')\nwhile f.read(1):\n    continue\n    f.close()\nf.read()\n");
    assert!(b.findings.is_empty(), "{b:#?}");
    assert!(!b.obligations.iter().any(|o| o.operation == "close"));
}
#[test]
fn exhaustion_executes_else() {
    let a = analyze("f=open('x')\nwhile False:\n    pass\nelse:\n    f.close()\nf.read()\n");
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
}
#[test]
fn inner_context_closes_on_break() {
    let a = analyze("f=open('x')\nwhile True:\n    with f as h:\n        break\nf.read()\n");
    assert!(
        a.obligations.iter().any(|o| o.operation == "close"),
        "{a:#?}"
    );
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
}

#[test]
fn break_does_not_close_outer_context() {
    let a = analyze("with open('x') as f:\n    while True:\n        break\n    f.read()\n");
    assert!(a.findings.is_empty(), "{a:#?}");
    assert!(
        a.obligations
            .iter()
            .any(|o| o.operation == "read" && o.status == ObligationStatus::Verified)
    );
}
#[test]
fn rebound_receiver_is_not_assumed_to_keep_first_iteration_type() {
    let a = analyze(
        "from pathlib import Path\np=Path('x')\nwhile True:\n    f=p.open()\n    p=unknown()\n",
    );
    assert!(
        !a.obligations.iter().any(|o| o.operation == "acquire"),
        "{a:#?}"
    );
    assert!(
        a.obligations
            .iter()
            .any(|o| o.status == ObligationStatus::Unverified)
    );
}
#[test]
fn else_rebinding_cannot_restore_stale_path_type() {
    let a = analyze(
        "from pathlib import Path\np=Path('x')\nwhile False:\n    pass\nelse:\n    p=unknown()\nf=p.open()\nf.close()\nf.read()\n",
    );
    assert!(a.findings.is_empty(), "{a:#?}");
    assert!(
        !a.obligations.iter().any(|o| o.operation == "acquire"),
        "{a:#?}"
    );
}
#[test]
fn unknown_truth_conversion_invalidates_resource_proof() {
    let a = analyze("f=open('x')\nwhile condition:\n    break\nf.read()\n");
    assert!(
        a.obligations
            .iter()
            .any(|o| o.operation == "read" && o.status == ObligationStatus::Unverified),
        "{a:#?}"
    );
}

#[test]
fn sentinel_read_is_repeated_after_close() {
    let a = analyze("f=open('x')\nfor chunk in iter(lambda: f.read(1), b''):\n    f.close()\n");
    assert!(
        a.findings
            .iter()
            .any(|f| f.rule == "LIFE001" && f.certainty == Certainty::Possible),
        "{a:#?}"
    );
}
#[test]
fn sentinel_break_skips_next_call_and_else() {
    let a = analyze(
        "f=open('x')\nfor chunk in iter(lambda: f.read(1), b''):\n    f.close()\n    break\nelse:\n    f.close()\n",
    );
    assert!(a.findings.is_empty(), "{a:#?}");
    assert!(
        a.obligations
            .iter()
            .any(|o| o.operation == "read" && o.status == ObligationStatus::Verified),
        "{a:#?}"
    );
}
#[test]
fn sentinel_continue_reaches_next_call() {
    let a = analyze(
        "f=open('x')\nfor chunk in iter(lambda: f.read(1), b''):\n    f.close()\n    continue\n",
    );
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
}
#[test]
fn sentinel_exhaustion_reaches_else() {
    let a = analyze(
        "f=open('x')\nfor chunk in iter(lambda: f.read(1), b''):\n    pass\nelse:\n    f.close()\nf.read()\n",
    );
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
}
#[test]
fn sentinel_captured_rebinding_is_unknown() {
    for source in [
        "f=open('x')\nfor chunk in iter(lambda: f.read(1), b''):\n    f=opaque()\n",
        "f=open('x')\nfor f in iter(lambda: f.read(1), b''):\n    pass\n",
        "f=open('x')\nfor chunk in iter(lambda: callback(), b''):\n    pass\nf.read()\n",
    ] {
        let a = analyze(source);
        assert!(
            a.obligations
                .iter()
                .any(|o| o.status == ObligationStatus::Unverified),
            "{a:#?}"
        );
        assert!(
            !a.obligations
                .iter()
                .any(|o| o.operation == "read" && o.status == ObligationStatus::Verified),
            "{a:#?}"
        );
    }
}
#[test]
fn unsupported_iterator_shapes_do_not_claim_read_proofs() {
    for source in [
        "iter=opaque\nf=open('x')\nfor x in iter(lambda: f.read(1), b''):\n    pass\n",
        "f=open('x')\nfor x in iter(lambda required: f.read(1), b''):\n    pass\n",
        "f=open('x')\nfor x in iter(lambda: f.read(1), sentinel):\n    pass\n",
        "f=open('x')\nfor x in iter(lambda: f.read(1), sentinel=b''):\n    pass\n",
        "f=open('x')\nfor x in iter(lambda: f.read(1)):\n    pass\n",
    ] {
        let a = analyze(source);
        assert!(
            a.obligations
                .iter()
                .any(|o| o.status == ObligationStatus::Unverified),
            "{a:#?}"
        );
        assert!(
            !a.obligations.iter().any(|o| o.operation == "read"),
            "{a:#?}"
        );
    }
}
#[test]
fn lambda_creation_does_not_execute_its_body() {
    let a = analyze("f=open('x')\nf.close()\ncallback=lambda: f.read()\n");
    assert!(a.findings.is_empty(), "{a:#?}");
    assert!(
        !a.obligations.iter().any(|o| o.operation == "read"),
        "{a:#?}"
    );
    assert!(
        a.obligations
            .iter()
            .any(|o| o.status == ObligationStatus::Unverified)
    );
}
#[test]
fn sentinel_inner_with_cleanup_preserves_violation() {
    let a = analyze(
        "f=open('x')\nfor x in iter(lambda: f.read(1), b''):\n    with f as h:\n        break\nf.read()\n",
    );
    assert!(
        a.obligations.iter().any(|o| o.operation == "close"),
        "{a:#?}"
    );
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
}

#[test]
fn literal_integer_expressions_preserve_sentinel_read_proof() {
    for size in ["1024 * 1024", "(4 + 2) * 3", "-1", "~1", "(7 & 3) | 8"] {
        let a = analyze(&format!(
            "f=open('x')\nfor chunk in iter(lambda: f.read({size}), b''):\n    pass\n"
        ));
        assert!(a.findings.is_empty(), "{a:#?}");
        assert!(
            a.obligations
                .iter()
                .all(|o| o.status == ObligationStatus::Verified),
            "{size}: {a:#?}"
        );
        assert!(
            a.obligations.iter().any(|o| o.operation == "read"),
            "{a:#?}"
        );
    }
}
#[test]
fn annotated_or_dynamic_arithmetic_does_not_prove_purity() {
    for size in ["count * 1024", "opaque() * 1024", "1 / 0", "2 ** -1"] {
        let a = analyze(&format!(
            "f=open('x')\nfor chunk in iter(lambda: f.read({size}), b''):\n    pass\nf.read()\n"
        ));
        assert!(
            a.obligations
                .iter()
                .any(|o| o.operation == "read" && o.status == ObligationStatus::Unverified),
            "{size}: {a:#?}"
        );
    }
}
#[test]
fn interpolation_cannot_hide_resource_side_effects() {
    let a = analyze("f=open('x')\nmessage=f'{f.close()}'\nf.read()\n");
    assert!(
        a.obligations.iter().any(|o| o.operation == "close"),
        "{a:#?}"
    );
    assert!(
        !a.obligations
            .iter()
            .any(|o| o.operation == "read" && o.status == ObligationStatus::Verified),
        "{a:#?}"
    );
}
#[test]
fn interpolated_sentinel_remains_unverified() {
    let a = analyze(
        "f=open('x')\nfor chunk in iter(lambda: f.read(1), f'{opaque()}'):\n    pass\nf.read()\n",
    );
    assert!(
        a.obligations
            .iter()
            .any(|o| o.operation == "read" && o.status == ObligationStatus::Unverified),
        "{a:#?}"
    );
}
