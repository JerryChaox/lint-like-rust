use lint_like_rust::{
    frontend_v2,
    solver_v2::{self, Analysis, ObligationStatus},
};
fn analyze(source: &str) -> Analysis {
    let mut p = frontend_v2::lower_project(&[("case.py".into(), source.into())]).unwrap();
    p.roots = vec!["case::run".into()];
    solver_v2::analyze(&p)
}
fn no_gaps(a: &Analysis) {
    assert!(
        !a.obligations
            .iter()
            .any(|o| o.status == ObligationStatus::Unverified),
        "{a:#?}"
    );
}
fn has_gap(a: &Analysis) {
    assert!(
        a.obligations
            .iter()
            .any(|o| o.status == ObligationStatus::Unverified),
        "{a:#?}"
    );
}
fn caller() -> &'static str {
    "\ndef run():\n    date='2026-09-13'\n    record={'ts':'2026-09-13T12:00:00Z','decision':'sleep_window_wait','nested':[1,True,None,{'key':'value'}]}\n    _append_brain_log(Path('logs'),date,record)\n"
}
#[test]
fn unchanged_log_bodies_work_with_proven_caller_data() {
    for (body, violates) in [
        (
            include_str!("corpus_v2/append_brain_log/original.py"),
            false,
        ),
        (
            include_str!("corpus_v2/append_brain_log/mutated_error.py"),
            true,
        ),
        (
            include_str!("corpus_v2/append_brain_log/repaired.py"),
            false,
        ),
    ] {
        let a = analyze(&(body.to_owned() + caller()));
        no_gaps(&a);
        assert_eq!(
            a.findings.iter().any(|f| f.rule == "LIFE001"),
            violates,
            "{a:#?}"
        );
    }
}
#[test]
fn nested_data_from_function_returns_is_not_just_an_annotation() {
    let a = analyze(
        "import json\ndef record() -> dict:\n    return {'nested':[1,{'text':'v'}]}\ndef serialize(value: dict) -> str:\n    return json.dumps(value,ensure_ascii=False)\ndef run():\n    text=serialize(record())\n    return text+'\\n'\n",
    );
    no_gaps(&a);
    let a = analyze(
        "import json\ndef serialize(value:dict):\n    return json.dumps(value)\ndef run(value:dict):\n    return serialize(value)\n",
    );
    has_gap(&a);
}
#[test]
fn decoder_output_is_plain_but_custom_hooks_are_not() {
    no_gaps(&analyze(
        "import json\ndef run():\n    data=json.loads('{\"key\":[1,2]}')\n    return json.dumps(data)\n",
    ));
    has_gap(&analyze(
        "import json\ndef run():\n    data=json.loads('{}',object_hook=external)\n    return json.dumps(data)\n",
    ));
    no_gaps(&analyze(
        "import json\ndef run():\n    with open('data.json') as f:\n        data=json.load(f)\n    return json.dumps(data)\n",
    ));
}
#[test]
fn unknown_nested_value_alias_mutation_and_encoder_are_not_pure() {
    for source in [
        "import json\ndef run(value):\n    return json.dumps({'child':[value]})\n",
        "import json\ndef run():\n    data={'v':1}\n    alias=data\n    alias['v']=external()\n    return json.dumps(data)\n",
        "import json\ndef run():\n    data={'v':1}\n    external(data)\n    return json.dumps(data)\n",
        "import json\ndef run():\n    return json.dumps({'v':1},default=external)\n",
        "import json\ndef run():\n    json.dumps=external\n    return json.dumps({'v':1})\n",
    ] {
        has_gap(&analyze(source));
    }
}
#[test]
fn formatting_requires_proven_values_and_supported_conversion() {
    no_gaps(&analyze(
        "def run():\n    name='today'\n    return f'{name}.ndjson'\n",
    ));
    has_gap(&analyze(
        "def run(name:str):\n    return f'{name}.ndjson'\n",
    ));
    has_gap(&analyze(
        "def run():\n    name='today'\n    return f'{name:>8}'\n",
    ));
}
#[test]
fn conditional_truth_callback_invalidates_data_and_resource_proof() {
    has_gap(&analyze(
        "import json\ndef run(flag):\n    data={'v':1}\n    with open('x','w') as f:\n        if flag:\n            pass\n        f.write(json.dumps(data))\n",
    ));
}
#[test]
fn serialization_exception_handler_preserves_resource_effects() {
    let a = analyze(
        "import json\ndef run():\n    f=open('x','w')\n    try:\n        text=json.dumps({'v':[1,2]})\n    except Exception:\n        f.close()\n    f.write('x')\n",
    );
    no_gaps(&a);
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
}
#[test]
fn selected_cli_entry_cannot_inherit_unselected_callers_arguments() {
    use std::process::Command;
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("case.py");
    std::fs::write(&path,"import json\ndef encode(record):\n    return json.dumps(record)\ndef caller():\n    return encode({'v':1})\n").unwrap();
    let default = Command::new(env!("CARGO_BIN_EXE_llr"))
        .args(["analyze", path.to_str().unwrap(), "--format", "json"])
        .output()
        .unwrap();
    assert_eq!(
        default.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&default.stdout)
    );
    let selected = Command::new(env!("CARGO_BIN_EXE_llr"))
        .args([
            "analyze",
            path.to_str().unwrap(),
            "--entry",
            "case::encode",
            "--format",
            "json",
        ])
        .output()
        .unwrap();
    assert_eq!(
        selected.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&selected.stdout)
    );
}
