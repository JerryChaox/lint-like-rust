use lint_like_rust::{
    frontend_v2,
    solver_v2::{self, Analysis, ObligationStatus},
};
fn analyze(body: &str) -> Analysis {
    let source = format!(
        "class Box:\n    def __init__(self,f):\n        self.f=f\n    def close(self):\n        self.f.close()\n    def read(self):\n        return self.f.read()\ndef run():\n{body}\n"
    );
    let mut p = frontend_v2::lower_project(&[("case.py".into(), source)]).unwrap();
    p.roots = vec!["case::run".into()];
    solver_v2::analyze(&p)
}
fn check(body: &str, violation: bool) {
    let a = analyze(body);
    assert_eq!(
        a.findings.iter().any(|f| f.rule == "LIFE001"),
        violation,
        "{a:#?}"
    );
    assert!(
        !a.obligations
            .iter()
            .any(|o| o.status == ObligationStatus::Unverified),
        "{a:#?}"
    );
}
#[test]
fn constructor_fields_and_method_aliases() {
    check(
        "    f=open('x')\n    b=Box(f)\n    alias=b\n    alias.close()\n    return b.read()",
        true,
    );
    check(
        "    f=open('x')\n    b=Box(f)\n    value=b.read()\n    b.close()\n    return value",
        false,
    );
}
#[test]
fn separate_instances_keep_resource_identity() {
    check(
        "    f=open('x')\n    g=open('y')\n    a=Box(f)\n    b=Box(g)\n    a.close()\n    return b.read()",
        false,
    );
}
#[test]
fn field_read_snapshots_old_reference() {
    check(
        "    f=open('x')\n    g=open('y')\n    b=Box(f)\n    old=b.f\n    b.f=g\n    old.close()\n    return b.read()",
        false,
    );
    check(
        "    f=open('x')\n    g=open('y')\n    b=Box(f)\n    old=b.f\n    b.f=g\n    f.close()\n    return old.read()",
        true,
    );
}
#[test]
fn unresolved_field_effects_remain_unknown() {
    for body in [
        "    f=open('x')\n    b=Box(f)\n    b.f=external()\n    return b.read()",
        "    f=open('x')\n    b=Box(f)\n    b.read=f\n    return b.read()",
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
fn exceptional_field_store_keeps_previous_value_possible() {
    check(
        "    f=open('x')\n    g=open('y')\n    b=Box(f)\n    f.close()\n    try:\n        b.f=g\n        raise ValueError()\n    except Exception:\n        return b.read()",
        true,
    );
}
#[test]
fn conditionally_missing_field_is_unverified() {
    let source = "class Box:\n    def __init__(self,f,flag):\n        if flag:\n            self.f=f\n    def read(self):\n        return self.f.read()\ndef run(flag):\n    f=open('x')\n    b=Box(f,flag)\n    return b.read()\n";
    let mut p = frontend_v2::lower_project(&[("case.py".into(), source.into())]).unwrap();
    p.roots = vec!["case::run".into()];
    let a = solver_v2::analyze(&p);
    assert!(
        a.obligations
            .iter()
            .any(|o| o.status == ObligationStatus::Unverified),
        "{a:#?}"
    );
}

fn project(sources: &[(&str, &str)]) -> Analysis {
    let mut p = frontend_v2::lower_project(
        &sources
            .iter()
            .map(|(p, s)| (p.to_string(), s.to_string()))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    p.roots = vec!["case::run".into()];
    solver_v2::analyze(&p)
}
fn has_unknown(a: &Analysis) -> bool {
    a.obligations
        .iter()
        .any(|o| o.status == ObligationStatus::Unverified)
}
#[test]
fn cross_module_nested_fields_keep_identity() {
    let a = project(&[
        (
            "box.py",
            "class Box:\n    def __init__(self,f):\n        self.f=f\nclass Outer:\n    def __init__(self,box):\n        self.box=box\n    def close(self):\n        self.box.f.close()\n",
        ),
        (
            "case.py",
            "from box import Box, Outer\ndef run():\n    f=open('x')\n    b=Box(f)\n    outer=Outer(b)\n    outer.close()\n    return b.f.read()\n",
        ),
    ]);
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
    assert!(!has_unknown(&a), "{a:#?}");
}
#[test]
fn conflicting_field_types_cannot_reuse_resource_model() {
    for value in ["42", "None", "'text'"] {
        let a = analyze(&format!(
            "    f=open('x')\n    b=Box(f)\n    b.f={value}\n    return b.read()"
        ));
        assert!(has_unknown(&a), "{a:#?}");
    }
}
#[test]
fn descriptor_inheritance_and_dynamic_hooks_are_unverified() {
    for definition in [
        "class Box:\n    @property\n    def f(self):\n        return open('x')\n",
        "class Box(Base):\n    def __init__(self):\n        self.f=open('x')\n",
        "class Box:\n    def __getattribute__(self,name):\n        return open('x')\n",
    ] {
        let source = format!("{definition}def run():\n    b=Box()\n    return b.f.read()\n");
        let a = project(&[("case.py", &source)]);
        assert!(has_unknown(&a), "{a:#?}");
    }
}
#[test]
fn field_alias_returned_across_call_observes_close() {
    let a = project(&[(
        "case.py",
        "class Box:\n    def __init__(self,f):\n        self.f=f\n    def get(self):\n        return self.f\ndef run():\n    f=open('x')\n    b=Box(f)\n    alias=b.get()\n    f.close()\n    return alias.read()\n",
    )]);
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
    assert!(!has_unknown(&a), "{a:#?}");
}

#[test]
fn frozen_hash_variants_are_verifiable_with_proven_caller_path() {
    for (source, violation) in [
        (include_str!("corpus_v2/path_sha256/original.py"), false),
        (include_str!("corpus_v2/path_sha256/mutated_error.py"), true),
        (include_str!("corpus_v2/path_sha256/repaired.py"), false),
    ] {
        let source = format!("{source}\ndef run():\n    return _sha256(Path('x'))\n");
        let a = project(&[("case.py", &source)]);
        assert_eq!(
            a.findings.iter().any(|f| f.rule == "LIFE001"),
            violation,
            "{a:#?}"
        );
        assert!(!has_unknown(&a), "{a:#?}");
    }
}
