use lint_like_rust::{
    frontend_v2,
    solver_v2::{self, Analysis, Certainty, ObligationStatus},
};
fn analyze(source: &str) -> Analysis {
    let mut p = frontend_v2::lower_project(&[("case.py".into(), source.into())]).unwrap();
    p.roots = vec!["case::run".into()];
    solver_v2::analyze(&p)
}
fn unknown(a: &Analysis) -> bool {
    a.obligations
        .iter()
        .any(|o| o.status == ObligationStatus::Unverified)
}
#[test]
fn constructor_close_reaches_alias_and_ordered_repair() {
    for (body, bad) in [
        ("    Worker(f)\n    return alias.read()", true),
        (
            "    value=alias.read()\n    Worker(f)\n    return value",
            false,
        ),
    ] {
        let a = analyze(&format!(
            "class Worker:\n    def __init__(self,f):\n        f.close()\ndef run():\n    f=open('x')\n    alias=f\n{body}\n"
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
fn constructor_self_call_and_instance_result_are_preserved() {
    let a = analyze(
        "class Worker:\n    def __init__(self,f):\n        self.finish(f)\n    def finish(self,f):\n        f.close()\n    def read(self,f):\n        return f.read()\ndef run():\n    f=open('x')\n    w=Worker(f)\n    alias=w\n    return alias.read(f)\n",
    );
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
    assert!(!unknown(&a), "{a:#?}");
}
#[test]
fn constructor_branch_close_is_possible() {
    let a = analyze(
        "class Worker:\n    def __init__(self,f,flag):\n        if flag:\n            f.close()\ndef work(flag):\n    f=open('x')\n    Worker(f,flag)\n    return f.read()\ndef run():\n    return work(True)\n",
    );
    assert!(
        a.findings
            .iter()
            .any(|f| f.rule == "LIFE001" && f.certainty == Certainty::Possible),
        "{a:#?}"
    );
    assert!(!unknown(&a), "{a:#?}");
}
#[test]
fn constructor_raise_propagates_mutation_to_handler() {
    let a = analyze(
        "class Worker:\n    def __init__(self,f):\n        f.close()\n        raise ValueError()\ndef run():\n    f=open('x')\n    try:\n        Worker(f)\n    except Exception:\n        return f.read()\n",
    );
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
    assert!(!unknown(&a), "{a:#?}");
}
#[test]
fn constructor_return_none_is_legal_but_non_none_is_unverified() {
    for (ret, gap) in [("None", false), ("42", true)] {
        let a = analyze(&format!(
            "class Worker:\n    def __init__(self):\n        return {ret}\n    def read(self,f):\n        return f.read()\ndef run():\n    w=Worker()\n    with open('x') as f:\n        return w.read(f)\n"
        ));
        assert_eq!(unknown(&a), gap, "{a:#?}");
    }
}
#[test]
fn unknown_constructor_and_special_attribute_store_remain_gaps() {
    for body in ["external(f)", "self.__dict__=f"] {
        let a = analyze(&format!(
            "class Worker:\n    def __init__(self,f):\n        {body}\ndef run():\n    f=open('x')\n    Worker(f)\n    return f.read()\n"
        ));
        assert!(unknown(&a), "{a:#?}");
    }
}
#[test]
fn unsupported_constructor_bindings_remain_gaps() {
    for call in ["Worker()", "Worker(f,f)", "Worker(f=f)", "Worker(*values)"] {
        let a = analyze(&format!(
            "class Worker:\n    def __init__(self,f):\n        f.close()\ndef run(values):\n    f=open('x')\n    {call}\n    return f.read()\n"
        ));
        assert!(unknown(&a), "{a:#?}");
    }
}
#[test]
fn cross_module_constructor_effects_are_resolved() {
    let mut p=frontend_v2::lower_project(&[("worker.py".into(),"def finish(f):\n    f.close()\nclass Worker:\n    def __init__(self,f):\n        finish(f)\n".into()),("case.py".into(),"from worker import Worker\ndef run():\n    f=open('x')\n    Worker(f)\n    return f.read()\n".into())]).unwrap();
    p.roots = vec!["case::run".into()];
    let a = solver_v2::analyze(&p);
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
    assert!(!unknown(&a), "{a:#?}");
}
