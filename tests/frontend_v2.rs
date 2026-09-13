use lint_like_rust::{
    frontend_v2::lower_project,
    v2_ir::{Kind, Terminator},
};
fn lower(src: &str) -> lint_like_rust::v2_ir::Program {
    lower_project(&[("main.py".into(), src.into())]).unwrap()
}
fn kinds(p: &lint_like_rust::v2_ir::Program) -> Vec<&Kind> {
    p.functions
        .iter()
        .flat_map(|f| &f.blocks)
        .flat_map(|b| &b.operations)
        .map(|i| &i.kind)
        .collect()
}
#[test]
fn file_alias_lifetime() {
    let p = lower("f = open('x')\na = f\nf.close()\na.read()\n");
    assert!(kinds(&p).iter().any(|k| matches!(k, Kind::Acquire { .. })));
    assert!(kinds(&p).iter().any(|k| matches!(k, Kind::Close { .. })));
    assert!(kinds(&p).iter().any(|k| matches!(k, Kind::Read { .. })));
}
#[test]
fn path_alias_constructor() {
    let p = lower("from pathlib import Path as P\np = P('x')\nq = p\nf = q.open()\nf.close()\n");
    assert_eq!(
        kinds(&p)
            .iter()
            .filter(|k| matches!(k, Kind::Acquire { .. }))
            .count(),
        1
    );
    assert!(kinds(&p).iter().any(|k| matches!(k, Kind::Close { .. })));
}
#[test]
fn unknown_open_is_not_a_file() {
    let p = lower("def f(dialog):\n    x = dialog.open()\n    x.read()\n");
    assert!(
        !kinds(&p)
            .iter()
            .any(|k| matches!(k, Kind::Acquire { .. } | Kind::Read { .. }))
    );
    assert!(kinds(&p).iter().any(|k| matches!(k, Kind::Unknown { .. })));
}
#[test]
fn builtin_shadow() {
    let p = lower("def f(open):\n    return open('x')\n");
    assert!(!kinds(&p).iter().any(|k| matches!(k, Kind::Acquire { .. })));
}
#[test]
fn cross_module_identity_and_inference() {
    let p = lower_project(&[
        (
            "helper.py".into(),
            "def close_it(f):\n    f.close()\n".into(),
        ),
        (
            "main.py".into(),
            "from helper import close_it\nf = open('x')\nclose_it(f)\nf.read()\n".into(),
        ),
    ])
    .unwrap();
    assert!(
        kinds(&p)
            .iter()
            .any(|k| matches!(k,Kind::Call{callee,..} | Kind::Invoke{callee,..} if callee=="helper::close_it"))
    );
    assert!(kinds(&p).iter().any(|k| matches!(k, Kind::Close { .. })));
}
#[test]
fn relative_import() {
    let p = lower_project(&[
        (
            "pkg/helpers.py".into(),
            "def make():\n    return open('x')\n".into(),
        ),
        (
            "pkg/main.py".into(),
            "from .helpers import make\nf = make()\nf.read()\n".into(),
        ),
    ])
    .unwrap();
    assert!(kinds(&p).iter().any(
        |k| matches!(k,Kind::Call{callee,..} | Kind::Invoke{callee,..} if callee=="helpers::make")
    ));
    assert!(kinds(&p).iter().any(|k| matches!(k, Kind::Read { .. })));
}
#[test]
fn builtin_and_with_cleanup() {
    let p = lower("def read(p):\n    with open(p, 'rb') as f:\n        return f.read()\n");
    assert!(kinds(&p).iter().any(|k| matches!(k, Kind::Acquire { .. })));
    assert!(kinds(&p).iter().any(|k| matches!(k, Kind::Read { .. })));
    assert!(kinds(&p).iter().any(|k| matches!(k, Kind::Close { .. })));
    assert!(
        p.functions
            .iter()
            .flat_map(|f| &f.blocks)
            .any(|b| matches!(b.terminator, Terminator::Return { .. }))
    );
}
#[test]
fn branches_are_cfg() {
    let p = lower("f = open('x')\nif flag:\n    f.close()\nf.read()\n");
    assert!(
        p.functions
            .iter()
            .flat_map(|f| &f.blocks)
            .any(|b| matches!(b.terminator, Terminator::Branch { .. }))
    );
}
#[test]
fn parse_errors_not_silent() {
    assert!(lower_project(&[("bad.py".into(), "def :".into())]).is_err());
}
#[test]
fn loop_is_explicit_unknown() {
    let p = lower("for f in files:\n    f.close()\n");
    assert!(
        kinds(&p)
            .iter()
            .any(|k| matches!(k,Kind::Unknown{reason,..}if reason.contains("for_statement")))
    );
}
#[test]
fn same_names_do_not_collide() {
    let p = lower_project(&[
        ("a.py".into(), "def close(f):\n    f.close()\n".into()),
        ("b.py".into(), "def close(f):\n    return f\n".into()),
        (
            "main.py".into(),
            "from b import close\nf = open('x')\nclose(f)\n".into(),
        ),
    ])
    .unwrap();
    assert!(kinds(&p).iter().any(
        |k| matches!(k,Kind::Call{callee,..} | Kind::Invoke{callee,..} if callee=="b::close")
    ));
}
#[test]
fn module_shadow_invalidates_function_builtin_resolution() {
    let p = lower("open = custom\ndef make():\n    return open('x')\n");
    assert!(!kinds(&p).iter().any(|k| matches!(k, Kind::Acquire { .. })));
}
#[test]
fn conflicting_call_types_do_not_invent_file_methods() {
    let p = lower("def use(f):\n    f.close()\na = open('x')\nuse(a)\nuse(42)\n");
    assert!(!kinds(&p).iter().any(|k| matches!(k, Kind::Close { .. })));
}
#[test]
fn custom_opener_effect_is_visible() {
    let p = lower("a = open('x')\nf = open('y', opener=callback)\na.read()\n");
    assert!(kinds(&p).iter().any(|k|matches!(k,Kind::Unknown{reason,affected}if reason.contains("callback") && affected.is_empty())));
}
#[test]
fn unknown_context_does_not_borrow_other_calls_type() {
    let p = lower("def use(f):\n    f.close()\na = open('x')\nuse(a)\nuse(mystery)\n");
    assert!(!kinds(&p).iter().any(|k| matches!(k, Kind::Close { .. })));
}
#[test]
fn modeled_try_resource_error_preserves_violation() {
    let p = lower(
        "def read(path):\n    try:\n        with open(path, 'r', encoding='utf-8') as handle:\n            pass\n        return handle.read()\n    except Exception:\n        return ''\n",
    );
    let analysis = lint_like_rust::solver_v2::analyze(&p);
    assert!(analysis.findings.iter().any(|f| f.rule == "LIFE001"));
    assert!(
        analysis
            .obligations
            .iter()
            .all(|o| o.status != lint_like_rust::solver_v2::ObligationStatus::Unverified)
    );
}
#[test]
fn normal_with_read_is_not_an_error() {
    let p = lower(
        "def read(path):\n    try:\n        with open(path, 'r', encoding='utf-8') as handle:\n            return handle.read()\n    except Exception:\n        return ''\n",
    );
    let analysis = lint_like_rust::solver_v2::analyze(&p);
    assert!(analysis.findings.is_empty());
}
#[test]
fn read_anchor_survives_close_removal() {
    let a = lower("f = open('x')\nf.close()\nf.read()\n");
    let b = lower("f = open('x')\nf.read()\n");
    let anchor = |p: lint_like_rust::v2_ir::Program| {
        p.functions
            .into_iter()
            .flat_map(|f| f.blocks)
            .flat_map(|b| b.operations)
            .find(|o| matches!(o.kind, Kind::Read { .. }))
            .unwrap()
            .site
            .anchor
    };
    assert_eq!(anchor(a), anchor(b));
}
#[test]
fn called_helper_not_reanalyzed_as_unknown_entry() {
    let p = lower("def close_it(f):\n    f.close()\nf = open('x')\nclose_it(f)\n");
    assert!(!p.roots.contains(&"main::close_it".into()));
}
#[test]
fn imported_rebinding_does_not_resolve_stale_local_function() {
    let p=lower_project(&[("helper.py".into(),"def close_it(f):\n    pass\n".into()),("main.py".into(),"def close_it(f):\n    f.close()\nfrom helper import close_it\ndef main():\n    f = open('x')\n    close_it(f)\n    f.read()\nmain()\n".into())]).unwrap();
    assert!(!kinds(&p).iter().any(
        |k| matches!(k,Kind::Call{callee,..} | Kind::Invoke{callee,..} if callee=="main::close_it")
    ));
    assert!(lint_like_rust::solver_v2::analyze(&p).findings.is_empty());
}
#[test]
fn temporal_duplicate_definitions_are_unknown_not_last_definition_wins() {
    let p = lower(
        "def close_it(f):\n    f.close()\ndef main():\n    f = open('x')\n    close_it(f)\n    f.read()\nmain()\ndef close_it(f):\n    pass\n",
    );
    let ids: std::collections::BTreeSet<_> = p.functions.iter().map(|f| &f.id).collect();
    assert_eq!(ids.len(), p.functions.len());
    assert!(!kinds(&p).iter().any(
        |k| matches!(k,Kind::Call{callee,..} | Kind::Invoke{callee,..} if callee=="main::close_it")
    ));
    assert!(
        kinds(&p)
            .iter()
            .any(|k| matches!(k,Kind::Unknown{reason,..}if reason.contains("temporal")))
    );
}
#[test]
fn defaults_execute_at_definition_time() {
    let p = lower("f = open('x')\ndef helper(value=f.close()):\n    pass\nf.read()\n");
    assert!(
        lint_like_rust::solver_v2::analyze(&p)
            .findings
            .iter()
            .any(|f| f.rule == "LIFE001")
    );
}
#[test]
fn path_base_annotation_is_not_exact_runtime_dispatch() {
    let p = lower(
        "from pathlib import Path\ndef use(p: Path):\n    f = p.open()\n    f.close()\n    f.read()\n",
    );
    assert!(!kinds(&p).iter().any(|k| matches!(k, Kind::Acquire { .. })));
    assert!(lint_like_rust::solver_v2::analyze(&p).findings.is_empty());
    assert!(kinds(&p).iter().any(|k| matches!(k, Kind::Unknown { .. })));
}
#[test]
fn project_import_execution_invalidates_live_resource() {
    let p = lower_project(&[
        (
            "main.py".into(),
            "f = open('x')\nimport hook\nf.read()\n".into(),
        ),
        ("hook.py".into(), "from main import f\nf.close()\n".into()),
    ])
    .unwrap();
    let analysis = lint_like_rust::solver_v2::analyze(&p);
    assert!(analysis.obligations.iter().any(|o| o.operation == "read"
        && o.status == lint_like_rust::solver_v2::ObligationStatus::Unverified));
}
#[test]
fn explicit_entry_cannot_hide_executable_module_setup() {
    let mut p = lower(
        "from pathlib import Path\nsetup()\ndef run():\n    f = Path('x').open()\n    f.read()\n",
    );
    p.roots = vec!["main::run".into()];
    let analysis = lint_like_rust::solver_v2::analyze(&p);
    assert!(
        analysis
            .obligations
            .iter()
            .any(|o| o.reason.contains("module initialization"))
    );
    assert!(!analysis.obligations.iter().any(|o| o.operation == "read"
        && o.status == lint_like_rust::solver_v2::ObligationStatus::Verified));
}
#[test]
fn deferred_body_does_not_execute_at_call_time() {
    for declaration in [
        "async def close_it(f):\n    f.close()\n",
        "def close_it(f):\n    yield 1\n    f.close()\n",
    ] {
        let p = lower_project(&[
            ("helper.py".into(), declaration.into()),
            (
                "main.py".into(),
                "from helper import close_it\nf = open('x')\nclose_it(f)\nf.read()\n".into(),
            ),
        ])
        .unwrap();
        assert!(lint_like_rust::solver_v2::analyze(&p).findings.is_empty());
        assert!(!kinds(&p).iter().any(|k| matches!(k, Kind::Close { .. })));
    }
}
