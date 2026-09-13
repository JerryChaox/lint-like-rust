use lint_like_rust::{
    frontend_v2,
    solver_v2::{self, ObligationStatus},
};
fn analyze(s: &str) -> solver_v2::Analysis {
    solver_v2::analyze(&frontend_v2::lower_project(&[("case.py".into(), s.into())]).unwrap())
}
#[test]
fn full_hash_group_preserves_results_through_derived_paths() {
    for (source, bad) in [
        (include_str!("corpus_v2/path_sha256/original.py"), false),
        (include_str!("corpus_v2/path_sha256/mutated_error.py"), true),
        (include_str!("corpus_v2/path_sha256/repaired.py"), false),
    ] {
        let a = analyze(&format!(
            "{source}\ndef run():\n    root=Path('workspace').resolve()\n    output=root / 'subdir' / 'rendered.mp4'\n    return _sha256(output.parent / 'rendered.mp4')\n"
        ));
        assert_eq!(
            a.findings.iter().any(|f| f.rule == "LIFE001"),
            bad,
            "{a:#?}"
        );
        assert!(
            a.obligations
                .iter()
                .all(|o| o.status != ObligationStatus::Unverified),
            "{a:#?}"
        );
    }
}
#[test]
fn dynamic_path_operands_do_not_gain_exact_dispatch() {
    for expression in [
        "root / part",
        "root / b'bytes'",
        "root / f'{custom}'",
        "custom.resolve()",
        "custom.parent",
    ] {
        let a = analyze(&format!(
            "from pathlib import Path\nroot=Path('x')\np={expression}\nf=p.open()\n"
        ));
        assert!(
            a.obligations
                .iter()
                .any(|o| o.status == ObligationStatus::Unverified),
            "{expression}: {a:#?}"
        );
    }
}
#[test]
fn nominal_root_does_not_become_exact_after_join() {
    let a = analyze(
        "from pathlib import Path\ndef run(root: Path):\n    p=root / 'child'\n    return p.open()\n",
    );
    assert!(
        !a.obligations.iter().any(|o| o.operation == "acquire"),
        "{a:#?}"
    );
}
#[test]
fn unknown_path_constructor_protocol_cannot_hide_resource_effects() {
    let a = analyze("from pathlib import Path\nf=open('x')\np=Path(custom)\nf.read()\n");
    assert!(
        a.obligations
            .iter()
            .any(|o| o.operation == "read" && o.status == ObligationStatus::Unverified),
        "{a:#?}"
    );
}
