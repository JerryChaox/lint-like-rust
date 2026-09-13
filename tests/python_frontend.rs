use lint_like_rust::{
    config::{Config, Contract},
    engine,
    python::lower_source,
};
fn check(s: &str) -> lint_like_rust::ir::Analysis {
    let mut a = lint_like_rust::ir::Analysis::default();
    for u in lower_source("test.py", s, &Config::default()).unwrap() {
        let r = engine::analyze(&u);
        a.diagnostics.extend(r.diagnostics);
        a.coverage.extend(r.coverage);
    }
    a
}
#[test]
fn malformed_syntax_rejected() {
    assert!(lower_source("bad.py", "def x(:\n", &Config::default()).is_err());
}
#[test]
fn missing_syntax_rejected() {
    assert!(lower_source("bad.py", "x =\n", &Config::default()).is_err());
}
#[test]
fn python311_exception_groups() {
    assert!(
        lower_source(
            "t.py",
            "try:\n    pass\nexcept* ValueError:\n    pass\n",
            &Config::default()
        )
        .is_ok()
    );
}
#[test]
fn modern_type_params_parse() {
    assert!(
        lower_source(
            "t.py",
            "def f[T](x: T) -> T:\n    return x\n",
            &Config::default()
        )
        .is_ok()
    );
}
#[test]
fn empty_module() {
    assert_eq!(
        lower_source("t.py", "", &Config::default()).unwrap().len(),
        1
    );
}
#[test]
fn unknown_operator_visible() {
    assert!(!check("a = x + y\n").coverage.is_empty());
}
#[test]
fn unknown_lambda_visible() {
    assert!(!check("a = lambda x: x\n").coverage.is_empty());
}
#[test]
fn unknown_comprehension_visible() {
    assert!(!check("a = [x for x in data]\n").coverage.is_empty());
}
#[test]
fn unknown_attribute_visible() {
    assert!(!check("a = response.content\n").coverage.is_empty());
}
#[test]
fn async_client_no_fake_resource_borrow() {
    let a = check(
        "async def f():\n    async with httpx.AsyncClient() as client:\n        response = await client.get('url')\n    return response.content\n",
    );
    assert!(a.diagnostics.is_empty());
    assert!(!a.coverage.is_empty());
}
#[test]
fn builtin_module_alias() {
    assert!(
        !check("import builtins as b\nf=b.open('x')\nf.close()\nf.read()\n")
            .diagnostics
            .is_empty()
    );
}
#[test]
fn io_module_alias() {
    assert!(
        !check("import io as i\nf=i.open('x')\nf.close()\nf.read()\n")
            .diagnostics
            .is_empty()
    );
}
#[test]
fn overwritten_import_unknown() {
    let a = check("from io import open as op\nop = factory\nf=op('x')\nf.close()\nf.read()\n");
    assert!(a.diagnostics.is_empty());
    assert!(!a.coverage.is_empty());
}
#[test]
fn parameter_shadow_open() {
    let a = check("def f(open):\n    x=open('x')\n    x.close()\n    x.read()\n");
    assert!(a.diagnostics.is_empty());
}
#[test]
fn local_assignment_shadow_open() {
    let a = check("def f():\n    x=open('x')\n    open=factory\n    x.close()\n    x.read()\n");
    assert!(a.diagnostics.is_empty());
}
#[test]
fn arbitrary_append_not_proven_mutator() {
    let a = check("def change(a,b):\n    a.append(b[0])\nx=factory()\nchange(x,x)\n");
    assert!(a.diagnostics.is_empty());
    assert!(
        a.coverage
            .iter()
            .any(|g| g.reason.contains("known builtin collection"))
    );
}
#[test]
fn unknown_parameter_call_not_readonly() {
    let a = check("def f(a):\n    unknown(a)\nf([])\n");
    assert!(
        a.coverage
            .iter()
            .any(|g| g.reason.contains("Incomplete inferred"))
    );
}
#[test]
fn branch_kinds_do_not_leak() {
    let a = check("if flag:\n    x=[]\nelse:\n    x=factory()\nx.append(1)\n");
    assert!(
        a.coverage
            .iter()
            .any(|g| g.reason.contains("Unresolved call: x.append"))
    );
}
#[test]
fn named_view_copy_preserves_live_view() {
    assert!(
        !check("d={}\nv=d.items()\ns=list(v)\nd.clear()\nprint(v)\n")
            .diagnostics
            .is_empty()
    );
}
#[test]
fn anonymous_view_copy_releases_borrow() {
    assert!(
        check("d={}\ns=list(d.items())\nd.clear()\nprint(s)\n")
            .diagnostics
            .is_empty()
    );
}
#[test]
fn keyword_contract_not_misbound() {
    let mut c = Config::default();
    c.contracts.insert(
        "consume".into(),
        Contract {
            consumes: vec![0],
            ..Default::default()
        },
    );
    let units = lower_source("t.py", "x=[]\nconsume(other=x)\nprint(x)\n", &c).unwrap();
    let a = engine::analyze(&units[0]);
    assert!(a.diagnostics.is_empty());
    assert!(!a.coverage.is_empty());
}

#[test]
fn fdopen_transfers_descriptor_and_alias() {
    let a = check(
        "import os\nfd=os.open('x', 0)\nalias=fd\nf=os.fdopen(fd, 'r')\nos.read(alias, 10)\n",
    );
    assert!(a.diagnostics.iter().any(|d| d.rule == "OWN001"), "{a:#?}");
}
#[test]
fn fdopen_wrapper_is_fresh_usable_resource() {
    let a = check("import os\nfd=os.open('x', 0)\nf=os.fdopen(fd, 'r')\nf.read()\nf.close()\n");
    assert!(a.diagnostics.is_empty(), "{a:#?}");
}
#[test]
fn fdopen_wrapper_close_invalidates_wrapper() {
    let a = check(
        "from os import open as raw_open, fdopen as wrap\nfd=raw_open('x', 0)\nf=wrap(fd)\nf.close()\nf.read()\n",
    );
    assert!(a.diagnostics.iter().any(|d| d.rule == "LIFE001"), "{a:#?}");
}
#[test]
fn raw_descriptor_finally_close_valid() {
    let a = check(
        "import os\nfd=os.open('x', 0)\ntry:\n    os.fsync(fd)\nfinally:\n    os.close(fd)\n",
    );
    assert!(a.diagnostics.is_empty(), "{a:#?}");
}
#[test]
fn closed_raw_descriptor_is_invalid() {
    let a = check("import os\nfd=os.open('x', 0)\nos.close(fd)\nos.write(fd, b'x')\n");
    assert!(a.diagnostics.iter().any(|d| d.rule == "LIFE001"), "{a:#?}");
}
#[test]
fn fdopen_closefd_false_keeps_raw_descriptor() {
    let a = check(
        "import os\nfd=os.open('x', 0)\nf=os.fdopen(fd, 'r', closefd=False)\nf.close()\nos.read(fd, 10)\nos.close(fd)\n",
    );
    assert!(a.diagnostics.is_empty(), "{a:#?}");
}
#[test]
fn fdopen_dynamic_keyword_does_not_assume_transfer() {
    let a =
        check("import os\nfd=os.open('x', 0)\nf=os.fdopen(fd, closefd=flag)\nos.read(fd, 10)\n");
    assert!(a.diagnostics.is_empty(), "{a:#?}");
    assert!(
        a.coverage
            .iter()
            .any(|g| g.reason.contains("closefd policy"))
    );
}
#[test]
fn fdopen_unknown_keyword_does_not_assume_transfer() {
    let a = check("import os\nfd=os.open('x', 0)\nf=os.fdopen(fd, **options)\nos.read(fd, 10)\n");
    assert!(a.diagnostics.is_empty(), "{a:#?}");
}
#[test]
fn shadowed_os_not_descriptor_model() {
    let a =
        check("import os\nos=factory()\nfd=os.open('x', 0)\nf=os.fdopen(fd)\nos.read(fd, 10)\n");
    assert!(a.diagnostics.is_empty(), "{a:#?}");
}
#[test]
fn builtins_open_fd_is_explicit_gap() {
    let a = check("import os\nfd=os.open('x', 0)\nf=open(fd)\n");
    assert!(
        a.coverage
            .iter()
            .any(|g| g.reason.contains("open(integer descriptor)"))
    );
}

#[test]
fn file_double_close_is_idempotent() {
    let a = check("f=open('x')\nf.close()\nf.close()\n");
    assert!(a.diagnostics.is_empty(), "{a:#?}");
}
#[test]
fn file_double_close_through_alias_is_idempotent() {
    let a = check("f=open('x')\ng=f\nf.close()\ng.close()\n");
    assert!(a.diagnostics.is_empty(), "{a:#?}");
}
#[test]
fn file_closed_metadata_after_close_is_valid() {
    let a = check("f=open('x')\ng=f\nf.close()\nprint(g.closed)\n");
    assert!(a.diagnostics.is_empty(), "{a:#?}");
    assert!(a.coverage.is_empty(), "{a:#?}");
}
#[test]
fn file_read_and_write_after_double_close_still_invalid() {
    let a = check("f=open('x')\nf.close()\nf.close()\nf.read()\nf.write('x')\n");
    assert!(
        a.diagnostics
            .iter()
            .any(|d| d.rule == "LIFE001" && d.span.line == 4),
        "{a:#?}"
    );
    assert!(
        a.diagnostics
            .iter()
            .any(|d| d.rule == "LIFE001" && d.span.line == 5),
        "{a:#?}"
    );
}
#[test]
fn moved_file_metadata_still_invalid() {
    let mut c = Config::default();
    c.contracts.insert(
        "consume".into(),
        Contract {
            consumes: vec![0],
            ..Default::default()
        },
    );
    let units = lower_source(
        "t.py",
        "f=open('x')\ng=f\nconsume(f)\nprint(g.closed)\ng.close()\n",
        &c,
    )
    .unwrap();
    let a = engine::analyze(&units[0]);
    assert!(
        a.diagnostics
            .iter()
            .any(|d| d.rule == "OWN001" && d.span.line == 4),
        "{a:#?}"
    );
    assert!(
        a.diagnostics
            .iter()
            .any(|d| d.rule == "OWN001" && d.span.line == 5),
        "{a:#?}"
    );
}
