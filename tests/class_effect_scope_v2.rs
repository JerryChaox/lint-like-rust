use lint_like_rust::{
    frontend_v2,
    solver_v2::{self, ObligationStatus},
};
const CLASS: &str = "class Reader:\n    def read(self, f):\n        return f.read()\n";
fn analyze(extra: &str, body: &str) -> solver_v2::Analysis {
    let source = format!("{CLASS}{extra}\ndef run():\n{body}");
    let mut p = frontend_v2::lower_project(&[("case.py".into(), source)]).unwrap();
    p.roots = vec!["case::run".into()];
    solver_v2::analyze(&p)
}
fn unknown(a: &solver_v2::Analysis) -> bool {
    a.obligations
        .iter()
        .any(|o| o.status == ObligationStatus::Unverified)
}
#[test]
fn unrelated_unknown_function_does_not_hide_class_error_or_repair() {
    for (body, bad) in [
        (
            "    r=Reader()\n    f=open('x')\n    f.close()\n    return r.read(f)\n",
            true,
        ),
        (
            "    r=Reader()\n    f=open('x')\n    with f:\n        return r.read(f)\n",
            false,
        ),
    ] {
        let a = analyze("def unrelated(x):\n    external(x)\n", body);
        assert_eq!(
            a.findings.iter().any(|f| f.rule == "LIFE001"),
            bad,
            "{a:#?}"
        );
        assert!(!unknown(&a), "{a:#?}");
    }
}
#[test]
fn unknown_callee_before_resource_acquisition_blocks_caller() {
    let a = analyze(
        "def mutate():\n    external()\n",
        "    mutate()\n    r=Reader()\n    f=open('x')\n    return r.read(f)\n",
    );
    assert!(unknown(&a), "{a:#?}");
}
#[test]
fn unknown_caller_blocks_otherwise_pure_acquiring_callee() {
    let a = analyze(
        "def acquire_and_read():\n    r=Reader()\n    f=open('x')\n    return r.read(f)\n",
        "    external()\n    return acquire_and_read()\n",
    );
    assert!(unknown(&a), "{a:#?}");
}
#[test]
fn sibling_call_sharing_a_method_is_conservatively_contaminated() {
    let a = analyze(
        "def other():\n    external()\n    r=Reader()\n    f=open('y')\n    return r.read(f)\n",
        "    r=Reader()\n    f=open('x')\n    return r.read(f)\n",
    );
    assert!(unknown(&a), "{a:#?}");
}
#[test]
fn effectful_module_initialization_remains_in_selected_entry() {
    let a = analyze(
        "external()\n",
        "    r=Reader()\n    f=open('x')\n    return r.read(f)\n",
    );
    assert!(unknown(&a), "{a:#?}");
}
