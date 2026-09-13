use lint_like_rust::{
    frontend_v2,
    solver_v2::{self, ObligationStatus},
};
fn analyze(s: &str) -> solver_v2::Analysis {
    solver_v2::analyze(&frontend_v2::lower_project(&[("case.py".into(), s.into())]).unwrap())
}
#[test]
fn unchanged_hash_function_is_verified_in_exact_constructor_context() {
    let original = include_str!("corpus_v2/path_sha256/original.py");
    let a = analyze(&format!(
        "{original}\ndef run():\n    return _sha256(Path('input'))\n"
    ));
    assert!(
        a.obligations.iter().any(|o| o.operation == "read"),
        "{a:#?}"
    );
    assert!(
        a.obligations
            .iter()
            .all(|o| o.status == ObligationStatus::Verified),
        "{a:#?}"
    );
}
#[test]
fn hash_update_with_unknown_buffer_remains_unverified() {
    let a = analyze(
        "import hashlib\nf=open('x')\nd=hashlib.sha256()\nd.update(unknown_buffer)\nf.read()\n",
    );
    assert!(
        a.obligations
            .iter()
            .any(|o| o.status == ObligationStatus::Unverified),
        "{a:#?}"
    );
}
#[test]
fn hash_name_rebinding_does_not_acquire_library_effects() {
    let a = analyze("import hashlib\nhashlib=custom\nd=hashlib.sha256()\n");
    assert!(
        !a.obligations
            .iter()
            .any(|o| o.operation == "library_effect"),
        "{a:#?}"
    );
}
#[test]
fn unchanged_mutation_and_repair_are_distinguished_in_constructor_context() {
    for (source, bad) in [
        (include_str!("corpus_v2/path_sha256/mutated_error.py"), true),
        (include_str!("corpus_v2/path_sha256/repaired.py"), false),
    ] {
        let a = analyze(&format!(
            "{source}\ndef run():\n    return _sha256(Path('input'))\n"
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
fn project_hashlib_module_is_not_the_standard_library() {
    let p = frontend_v2::lower_project(&[
        (
            "main.py".into(),
            "import hashlib\ndef run():\n    return hashlib.sha256()\n".into(),
        ),
        (
            "hashlib.py".into(),
            "def sha256():\n    return opaque()\n".into(),
        ),
    ])
    .unwrap();
    let a = solver_v2::analyze(&p);
    assert!(
        !a.obligations
            .iter()
            .any(|o| o.operation == "library_effect"),
        "{a:#?}"
    );
    assert!(
        a.obligations
            .iter()
            .any(|o| o.status == ObligationStatus::Unverified),
        "{a:#?}"
    );
}
