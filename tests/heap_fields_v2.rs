use lint_like_rust::{
    ir::Span,
    solver_v2::{Analysis, Certainty, ObligationStatus, analyze},
    v2_ir::*,
};
fn p(n: &str) -> Place {
    let mut s = n.split('.');
    Place {
        root: s.next().unwrap().into(),
        projections: s.map(Into::into).collect(),
    }
}
fn i(n: &str, kind: Kind) -> Instruction {
    Instruction {
        site: Site {
            path: "heap.py".into(),
            span: Span::default(),
            anchor: n.into(),
        },
        kind,
    }
}
fn object(n: &str) -> Instruction {
    i(&format!("obj:{n}"), Kind::AllocateObject { target: p(n) })
}
fn resource(n: &str) -> Instruction {
    i(&format!("res:{n}"), Kind::Acquire { target: p(n) })
}
fn assign(to: &str, from: &str) -> Instruction {
    i(
        &format!("set:{to}:{from}"),
        Kind::Assign {
            target: p(to),
            source: p(from),
        },
    )
}
fn close(n: &str) -> Instruction {
    i(&format!("close:{n}"), Kind::Close { value: p(n) })
}
fn read(n: &str) -> Instruction {
    i(&format!("read:{n}"), Kind::Read { value: p(n) })
}
fn block(operations: Vec<Instruction>, terminator: Terminator) -> Block {
    Block {
        operations,
        terminator,
    }
}
fn fun(id: &str, params: &[&str], blocks: Vec<Block>) -> Function {
    Function {
        id: id.into(),
        params: params.iter().map(|n| p(n)).collect(),
        entry: 0,
        blocks,
    }
}
fn solve(ops: Vec<Instruction>) -> Analysis {
    analyze(&Program {
        functions: vec![fun("main", &[], vec![block(ops, Terminator::Stop)])],
        roots: vec!["main".into()],
    })
}
fn status(a: &Analysis, name: &str) -> ObligationStatus {
    a.obligations
        .iter()
        .find(|o| o.site.anchor == format!("read:{name}"))
        .unwrap()
        .status
}
#[test]
fn object_aliases_share_fields_and_independent_instances_do_not() {
    let a = solve(vec![
        object("a"),
        object("b"),
        resource("f"),
        resource("g"),
        assign("a.file", "f"),
        assign("b.file", "g"),
        assign("alias", "a"),
        close("alias.file"),
        read("a.file"),
        read("b.file"),
    ]);
    assert_eq!(status(&a, "a.file"), ObligationStatus::Violated);
    assert_eq!(status(&a, "b.file"), ObligationStatus::Verified);
}
#[test]
fn field_rebind_preserves_extracted_old_alias() {
    let a = solve(vec![
        object("a"),
        resource("f"),
        resource("g"),
        assign("a.file", "f"),
        assign("old", "a.file"),
        assign("a.file", "g"),
        close("f"),
        read("old"),
        read("a.file"),
    ]);
    assert_eq!(status(&a, "old"), ObligationStatus::Violated);
    assert_eq!(status(&a, "a.file"), ObligationStatus::Verified);
}
#[test]
fn nested_fields_and_aliases_resolve_same_resource() {
    let a = solve(vec![
        object("a"),
        object("b"),
        assign("a.child", "b"),
        resource("a.child.file"),
        assign("old", "b.file"),
        close("a.child.file"),
        read("old"),
    ]);
    assert_eq!(status(&a, "old"), ObligationStatus::Violated);
}
#[test]
fn field_close_propagates_through_calls_and_returned_alias() {
    let getter = fun(
        "getter",
        &["x"],
        vec![block(
            vec![close("x.file")],
            Terminator::Return {
                value: Some(p("x.file")),
                site: i("return", Kind::SetException { categories: 0 }).site,
            },
        )],
    );
    let main = fun(
        "main",
        &[],
        vec![block(
            vec![
                object("a"),
                resource("a.file"),
                i(
                    "get",
                    Kind::Call {
                        target: Some(p("out")),
                        callee: "getter".into(),
                        args: vec![p("a")],
                    },
                ),
                read("out"),
                read("a.file"),
            ],
            Terminator::Stop,
        )],
    );
    let a = analyze(&Program {
        functions: vec![main, getter],
        roots: vec!["main".into()],
    });
    assert_eq!(status(&a, "out"), ObligationStatus::Violated);
    assert_eq!(status(&a, "a.file"), ObligationStatus::Violated);
}
#[test]
fn callee_field_rebind_and_exception_heap_return_to_caller() {
    let child = fun(
        "child",
        &["x", "g"],
        vec![block(
            vec![
                assign("x.file", "g"),
                close("x.file"),
                i("exception", Kind::SetException { categories: 1 }),
            ],
            Terminator::Raise {
                site: i("raise", Kind::SetException { categories: 1 }).site,
            },
        )],
    );
    let main = fun(
        "main",
        &[],
        vec![
            block(
                vec![
                    object("a"),
                    resource("a.file"),
                    resource("g"),
                    assign("old", "a.file"),
                    i(
                        "call",
                        Kind::Invoke {
                            target: None,
                            callee: "child".into(),
                            args: vec![p("a"), p("g")],
                            unwind: 1,
                        },
                    ),
                ],
                Terminator::Stop,
            ),
            block(vec![read("a.file"), read("old")], Terminator::Stop),
        ],
    );
    let a = analyze(&Program {
        functions: vec![main, child],
        roots: vec!["main".into()],
    });
    assert_eq!(status(&a, "a.file"), ObligationStatus::Violated);
    assert_eq!(status(&a, "old"), ObligationStatus::Verified);
}
#[test]
fn unknown_call_taints_transitively_reachable_fields_even_with_cycles() {
    let a = solve(vec![
        object("a"),
        object("b"),
        assign("a.child", "b"),
        assign("b.parent", "a"),
        resource("b.file"),
        assign("alias", "b.file"),
        i(
            "unknown",
            Kind::Unknown {
                affected: vec![p("a")],
                reason: "plugin".into(),
            },
        ),
        read("alias"),
        read("b.file"),
    ]);
    assert_eq!(status(&a, "alias"), ObligationStatus::Unverified);
    assert_eq!(status(&a, "b.file"), ObligationStatus::Unverified);
}
#[test]
fn unknown_receiver_write_cannot_prove_an_existing_field_safe() {
    let a = solve(vec![
        object("a"),
        resource("a.file"),
        resource("g"),
        assign("unknown.file", "g"),
        read("a.file"),
    ]);
    assert_eq!(status(&a, "a.file"), ObligationStatus::Unverified);
}
#[test]
fn missing_field_on_one_branch_does_not_become_verified() {
    let f = fun(
        "main",
        &[],
        vec![
            block(
                vec![object("a"), resource("f")],
                Terminator::Branch {
                    then_target: 1,
                    else_target: 2,
                },
            ),
            block(vec![assign("a.file", "f")], Terminator::Jump { target: 2 }),
            block(vec![read("a.file")], Terminator::Stop),
        ],
    );
    let a = analyze(&Program {
        functions: vec![f],
        roots: vec!["main".into()],
    });
    assert_eq!(status(&a, "a.file"), ObligationStatus::Unverified);
}
#[test]
fn multiple_possible_receivers_require_weak_field_updates() {
    let f = fun(
        "main",
        &[],
        vec![
            block(
                vec![
                    object("a"),
                    object("b"),
                    resource("f"),
                    resource("g"),
                    assign("a.file", "f"),
                    assign("b.file", "f"),
                    close("f"),
                ],
                Terminator::Branch {
                    then_target: 1,
                    else_target: 2,
                },
            ),
            block(vec![assign("x", "a")], Terminator::Jump { target: 3 }),
            block(vec![assign("x", "b")], Terminator::Jump { target: 3 }),
            block(
                vec![assign("x.file", "g"), read("a.file")],
                Terminator::Stop,
            ),
        ],
    );
    let a = analyze(&Program {
        functions: vec![f],
        roots: vec!["main".into()],
    });
    assert!(
        a.findings
            .iter()
            .any(|f| f.site.anchor == "read:a.file" && f.certainty == Certainty::Possible),
        "{a:#?}"
    );
}
#[test]
fn loop_summary_objects_never_allow_strong_field_replacement() {
    let f = fun(
        "main",
        &[],
        vec![
            block(
                vec![resource("f"), resource("g"), close("f")],
                Terminator::Jump { target: 1 },
            ),
            block(
                vec![object("a"), assign("a.file", "f")],
                Terminator::Branch {
                    then_target: 1,
                    else_target: 2,
                },
            ),
            block(
                vec![assign("a.file", "g"), read("a.file")],
                Terminator::Stop,
            ),
        ],
    );
    let a = analyze(&Program {
        functions: vec![f],
        roots: vec!["main".into()],
    });
    assert_ne!(status(&a, "a.file"), ObligationStatus::Verified, "{a:#?}");
}

#[test]
fn heap_mutation_on_normal_return_preserves_callers_local_bindings() {
    let child = fun(
        "child",
        &["x", "g"],
        vec![block(vec![assign("x.file", "g")], Terminator::Stop)],
    );
    let main = fun(
        "main",
        &[],
        vec![block(
            vec![
                object("a"),
                resource("a.file"),
                resource("g"),
                assign("old", "a.file"),
                close("g"),
                i(
                    "call",
                    Kind::Call {
                        target: None,
                        callee: "child".into(),
                        args: vec![p("a"), p("g")],
                    },
                ),
                read("a.file"),
                read("old"),
            ],
            Terminator::Stop,
        )],
    );
    let a = analyze(&Program {
        functions: vec![main, child],
        roots: vec!["main".into()],
    });
    assert_eq!(status(&a, "a.file"), ObligationStatus::Violated);
    assert_eq!(status(&a, "old"), ObligationStatus::Verified);
}
#[test]
fn object_allocation_does_not_invent_resource_protocol() {
    let a = solve(vec![object("a"), close("a"), read("a")]);
    assert_eq!(status(&a, "a"), ObligationStatus::Unverified);
    assert!(
        a.obligations
            .iter()
            .any(|o| o.operation == "close" && o.status == ObligationStatus::Unverified)
    );
}
#[test]
fn unrelated_heap_resource_survives_bounded_unknown_footprint() {
    let a = solve(vec![
        object("a"),
        object("b"),
        resource("a.file"),
        resource("b.file"),
        i(
            "unknown",
            Kind::Unknown {
                affected: vec![p("a")],
                reason: "bounded effect".into(),
            },
        ),
        read("a.file"),
        read("b.file"),
    ]);
    assert_eq!(status(&a, "a.file"), ObligationStatus::Unverified);
    assert_eq!(status(&a, "b.file"), ObligationStatus::Verified);
}
