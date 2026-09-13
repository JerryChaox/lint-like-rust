use lint_like_rust::{
    frontend_v2,
    solver_v2::{self, ObligationStatus},
};
fn analyze(source: &str) -> solver_v2::Analysis {
    solver_v2::analyze(&frontend_v2::lower_project(&[("case.py".into(), source.into())]).unwrap())
}
fn unknown(a: &solver_v2::Analysis) -> bool {
    a.obligations
        .iter()
        .any(|o| o.status == ObligationStatus::Unverified)
}
#[test]
fn stateless_methods_propagate_close_and_read_with_aliases() {
    for (body, bad) in [
        ("    worker.finish(f)\n    return alias.read(f)\n", true),
        (
            "    value=alias.read(f)\n    worker.finish(f)\n    return value\n",
            false,
        ),
    ] {
        let a = analyze(&format!(
            "class Worker:\n    def finish(self, f):\n        f.close()\n    def read(self, f):\n        return f.read()\ndef run():\n    worker=Worker()\n    alias=worker\n    f=open('x')\n{body}"
        ));
        assert_eq!(
            a.findings.iter().any(|f| f.rule == "LIFE001"),
            bad,
            "{a:#?}"
        );
        assert!(!unknown(&a), "{a:#?}");
    }
}
#[test]
fn method_read_error_and_repair_with_same_method_inventory() {
    for (body, bad) in [
        ("    f.close()\n    return reader.read(f)", true),
        ("    with f:\n        return reader.read(f)", false),
    ] {
        let a = analyze(&format!(
            "class Reader:\n    def read(self, f):\n        return f.read()\ndef run():\n    reader=Reader()\n    f=open('x')\n{body}\n"
        ));
        assert_eq!(
            a.findings.iter().any(|f| f.rule == "LIFE001"),
            bad,
            "{a:#?}"
        );
        assert!(!unknown(&a), "{a:#?}");
    }
}
#[test]
fn inheritance_allocation_hooks_and_properties_remain_unknown() {
    for class in [
        "class Reader(Base):\n    def read(self, f):\n        return f.read()\n",
        "class Reader:\n    def __new__(cls):\n        pass\n    def read(self, f):\n        return f.read()\n",
        "class Reader:\n    @property\n    def read(self):\n        return custom\n",
    ] {
        let a = analyze(&format!(
            "{class}\ndef run():\n    reader=Reader()\n    f=open('x')\n    return reader.read(f)\n"
        ));
        assert!(unknown(&a), "{a:#?}");
    }
}
#[test]
fn helper_mutation_before_acquisition_blocks_exact_dispatch() {
    let a = analyze(
        "class Reader:\n    def read(self, f):\n        return f.read()\ndef mutate():\n    external()\ndef run():\n    reader=Reader()\n    mutate()\n    f=open('x')\n    return reader.read(f)\n",
    );
    assert!(unknown(&a), "{a:#?}");
}
#[test]
fn instance_or_class_method_replacement_is_not_silently_ignored() {
    for target in ["reader.read", "Reader.read", "alias.read"] {
        let a = analyze(&format!(
            "class Reader:\n    def read(self, f):\n        return f.read()\ndef run(custom):\n    reader=Reader()\n    alias=reader\n    {target}=custom\n    f=open('x')\n    return reader.read(f)\n"
        ));
        assert!(unknown(&a), "{a:#?}");
    }
}
#[test]
fn cross_module_constructor_and_methods_resolve() {
    let p=frontend_v2::lower_project(&[("helpers.py".into(),"class Reader:\n    def read(self, f):\n        return f.read()\n".into()),("case.py".into(),"from helpers import Reader\ndef run():\n    r=Reader()\n    f=open('x')\n    f.close()\n    return r.read(f)\n".into())]).unwrap();
    let a = solver_v2::analyze(&p);
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
    assert!(!unknown(&a), "{a:#?}");
}

#[test]
fn self_method_chain_is_resolved_without_annotations() {
    let a = analyze(
        "class Reader:\n    def read(self, f):\n        return self.contents(f)\n    def contents(self, f):\n        return f.read()\ndef run():\n    r=Reader()\n    f=open('x')\n    f.close()\n    return r.read(f)\n",
    );
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
    assert!(!unknown(&a), "{a:#?}");
}
#[test]
fn duplicate_class_binding_and_unused_unknown_method_do_not_get_exact_dispatch() {
    for suffix in [
        "class Reader:\n    def other(self):\n        pass\n",
        "def unrelated(x):\n    external(x)\n",
    ] {
        let a = analyze(&format!(
            "class Reader:\n    def read(self, f):\n        return f.read()\n{suffix}def run():\n    r=Reader()\n    f=open('x')\n    return r.read(f)\n"
        ));
        assert!(unknown(&a), "{a:#?}");
    }
}
