use lint_like_rust::type_evidence_v2::{Bundle, sha256};
use serde_json::{Value, json};
fn fixture() -> (String, Value) {
    let source = "中文😀; p.open()\r\n".to_string();
    let range = json!({"start":{"line":0,"character":6},"end":{"line":0,"character":12}});
    let bundle = json!({"schema_version":1,"facts":[{"path":"case.py","normalized":{"status":"candidates","candidates":[{"name":"open","uri":"file:///pathlib.pyi","range_utf16":range}],"reasons":[],"dispatch":"nominal_candidates_only","span":{"start":12,"end":18},"binding":{"snapshot":5,"document_sha256":sha256(source.as_bytes()),"provider_sha256":"provider","stubs_sha256":"stubs","configuration_sha256":"config","uri":"file:///case.py","protocol":"0.4.1","query_range":range}}}]});
    (source, bundle)
}
fn valid(source: String, value: Value) -> bool {
    serde_json::from_value::<Bundle>(value)
        .is_ok_and(|b| b.validate(&[("case.py".into(), source)]).is_ok())
}
#[test]
fn unicode_snapshot_and_normalized_candidate_are_accepted() {
    let (s, v) = fixture();
    assert!(valid(s, v));
}
#[test]
fn source_edit_range_drift_and_exact_claim_are_rejected() {
    let (s, v) = fixture();
    assert!(!valid(s.clone() + "# edited", v.clone()));
    for (pointer, replacement) in [
        ("/facts/0/normalized/span/start", json!(10)),
        ("/facts/0/normalized/dispatch", json!("exact")),
        (
            "/facts/0/normalized/binding/query_range/start/character",
            json!(3),
        ),
        ("/facts/0/normalized/binding/provider_sha256", json!("")),
        ("/facts/0/normalized/binding/snapshot", Value::Null),
        ("/facts/0/path", json!("other.py")),
    ] {
        let mut changed = v.clone();
        *changed.pointer_mut(pointer).unwrap() = replacement;
        assert!(!valid(s.clone(), changed), "{pointer}");
    }
}
#[test]
fn partial_candidates_remain_unknown_and_cannot_claim_complete() {
    let (s, mut v) = fixture();
    v["facts"][0]["normalized"]["reasons"] = json!(["unresolved_type_reference"]);
    assert!(!valid(s.clone(), v.clone()));
    v["facts"][0]["normalized"]["status"] = json!("unknown");
    assert!(valid(s, v));
}

#[test]
fn cli_attaches_nominal_evidence_without_clearing_unknown_and_rejects_stale() {
    use std::{fs, process::Command};
    let dir = tempfile::tempdir().unwrap();
    let source = "def run(x):\n    return x.read()\n";
    let (_, mut v) = fixture();
    let n = &mut v["facts"][0]["normalized"];
    n["binding"]["document_sha256"] = json!(sha256(source.as_bytes()));
    n["binding"]["query_range"] =
        json!({"start":{"line":1,"character":11},"end":{"line":1,"character":17}});
    n["span"] = json!({"start":23,"end":29});
    let file = dir.path().join("case.py");
    let facts = dir.path().join("facts.json");
    fs::write(&file, source).unwrap();
    fs::write(&facts, serde_json::to_vec(&v).unwrap()).unwrap();
    let run = || {
        Command::new(env!("CARGO_BIN_EXE_llr"))
            .arg("analyze")
            .arg(&file)
            .args([
                "--entry",
                "case::run",
                "--format",
                "json",
                "--type-evidence",
            ])
            .arg(&facts)
            .output()
            .unwrap()
    };
    let result = run();
    assert_eq!(
        result.status.code(),
        Some(3),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let report: Value = serde_json::from_slice(&result.stdout).unwrap();
    assert!(report["nominal_type_evidence"]["facts"].is_array());
    assert!(
        report["obligations"]
            .as_array()
            .unwrap()
            .iter()
            .any(|o| o["status"] == "unverified")
    );
    fs::write(&file, format!("{source}# changed\n")).unwrap();
    assert_eq!(run().status.code(), Some(2));
}

fn declaration_fixture() -> (Vec<(String, String)>, Value) {
    let (_, mut bundle) = fixture();
    let source = "p.open()\n";
    let helper = "def open(f):\n    f.close()\n";
    let n = &mut bundle["facts"][0]["normalized"];
    n["binding"]["document_sha256"] = json!(sha256(source.as_bytes()));
    n["binding"]["query_range"] =
        json!({"start":{"line":0,"character":0},"end":{"line":0,"character":6}});
    n["span"] = json!({"start":0,"end":6});
    n["candidates"][0]["uri"] = json!("file:///helper.py");
    n["candidates"][0]["range_utf16"] =
        json!({"start":{"line":0,"character":4},"end":{"line":0,"character":8}});
    bundle["documents"] = json!([{"uri":"file:///helper.py","path":"helper.py","document_sha256":sha256(helper.as_bytes())}]);
    (
        vec![
            ("case.py".into(), source.into()),
            ("helper.py".into(), helper.into()),
            ("other.py".into(), helper.into()),
        ],
        bundle,
    )
}
#[test]
fn declaration_mapping_uses_bound_document_and_ast_span_not_name() {
    let (s, v) = declaration_fixture();
    let b: Bundle = serde_json::from_value(v).unwrap();
    let mapped = b.map_declarations(&s).unwrap();
    assert_eq!(mapped[0].symbol.as_deref(), Some("helper::open"));
    assert_eq!(mapped[0].dispatch, "nominal_candidates_only");
}
#[test]
fn stale_target_and_duplicate_uri_bindings_are_rejected() {
    let (mut s, v) = declaration_fixture();
    s[1].1.push_str("# edit");
    let b: Bundle = serde_json::from_value(v.clone()).unwrap();
    assert!(b.map_declarations(&s).is_err());
    let (s, mut v) = declaration_fixture();
    let doc = v["documents"][0].clone();
    v["documents"].as_array_mut().unwrap().push(doc);
    assert!(
        serde_json::from_value::<Bundle>(v)
            .unwrap()
            .map_declarations(&s)
            .is_err()
    );
}
#[test]
fn unbound_wrong_span_and_decorated_candidates_remain_unmapped() {
    let (s, v) = declaration_fixture();
    for (pointer, value) in [
        (
            "/facts/0/normalized/candidates/0/uri",
            json!("file:///unscanned.pyi"),
        ),
        (
            "/facts/0/normalized/candidates/0/range_utf16/start/character",
            json!(5),
        ),
    ] {
        let mut changed = v.clone();
        *changed.pointer_mut(pointer).unwrap() = value;
        assert!(
            serde_json::from_value::<Bundle>(changed)
                .unwrap()
                .map_declarations(&s)
                .unwrap()[0]
                .symbol
                .is_none()
        );
    }
    let (mut s, mut v) = declaration_fixture();
    s[1].1 = "@decorate\ndef open(f):\n    f.close()\n".into();
    v["documents"][0]["document_sha256"] = json!(sha256(s[1].1.as_bytes()));
    v["facts"][0]["normalized"]["candidates"][0]["range_utf16"] =
        json!({"start":{"line":1,"character":4},"end":{"line":1,"character":8}});
    assert!(
        serde_json::from_value::<Bundle>(v)
            .unwrap()
            .map_declarations(&s)
            .unwrap()[0]
            .symbol
            .is_none()
    );
}

#[test]
fn duplicate_runtime_names_are_not_unique_declarations() {
    let (mut s, mut v) = declaration_fixture();
    s[1].1.push_str("def open(f):\n    return f\n");
    v["documents"][0]["document_sha256"] = json!(sha256(s[1].1.as_bytes()));
    assert!(
        serde_json::from_value::<Bundle>(v)
            .unwrap()
            .map_declarations(&s)
            .unwrap()[0]
            .symbol
            .is_none()
    );
}
