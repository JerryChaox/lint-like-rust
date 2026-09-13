use lint_like_rust::entry_contracts_v2::{Bundle, Entry, Parameter, ParameterKind};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::process::Command;
fn bundle(source: &str, symbol: &str, param: &str) -> Bundle {
    Bundle {
        schema_version: 1,
        entries: vec![Entry {
            path: "case.py".into(),
            source_sha256: format!("{:x}", Sha256::digest(source)),
            symbol: symbol.into(),
            parameters: vec![Parameter {
                name: param.into(),
                kind: ParameterKind::ExactStdlibPath,
            }],
        }],
    }
}
fn run(source: &str, contract: Option<Value>, format: &str) -> (i32, String) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("case.py");
    std::fs::write(&path, source).unwrap();
    let mut command = Command::new(env!("CARGO_BIN_EXE_llr"));
    command.args(["analyze", path.to_str().unwrap(), "--format", format]);
    if let Some(value) = contract {
        let p = dir.path().join("entry.json");
        std::fs::write(&p, serde_json::to_vec(&value).unwrap()).unwrap();
        command.arg("--entry-contract").arg(p);
    }
    let result = command.output().unwrap();
    (
        result.status.code().unwrap(),
        String::from_utf8(result.stdout).unwrap() + &String::from_utf8(result.stderr).unwrap(),
    )
}
const ORIGINAL: &str = include_str!("corpus_v2/path_sha256/original.py");
const MUTATED: &str = include_str!("corpus_v2/path_sha256/mutated_error.py");
const REPAIRED: &str = include_str!("corpus_v2/path_sha256/repaired.py");
fn contract(source: &str) -> Value {
    serde_json::to_value(bundle(source, "case::_sha256", "path")).unwrap()
}
#[test]
fn unchanged_hash_variants_meet_declared_scope_without_changing_default() {
    for (source, code) in [(ORIGINAL, 0), (MUTATED, 1), (REPAIRED, 0)] {
        let (actual, output) = run(source, Some(contract(source)), "json");
        assert_eq!(actual, code, "{output}");
        let report: Value = serde_json::from_str(&output).unwrap();
        assert_eq!(
            report["verification_basis"],
            "conditional_on_explicit_caller_assumptions"
        );
        assert!(report["gaps"].as_array().unwrap().is_empty(), "{report}");
        assert!(report["obligations"].as_array().unwrap().iter().all(|o| {
            o["assumptions"].as_array().unwrap().iter().any(|a| {
                a.as_str()
                    .unwrap()
                    .contains("not inferred or runtime-checked")
            })
        }));
        assert_eq!(run(source, None, "json").0, 3);
    }
}
#[test]
fn text_report_discloses_caller_assumption() {
    let (code, output) = run(ORIGINAL, Some(contract(ORIGINAL)), "text");
    assert_eq!(code, 0);
    assert!(
        output.contains("Caller-supplied assumption (not inferred or runtime-checked)"),
        "{output}"
    );
}
#[test]
fn stale_snapshot_unknown_parameter_symbol_and_duplicate_are_rejected() {
    let good = contract(ORIGINAL);
    let mut cases = Vec::new();
    let mut x = good.clone();
    x["entries"][0]["source_sha256"] = json!("0".repeat(64));
    cases.push(x);
    let mut x = good.clone();
    x["entries"][0]["parameters"][0]["name"] = json!("other");
    cases.push(x);
    let mut x = good.clone();
    x["entries"][0]["symbol"] = json!("case::missing");
    cases.push(x);
    let mut x = good.clone();
    x["entries"][0]["path"] = json!("other.py");
    cases.push(x);
    let mut x = good.clone();
    x["entries"][0]["parameters"]
        .as_array_mut()
        .unwrap()
        .push(good["entries"][0]["parameters"][0].clone());
    cases.push(x);
    let mut x = good.clone();
    x["entries"]
        .as_array_mut()
        .unwrap()
        .push(good["entries"][0].clone());
    cases.push(x);
    let mut x = good.clone();
    x["schema_version"] = json!(2);
    cases.push(x);
    let mut x = good.clone();
    x["entries"] = json!([]);
    cases.push(x);
    let mut x = good.clone();
    x["entries"][0]["parameters"][0]["kind"] = json!("nominal_path");
    cases.push(x);
    let mut x = good.clone();
    x["trust_annotations"] = json!(true);
    cases.push(x);
    for value in cases {
        let (code, out) = run(ORIGINAL, Some(value), "json");
        assert_eq!(code, 2, "{out}");
    }
}
#[test]
fn entry_assumptions_cannot_leak_into_internal_callers() {
    let source = ORIGINAL.to_owned() + "\ndef other(x):\n    return _sha256(x)\n";
    let (code, out) = run(&source, Some(contract(&source)), "json");
    assert_eq!(code, 2, "{out}");
    let error = bundle(&source, "case::_sha256", "path")
        .lower(
            &[("case.py".into(), source.clone())],
            &["case::_sha256".into()],
        )
        .unwrap_err();
    assert!(error.contains("internally called"), "{error}");
    // An unresolved callable alias cannot borrow the entry assertion: its
    // caller remains unverified, while the separately selected root is conditional.
    let source =
        ORIGINAL.to_owned() + "\ndef other(x):\n    target=_sha256\n    return target(x)\n";
    let (code, out) = run(&source, Some(contract(&source)), "json");
    assert_eq!(code, 3, "{out}");
    let report: Value = serde_json::from_str(&out).unwrap();
    assert!(
        report["gaps"]
            .as_array()
            .unwrap()
            .iter()
            .any(|g| g["scope"]["symbol"] == "case::other")
    );
}
#[test]
fn shadowing_unknown_callbacks_and_branches_are_not_trusted_by_contract() {
    for source in [
        "from pathlib import Path\ndef run(path):\n    external(path)\n    with path.open() as f:\n        return f.read()\n",
        "from pathlib import Path\ndef run(path):\n    Path.open=external\n    with path.open() as f:\n        return f.read()\n",
        "from pathlib import Path\ndef run(path, flag):\n    if flag:\n        path=external()\n    with path.open() as f:\n        return f.read()\n",
    ] {
        let (code, out) = run(
            source,
            Some(serde_json::to_value(bundle(source, "case::run", "path")).unwrap()),
            "json",
        );
        assert_eq!(code, 3, "{out}");
    }
}
#[test]
fn selected_root_and_declaration_identity_are_checked() {
    let source = "def run(path):\n    pass\ndef other():\n    pass\n";
    let b = bundle(source, "case::run", "path");
    assert!(
        b.lower(
            &[("case.py".into(), source.into())],
            &["case::other".into()]
        )
        .is_err()
    );
    let redefined = "def run(path):\n    pass\ndef run(path):\n    pass\n";
    assert!(
        bundle(redefined, "case::run", "path")
            .lower(&[("case.py".into(), redefined.into())], &[])
            .is_err()
    );
}
#[test]
fn fingerprint_ignores_admission_hash_but_preserves_contract_semantics() {
    let a = bundle(ORIGINAL, "case::_sha256", "path");
    let b = bundle(MUTATED, "case::_sha256", "path");
    assert_eq!(a.fingerprint(), b.fingerprint());
    let mut c = a.clone();
    c.entries[0].parameters[0].name = "other".into();
    assert_ne!(a.fingerprint(), c.fingerprint());
}
#[test]
fn comparable_contract_repair_resolves_and_context_changes_do_not() {
    use lint_like_rust::diagnostics_v2::{self as d, Report};
    let make = |source, with_contract: bool| {
        let (_, text) = run(source, with_contract.then(|| contract(source)), "json");
        serde_json::from_str::<Report>(&text).unwrap()
    };
    let before = make(MUTATED, true);
    let after = make(REPAIRED, true);
    let default = make(REPAIRED, false);
    assert!(!d::compare_reports(&before, &after).issues.is_empty());
    assert!(
        d::compare_reports(&before, &after)
            .issues
            .iter()
            .all(|i| i.kind == d::ChangeKind::Resolved)
    );
    assert!(
        !d::compare_reports(&before, &default)
            .issues
            .iter()
            .any(|i| i.kind == d::ChangeKind::Resolved)
    );
}
