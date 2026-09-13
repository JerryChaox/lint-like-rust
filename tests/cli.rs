use std::{fs, process::Command};
fn check(args: &[&str]) -> std::process::Output {
    Command::new(env!("CARGO_BIN_EXE_llr"))
        .args(args)
        .output()
        .unwrap()
}
#[test]
fn nonexistent_is_error_json() {
    let out = check(&[
        "check",
        "/a-path-that-does-not-exist-llr",
        "--format",
        "json",
    ]);
    assert_eq!(out.status.code(), Some(2));
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(!report["errors"].as_array().unwrap().is_empty());
}
#[test]
fn clean_and_invalid_source_have_distinct_statuses() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("sample.py");
    fs::write(&file, "x = 1\n").unwrap();
    let out = check(&["check", file.to_str().unwrap(), "--format", "json"]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["files"], 1);
    fs::write(&file, "def broken(:\n").unwrap();
    assert_eq!(
        check(&["check", file.to_str().unwrap()]).status.code(),
        Some(2)
    );
}
#[test]
fn invalid_rule_and_empty_scan_are_errors() {
    let dir = tempfile::tempdir().unwrap();
    assert_eq!(
        check(&["check", dir.path().to_str().unwrap()])
            .status
            .code(),
        Some(2)
    );
    let file = dir.path().join("sample.py");
    fs::write(&file, "x = 1\n").unwrap();
    assert_eq!(
        check(&["check", file.to_str().unwrap(), "--select", "FAKE001"])
            .status
            .code(),
        Some(2)
    );
}
#[test]
fn nearest_configuration_is_loaded_and_contracts_audited() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("pyproject.toml"),
        "[tool.llr]\nstrict = true\n[tool.llr.contracts.transfer]\nconsumes = [0]\n",
    )
    .unwrap();
    let nested = dir.path().join("nested");
    fs::create_dir(&nested).unwrap();
    let file = nested.join("sample.py");
    fs::write(&file, "x = 1\n").unwrap();
    let out = check(&["check", file.to_str().unwrap(), "--format", "json"]);
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["configurations"][0]["effective"]["strict"], true);
    assert_eq!(report["configurations"][0]["trusted_contracts"], true);
}
#[test]
fn exclusions_and_default_directory_skips_work() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(dir.path().join("good.py"), "x = 1\n").unwrap();
    fs::create_dir(dir.path().join(".venv")).unwrap();
    fs::write(dir.path().join(".venv/bad.py"), "def broken(:\n").unwrap();
    fs::write(dir.path().join("skip.py"), "def broken(:\n").unwrap();
    let out = check(&[
        "check",
        dir.path().to_str().unwrap(),
        "--exclude",
        "skip.py",
        "--format",
        "json",
    ]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["files"], 1);
}

#[test]
fn nested_glob_exclusion() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join("pkg/tests")).unwrap();
    fs::write(dir.path().join("pkg/tests/bad.py"), "def broken(:\n").unwrap();
    fs::write(dir.path().join("good.py"), "x = 1\n").unwrap();
    let out = check(&[
        "check",
        dir.path().to_str().unwrap(),
        "--exclude",
        "**/tests/**",
        "--format",
        "json",
    ]);
    assert_eq!(
        out.status.code(),
        Some(0),
        "{}",
        String::from_utf8_lossy(&out.stdout)
    );
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["files"], 1);
}

#[test]
fn scoped_suppression_requires_real_comment_and_is_audited() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("sample.py");
    fs::write(
        &file,
        "f = open('a')\nf.close()\nf.read() # llr: ignore[LIFE001]\n",
    )
    .unwrap();
    let out = check(&["check", file.to_str().unwrap(), "--format", "json"]);
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        report["suppressed"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["diagnostic"]["rule"] == "LIFE001")
    );
    fs::write(
        &file,
        "f = open('a')\nf.close()\nf.read(); x = '# llr: ignore[LIFE001]'\n",
    )
    .unwrap();
    let out = check(&["check", file.to_str().unwrap(), "--format", "json"]);
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(
        report["diagnostics"]
            .as_array()
            .unwrap()
            .iter()
            .any(|d| d["rule"] == "LIFE001")
    );
}

#[test]
fn strict_fails_for_unknown_behavior_without_hiding_coverage() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("sample.py");
    fs::write(&file, "unknown_external_function()\n").unwrap();
    let out = check(&[
        "check",
        file.to_str().unwrap(),
        "--strict",
        "--format",
        "json",
    ]);
    assert_eq!(out.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert!(!report["coverage"].as_array().unwrap().is_empty());
}

#[test]
fn explicit_config_and_output_are_supported() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("sample.py");
    fs::write(&file, "x = 1\n").unwrap();
    let config = dir.path().join("checker.toml");
    fs::write(&config, "strict = true\n").unwrap();
    let output = dir.path().join("result.json");
    let out = check(&[
        "check",
        file.to_str().unwrap(),
        "--config",
        config.to_str().unwrap(),
        "--format",
        "json",
        "--output",
        output.to_str().unwrap(),
    ]);
    assert_eq!(out.status.code(), Some(0));
    assert!(out.stdout.is_empty());
    let report: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(output).unwrap()).unwrap();
    assert_eq!(report["configurations"][0]["effective"]["strict"], true);
}

#[test]
fn unrelated_nested_pyproject_does_not_hide_parent_policy() {
    let dir = tempfile::tempdir().unwrap();
    fs::write(
        dir.path().join("pyproject.toml"),
        "[tool.llr]\nstrict = true\n",
    )
    .unwrap();
    let nested = dir.path().join("nested");
    fs::create_dir(&nested).unwrap();
    fs::write(
        nested.join("pyproject.toml"),
        "[project]\nname = 'nested-package'\n",
    )
    .unwrap();
    let file = nested.join("sample.py");
    fs::write(&file, "unknown_external_function()\n").unwrap();
    let out = check(&["check", file.to_str().unwrap(), "--format", "json"]);
    assert_eq!(out.status.code(), Some(1));
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["configurations"][0]["effective"]["strict"], true);
    fs::write(nested.join("pyproject.toml"), "[tool.llr]\n").unwrap();
    let out = check(&["check", file.to_str().unwrap(), "--format", "json"]);
    assert_eq!(out.status.code(), Some(0));
    let report: serde_json::Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(report["configurations"][0]["effective"]["strict"], false);
}
