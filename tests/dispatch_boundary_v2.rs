use lint_like_rust::{
    frontend_v2,
    solver_v2::{self, ObligationStatus},
    v2_ir::Kind,
};
const READ: &str = "from pathlib import Path\ndef read(path: Path):\n    with path.open('rb') as handle:\n        return handle.read()\n";
#[test]
fn nominal_boundary_cannot_claim_standard_open_effects() {
    let p = frontend_v2::lower_project(&[("case.py".into(), READ.into())]).unwrap();
    assert!(
        !p.functions
            .iter()
            .flat_map(|f| &f.blocks)
            .flat_map(|b| &b.operations)
            .any(|i| matches!(i.kind, Kind::Acquire { .. }))
    );
    let a = solver_v2::analyze(&p);
    assert!(
        a.obligations
            .iter()
            .any(|o| o.status == ObligationStatus::Unverified),
        "{a:#?}"
    );
}
#[test]
fn closed_constructor_context_proves_same_function_without_annotation_changes() {
    let source = format!("{READ}\ndef run():\n    return read(Path('input'))\n");
    let p = frontend_v2::lower_project(&[("case.py".into(), source)]).unwrap();
    let a = solver_v2::analyze(&p);
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
fn overriding_subclass_does_not_inherit_verified_factory_model() {
    // Static input only: neither the subclass nor its open method is executed.
    let source = format!(
        "{READ}\nfrom pathlib import PosixPath\nclass ClosedPath(PosixPath):\n    def open(self, *args, **kwargs):\n        stream=super().open(*args, **kwargs)\n        stream.close()\n        return stream\nread(ClosedPath('input'))\n"
    );
    let p = frontend_v2::lower_project(&[("case.py".into(), source)]).unwrap();
    let a = solver_v2::analyze(&p);
    assert!(
        a.obligations
            .iter()
            .any(|o| o.status == ObligationStatus::Unverified),
        "{a:#?}"
    );
    assert!(
        !a.obligations
            .iter()
            .any(|o| o.operation == "acquire" && o.status == ObligationStatus::Verified),
        "{a:#?}"
    );
}
