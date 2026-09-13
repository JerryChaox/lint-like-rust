use lint_like_rust::{
    frontend_v2,
    solver_v2::{self, ObligationStatus},
};
fn analyze(s: &str) -> solver_v2::Analysis {
    solver_v2::analyze(&frontend_v2::lower_project(&[("case.py".into(), s.into())]).unwrap())
}
fn unknown(a: &solver_v2::Analysis) -> bool {
    a.obligations
        .iter()
        .any(|o| o.status == ObligationStatus::Unverified)
}
#[test]
fn module_constant_and_cross_function_json_read_error_repair() {
    for (body, bad) in [
        ("    f.close()\n    return decode(f)\n", true),
        ("    with f:\n        return decode(f)\n", false),
    ] {
        let s = format!(
            "import json\nFILE_NAME = 'settings.json'\ndef decode(stream):\n    return json.load(stream)\ndef run():\n    f = open(FILE_NAME)\n{body}"
        );
        let a = analyze(&s);
        assert_eq!(
            a.findings.iter().any(|f| f.rule == "LIFE001"),
            bad,
            "{a:#?}"
        );
        assert!(!unknown(&a), "{a:#?}");
    }
}
#[test]
fn default_json_failure_models_cleanup_and_detects_handler_read() {
    let a = analyze(
        "import json\ndef run():\n    f=open('x')\n    try:\n        with f:\n            json.load(f)\n    except BaseException:\n        f.read()\n",
    );
    assert!(
        a.obligations.iter().any(|o| o.operation == "close"),
        "{a:#?}"
    );
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
    assert!(!unknown(&a), "{a:#?}");
}

#[test]
fn literal_shadowing_never_restores_builtin_or_library_identity() {
    for binding in ["open = 0", "json = 0"] {
        let a = analyze(&format!(
            "import json\n{binding}\ndef run():\n    f=open('x')\n    return json.load(f)\n"
        ));
        assert!(unknown(&a), "{a:#?}");
    }
}
#[test]
fn effectful_initialization_and_fstring_stay_unknown() {
    for binding in [
        "__builtins__ = None",
        "x = configure()",
        "x = f'{configure()}'",
        "f'{configure()}'",
        "x: configure() = 1",
    ] {
        let a = analyze(&format!(
            "import json\n{binding}\ndef run():\n    f=open('x')\n    return json.load(f)\n"
        ));
        assert!(unknown(&a), "{a:#?}");
    }
}
#[test]
fn custom_decoder_and_unknown_file_protocol_are_not_default_model() {
    for source in [
        "import json\ndef run(hook):\n    f=open('x')\n    json.load(f, object_hook=hook)\n    f.read()\n",
        "import json\ndef run(f):\n    return json.load(f)\n",
    ] {
        assert!(unknown(&analyze(source)));
    }
}
#[test]
fn local_json_shadow_is_not_trusted_standard_library() {
    let p = frontend_v2::lower_project(&[
        (
            "case.py".into(),
            "import json\ndef run():\n    f=open('x')\n    json.load(f)\n    f.read()\n".into(),
        ),
        ("json.py".into(), "def load(f):\n    f.close()\n".into()),
    ])
    .unwrap();
    let a = solver_v2::analyze(&p);
    assert!(
        a.findings.iter().any(|f| f.rule == "LIFE001") || unknown(&a),
        "{a:#?}"
    );
}
