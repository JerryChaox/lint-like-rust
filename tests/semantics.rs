//! Adversarial frontend-to-engine tests; these encode semantics, not IR layout.
use lint_like_rust::{
    config::{Config, Contract},
    engine,
    ir::Analysis,
    python,
};

fn check(source: &str, config: Config) -> Analysis {
    let units = python::lower_source("case.py", source, &config).expect("valid Python must parse");
    let mut result = Analysis::default();
    for unit in units {
        let analysis = engine::analyze(&unit);
        result.diagnostics.extend(analysis.diagnostics);
        result.coverage.extend(analysis.coverage);
    }
    result
}

fn default_check(source: &str) -> Analysis {
    check(source, Config::default())
}
fn clean(source: &str) {
    let result = default_check(source);
    assert!(
        result.diagnostics.is_empty(),
        "unexpected diagnostics: {result:#?}"
    );
}
fn detects(source: &str, rule: &str) {
    let result = default_check(source);
    assert!(
        result.diagnostics.iter().any(|d| d.rule == rule),
        "expected diagnostic: {source}\n{result:#?}"
    );
}
fn contract(name: &str, value: Contract) -> Config {
    let mut config = Config::default();
    config.contracts.insert(name.into(), value);
    config
}

#[test]
fn closed_file_read() {
    detects("f = open('x')\nf.close()\nf.read()\n", "LIFE001");
}
#[test]
fn alias_of_closed_file() {
    detects("f = open('x')\ng = f\nf.close()\ng.read()\n", "LIFE001");
}
#[test]
fn close_via_alias() {
    detects("f = open('x')\ng = f\ng.close()\nf.read()\n", "LIFE001");
}
#[test]
fn valid_file_use_before_close() {
    clean("f = open('x')\nf.read()\nf.close()\n");
}
#[test]
fn fresh_resource_reassignment() {
    clean("f = open('x')\nf.close()\nf = open('y')\nf.read()\n");
}
#[test]
fn resource_with_scope_exit() {
    detects("with open('x') as f:\n    f.read()\nf.read()\n", "LIFE001");
}
#[test]
fn resource_alias_survives_with_scope() {
    detects(
        "with open('x') as f:\n    alias = f\nalias.read()\n",
        "LIFE001",
    );
}
#[test]
fn resource_used_inside_with() {
    clean("with open('x') as f:\n    f.read()\n");
}
#[test]
fn resource_returned_from_with_is_closed() {
    detects(
        "def factory():\n    with open('x') as f:\n        return f\n",
        "LIFE002",
    );
}
#[test]
fn branch_may_close_resource() {
    detects(
        "f = open('x')\nif flag:\n    f.close()\nf.read()\n",
        "LIFE001",
    );
}
#[test]
fn both_branches_keep_resource_live() {
    clean("f = open('x')\nif flag:\n    f.read()\nelse:\n    f.read()\nf.read()\n");
}
#[test]
fn finally_closes_resource() {
    detects(
        "f = open('x')\ntry:\n    f.read()\nfinally:\n    f.close()\nf.read()\n",
        "LIFE001",
    );
}
#[test]
fn exception_handler_sees_prior_close() {
    detects(
        "f = open('x')\ntry:\n    f.close()\n    raise ValueError()\nexcept ValueError:\n    f.read()\n",
        "LIFE001",
    );
}
#[test]
fn unreachable_after_return_does_not_diagnose() {
    clean("def work():\n    f = open('x')\n    f.close()\n    return\n    f.read()\n");
}
#[test]
fn imported_open_resource() {
    detects(
        "from io import open as io_open\nf = io_open('x')\nf.close()\nf.read()\n",
        "LIFE001",
    );
}
#[test]
fn local_open_is_not_builtin() {
    clean("def open(path):\n    return []\nf = open('x')\nf.close()\nf.append(1)\n");
}
#[test]
fn ordinary_assignment_is_not_move() {
    clean("a = []\nb = a\na.append(1)\nb.append(2)\n");
}

#[test]
fn configured_move_invalidates_alias() {
    let result = check(
        "job = []\nalias = job\nsubmit(job)\nalias.append(1)\n",
        contract(
            "submit",
            Contract {
                consumes: vec![0],
                ..Contract::default()
            },
        ),
    );
    assert!(
        result.diagnostics.iter().any(|d| d.rule == "OWN001"),
        "{result:#?}"
    );
}
#[test]
fn configured_move_after_last_use_is_valid() {
    let result = check(
        "job = []\njob.append(1)\nsubmit(job)\n",
        contract(
            "submit",
            Contract {
                consumes: vec![0],
                ..Contract::default()
            },
        ),
    );
    assert!(result.diagnostics.is_empty(), "{result:#?}");
}
#[test]
fn configured_double_move() {
    let result = check(
        "job = []\nsubmit(job)\nsubmit(job)\n",
        contract(
            "submit",
            Contract {
                consumes: vec![0],
                ..Contract::default()
            },
        ),
    );
    assert!(
        result.diagnostics.iter().any(|d| d.rule == "OWN002"),
        "{result:#?}"
    );
}
#[test]
fn inferred_mutable_and_shared_arguments_conflict() {
    detects(
        "def change(dst, src):\n    dst.append(src[0])\nx = [1]\nchange(x, x)\n",
        "BOR001",
    );
}
#[test]
fn inferred_shared_arguments_can_alias() {
    clean("def inspect(left, right):\n    return left[0] + right[0]\nx = [1]\ny = inspect(x, x)\n");
}
#[test]
fn inferred_mutation_of_distinct_arguments_is_valid() {
    clean("def change(dst, src):\n    dst.append(src[0])\nx = []\ny = [1]\nchange(x, y)\n");
}
#[test]
fn dictionary_iteration_structural_mutation() {
    detects("d = {'x': 1}\nfor key in d:\n    d.pop(key)\n", "BOR002");
}
#[test]
fn dictionary_items_iteration_structural_mutation() {
    detects(
        "d = {'x': 1}\nfor key, value in d.items():\n    d.pop(key)\n",
        "BOR002",
    );
}
#[test]
fn dictionary_snapshot_iteration_is_safe() {
    clean("d = {'x': 1}\nfor key, value in list(d.items()):\n    d.pop(key)\n");
}
#[test]
fn dictionary_mutation_after_loop_is_safe() {
    clean("d = {'x': 1}\nfor key in d:\n    pass\nd.clear()\n");
}
#[test]
fn configured_shared_borrow_conflicts_until_last_use() {
    let result = check(
        "data = [1]\nview = borrow(data)\ndata.append(2)\nx = view[0]\n",
        contract(
            "borrow",
            Contract {
                returns_borrow: Some(0),
                ..Contract::default()
            },
        ),
    );
    assert!(
        result.diagnostics.iter().any(|d| d.rule == "BOR002"),
        "{result:#?}"
    );
}
#[test]
fn configured_shared_borrow_expires_after_last_use() {
    let result = check(
        "data = [1]\nview = borrow(data)\nx = view[0]\ndata.append(2)\n",
        contract(
            "borrow",
            Contract {
                returns_borrow: Some(0),
                ..Contract::default()
            },
        ),
    );
    assert!(result.diagnostics.is_empty(), "{result:#?}");
}
#[test]
fn configured_exclusive_borrow_conflicts_with_owner_read() {
    let result = check(
        "data = [1]\nview = borrow_mut(data)\nx = data[0]\ny = view[0]\n",
        contract(
            "borrow_mut",
            Contract {
                returns_mut_borrow: Some(0),
                ..Contract::default()
            },
        ),
    );
    assert!(
        result.diagnostics.iter().any(|d| d.rule == "BOR002"),
        "{result:#?}"
    );
}
#[test]
fn discarded_pure_return_is_detected() {
    detects("def pure(x):\n    return 42\npure(2)\n", "ERR001");
}
#[test]
fn retained_pure_return_is_valid() {
    clean("def pure(x):\n    return x + 1\ny = pure(2)\n");
}
#[test]
fn unresolved_call_is_visible() {
    let result = default_check("mystery()\n");
    assert!(
        !result.coverage.is_empty(),
        "unknown call must not appear verified: {result:#?}"
    );
}
#[test]
fn descriptor_ownership_is_modeled_or_explicitly_unknown() {
    let result = default_check("import os\nf = os.fdopen(fd)\nos.close(fd)\nf.read()\n");
    assert!(
        !result.coverage.is_empty() || !result.diagnostics.is_empty(),
        "unmodeled fd ownership must remain visible: {result:#?}"
    );
}

#[test]
fn untyped_operator_effect_is_unknown() {
    let result = default_check("def calculate(x):\n    return x + 1\ncalculate(value)\n");
    assert!(
        !result.coverage.is_empty(),
        "overloaded operators may have effects: {result:#?}"
    );
    assert!(
        !result.diagnostics.iter().any(|d| d.rule == "ERR001"),
        "purity was not proved: {result:#?}"
    );
}

#[test]
fn nested_open_does_not_shadow_module_builtin() {
    detects(
        "def wrapper():\n    def open(path):\n        return []\n    return open('x')\nf = open('x')\nf.close()\nf.read()\n",
        "LIFE001",
    );
}

#[test]
fn class_open_method_does_not_shadow_builtin() {
    detects(
        "class Factory:\n    def open(self, path):\n        return []\nf = open('x')\nf.close()\nf.read()\n",
        "LIFE001",
    );
}

#[test]
fn nested_function_summary_does_not_leak_into_module() {
    let result = default_check("def outer():\n    def inner():\n        return 42\ninner()\n");
    assert!(
        !result.coverage.is_empty(),
        "inner is not module-resolvable: {result:#?}"
    );
    assert!(
        !result.diagnostics.iter().any(|d| d.rule == "ERR001"),
        "{result:#?}"
    );
}

#[test]
fn mutable_borrow_allows_writes_through_handle() {
    let result = check(
        "data = [1]\nview = borrow_mut(data)\nview[0] = 2\nx = view[0]\ndata.append(3)\n",
        contract(
            "borrow_mut",
            Contract {
                returns_mut_borrow: Some(0),
                ..Contract::default()
            },
        ),
    );
    assert!(result.diagnostics.is_empty(), "{result:#?}");
}

#[test]
fn rebinding_with_target_does_not_close_replacement() {
    clean("with open('x') as f:\n    f = open('y')\nf.read()\n");
}

#[test]
fn rebinding_with_target_still_closes_original() {
    detects(
        "with open('x') as f:\n    original = f\n    f = open('y')\noriginal.read()\n",
        "LIFE001",
    );
}

#[test]
fn borrow_alias_keeps_loan_alive() {
    let result = check(
        "data = [1]\nview = borrow(data)\nalias = view\ndata.append(2)\nx = alias[0]\n",
        contract(
            "borrow",
            Contract {
                returns_borrow: Some(0),
                ..Contract::default()
            },
        ),
    );
    assert!(
        result.diagnostics.iter().any(|d| d.rule == "BOR002"),
        "{result:#?}"
    );
}

#[test]
fn rebinding_borrow_handle_releases_original_loan() {
    let result = check(
        "data = [1]\nview = borrow(data)\nview = [2]\ndata.append(3)\nx = view[0]\n",
        contract(
            "borrow",
            Contract {
                returns_borrow: Some(0),
                ..Contract::default()
            },
        ),
    );
    assert!(result.diagnostics.is_empty(), "{result:#?}");
}

#[test]
fn move_only_one_branch_reports_possible() {
    let result = check(
        "data = []\nif flag:\n    consume(data)\ndata.append(1)\n",
        contract(
            "consume",
            Contract {
                consumes: vec![0],
                ..Contract::default()
            },
        ),
    );
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.rule == "OWN001" && d.confidence == lint_like_rust::ir::Confidence::Possible),
        "{result:#?}"
    );
}

#[test]
fn finally_closes_resource_returned_from_try() {
    detects(
        "def factory():\n    f = open('x')\n    try:\n        return f\n    finally:\n        f.close()\n",
        "LIFE002",
    );
}
