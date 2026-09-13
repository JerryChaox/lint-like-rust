use serde_json::Value;
use std::{fs, process::Command};
fn run(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_llr"))
        .args(args)
        .output()
        .unwrap()
}
fn write_project(root: &std::path::Path, operation: &str) {
    fs::create_dir_all(root).unwrap();
    let imports = if operation.contains("opaque(") {
        "from external import opaque\n"
    } else {
        ""
    };
    fs::write(root.join("main.py"),format!("from pathlib import Path\nfrom resources import release\n{imports}def run():\n    f=Path('x').open('rb')\n    alias=f\n{operation}\n")).unwrap();
    fs::write(
        root.join("resources.py"),
        "def release(f):\n    f.close()\n",
    )
    .unwrap();
}
#[test]
fn real_cli_crossfile_diagnostic_and_repair_comparison() {
    let d = tempfile::tempdir().unwrap();
    let root = d.path().join("project");
    write_project(&root, "    release(f)\n    alias.read()");
    let before = d.path().join("before.json");
    let after = d.path().join("after.json");
    let root = root.to_str().unwrap();
    let before = before.to_str().unwrap();
    let after = after.to_str().unwrap();
    let x = run(&[
        "analyze",
        root,
        "--entry",
        "main::run",
        "--format",
        "json",
        "--output",
        before,
    ]);
    assert_eq!(
        x.status.code(),
        Some(1),
        "{}",
        String::from_utf8_lossy(&x.stderr)
    );
    let report: Value = serde_json::from_str(&fs::read_to_string(before).unwrap()).unwrap();
    let evidence = report["obligations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|o| o["status"] == "violated")
        .unwrap()["evidence"]
        .clone();
    assert!(
        evidence["trace"]
            .as_array()
            .unwrap()
            .iter()
            .any(|t| t["location"]["path"] == "resources.py")
    );
    write_project(
        std::path::Path::new(root),
        "    alias.read()\n    release(f)",
    );
    assert_eq!(
        run(&[
            "analyze",
            root,
            "--entry",
            "main::run",
            "--format",
            "json",
            "--output",
            after
        ])
        .status
        .code(),
        Some(0)
    );
    let x = run(&["compare", before, after]);
    assert_eq!(x.status.code(), Some(0));
    let comparison: Value = serde_json::from_slice(&x.stdout).unwrap();
    assert!(
        comparison["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["kind"] == "resolved")
    );
    write_project(
        std::path::Path::new(root),
        "    opaque(f)\n    alias.read()",
    );
    assert_eq!(
        run(&[
            "analyze",
            root,
            "--entry",
            "main::run",
            "--format",
            "json",
            "--output",
            after
        ])
        .status
        .code(),
        Some(3)
    );
    let x = run(&["compare", before, after]);
    assert_eq!(x.status.code(), Some(3));
    let comparison: Value = serde_json::from_slice(&x.stdout).unwrap();
    assert!(
        comparison["issues"]
            .as_array()
            .unwrap()
            .iter()
            .any(|i| i["kind"] == "became_unverified")
    );
}
#[test]
fn empty_proof_inventory_is_not_success() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("main.py");
    fs::write(&p, "pass\n").unwrap();
    assert_eq!(
        run(&["analyze", p.to_str().unwrap()]).status.code(),
        Some(3)
    );
}
#[test]
fn invalid_entry_is_error_not_empty_success() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("main.py");
    fs::write(&p, "def f():\n    pass\n").unwrap();
    assert_eq!(
        run(&["analyze", p.to_str().unwrap(), "--entry", "missing"])
            .status
            .code(),
        Some(2)
    );
}

#[test]
fn lint_reports_life001_with_actionable_evidence() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("main.py");
    fs::write(
        &p,
        "def run():\n    f=open('x')\n    f.close()\n    f.read()\n",
    )
    .unwrap();
    let output = run(&["lint", p.to_str().unwrap(), "--format", "json"]);
    assert_eq!(output.status.code(), Some(1));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    let finding = &report["findings"][0];
    assert_eq!(finding["rule"], "LIFE001");
    assert_eq!(finding["location"]["line"], 4);
    assert!(finding["evidence_chain"].as_array().unwrap().len() >= 2);
    assert!(finding["suggested_fix"].as_str().unwrap().contains("close"));
}

#[test]
fn lint_treats_unknown_effect_as_benign_without_exit_three() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("main.py");
    fs::write(
        &p,
        "from external import opaque\ndef run():\n    f=open('x')\n    opaque(f)\n    f.read()\n",
    )
    .unwrap();
    let path = p.to_str().unwrap();
    assert_eq!(
        run(&["analyze", path, "--entry", "main::run"])
            .status
            .code(),
        Some(3)
    );
    let output = run(&["lint", path, "--entry", "main::run", "--format", "json"]);
    assert_eq!(output.status.code(), Some(0));
    let report: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(report["findings"].as_array().unwrap().is_empty());
}

#[test]
fn lint_preserves_analyze_violation() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("main.py");
    fs::write(
        &p,
        "def run():\n    f=open('x')\n    f.close()\n    f.write('x')\n",
    )
    .unwrap();
    let path = p.to_str().unwrap();
    assert_eq!(
        run(&["analyze", path, "--entry", "main::run"])
            .status
            .code(),
        Some(1)
    );
    assert_eq!(
        run(&["lint", path, "--entry", "main::run"]).status.code(),
        Some(1)
    );
}

#[test]
fn lint_does_not_promote_joined_possible_evidence_to_a_finding() {
    let d = tempfile::tempdir().unwrap();
    let p = d.path().join("main.py");
    fs::write(
        &p,
        "def run():\n    f=open('x')\n    try:\n        f.close()\n    except:\n        pass\n    f.read()\n",
    )
    .unwrap();
    let path = p.to_str().unwrap();
    assert_eq!(
        run(&["analyze", path, "--entry", "main::run"])
            .status
            .code(),
        Some(1)
    );
    assert_eq!(
        run(&["lint", path, "--entry", "main::run"]).status.code(),
        Some(0)
    );
}
