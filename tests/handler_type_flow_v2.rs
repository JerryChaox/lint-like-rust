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
fn cleanup_error_handler_error_repair_and_unknown_are_distinct() {
    for (handler, bad, gap) in [
        ("return alias.read()", true, false),
        ("return ''", false, false),
        ("return custom(alias)", false, true),
    ] {
        let a = analyze(&format!(
            "import json\ndef run():\n    f=open('config')\n    alias=f\n    try:\n        with f:\n            return json.load(f)\n    except BaseException:\n        {handler}\n"
        ));
        assert_eq!(
            a.findings.iter().any(|f| f.rule == "LIFE001"),
            bad,
            "{a:#?}"
        );
        assert_eq!(unknown(&a), gap, "{a:#?}");
    }
}
#[test]
fn rebinding_before_a_raise_does_not_keep_old_file_type() {
    for binding in ["f = other", "f, x = other", "del f", "import other as f"] {
        let a = analyze(&format!(
            "def run(other):\n    f=open('x')\n    try:\n        {binding}\n        raise ValueError()\n    except BaseException:\n        f.read()\n"
        ));
        assert!(unknown(&a), "{a:#?}");
    }
}
#[test]
fn an_earlier_handlers_bindings_do_not_contaminate_later_handlers() {
    let a = analyze(
        "def run(other):\n    f=open('x')\n    f.close()\n    try:\n        raise KeyboardInterrupt()\n    except Exception:\n        f=other\n    except BaseException:\n        f.read()\n",
    );
    assert!(unknown(&a), "{a:#?}");
}
#[test]
fn context_alias_rebinding_and_unknown_callback_remain_unknown() {
    for body in [
        "with other as f:\n            raise ValueError()",
        "custom(f)\n        raise ValueError()",
    ] {
        let a = analyze(&format!(
            "def run(other):\n    f=open('x')\n    try:\n        {body}\n    except BaseException:\n        f.read()\n"
        ));
        assert!(unknown(&a), "{a:#?}");
    }
}
#[test]
fn stable_file_type_after_join_does_not_erase_closed_state() {
    let a = analyze(
        "def run():\n    f=open('x')\n    try:\n        with f:\n            f.read()\n    except BaseException:\n        pass\n    f.read()\n",
    );
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
    assert!(!unknown(&a), "{a:#?}");
}
