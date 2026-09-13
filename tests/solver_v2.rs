use lint_like_rust::ir::Span;
use lint_like_rust::solver_v2::{Certainty, ObligationStatus, analyze};
use lint_like_rust::v2_ir::*;
fn p(s: &str) -> Place {
    Place::local(s)
}
fn site(a: &str) -> Site {
    Site {
        path: format!("{}.py", a.split(':').next().unwrap()),
        span: Span::default(),
        anchor: a.into(),
    }
}
fn i(a: &str, kind: Kind) -> Instruction {
    Instruction {
        site: site(a),
        kind,
    }
}
fn fun(name: &str, params: &[&str], ops: Vec<Instruction>) -> Function {
    Function {
        id: name.into(),
        params: params.iter().map(|s| p(s)).collect(),
        entry: 0,
        blocks: vec![Block {
            operations: ops,
            terminator: Terminator::Stop,
        }],
    }
}
fn program(funcs: Vec<Function>) -> Program {
    Program {
        functions: funcs,
        roots: vec!["main".into()],
    }
}
fn acquire() -> Instruction {
    i("main:acquire", Kind::Acquire { target: p("f") })
}
fn read() -> Instruction {
    i("main:read", Kind::Read { value: p("f") })
}
fn close() -> Instruction {
    i("main:close", Kind::Close { value: p("f") })
}
fn call(name: &str, args: Vec<Place>, target: Option<Place>) -> Instruction {
    i(
        "main:call",
        Kind::Call {
            target,
            callee: name.into(),
            args,
        },
    )
}
#[test]
fn direct_closed_alias() {
    let a = analyze(&program(vec![fun(
        "main",
        &[],
        vec![
            acquire(),
            i(
                "main:alias",
                Kind::Assign {
                    target: p("g"),
                    source: p("f"),
                },
            ),
            close(),
            i("main:read", Kind::Read { value: p("g") }),
        ],
    )]));
    assert_eq!(a.findings.len(), 1);
    assert_eq!(a.findings[0].certainty, Certainty::Definite);
    assert!(
        a.findings[0]
            .trace
            .iter()
            .any(|t| t.message == "alias assigned")
    );
}
#[test]
fn helper_close_propagates_and_trace_crosses_files() {
    let a = analyze(&program(vec![
        fun(
            "main",
            &[],
            vec![acquire(), call("helper", vec![p("f")], None), read()],
        ),
        fun(
            "helper",
            &["x"],
            vec![i("helper:close", Kind::Close { value: p("x") })],
        ),
    ]));
    assert_eq!(a.findings.len(), 1);
    assert!(
        a.findings[0]
            .trace
            .iter()
            .any(|t| t.site.path == "helper.py")
    );
    assert!(
        a.findings[0]
            .trace
            .iter()
            .any(|t| t.message.contains("argument passed"))
    );
}
#[test]
fn helper_read_without_close_is_verified() {
    let a = analyze(&program(vec![
        fun(
            "main",
            &[],
            vec![acquire(), call("helper", vec![p("f")], None), read()],
        ),
        fun(
            "helper",
            &["x"],
            vec![i("helper:read", Kind::Read { value: p("x") })],
        ),
    ]));
    assert!(a.findings.is_empty());
    assert!(
        a.obligations
            .iter()
            .all(|o| o.status == ObligationStatus::Verified)
    );
}
#[test]
fn fresh_return_preserves_closed_state() {
    let mut helper = fun(
        "helper",
        &[],
        vec![
            i("helper:acquire", Kind::Acquire { target: p("x") }),
            i("helper:close", Kind::Close { value: p("x") }),
        ],
    );
    helper.blocks[0].terminator = Terminator::Return {
        value: Some(p("x")),
        site: site("helper:return"),
    };
    let a = analyze(&program(vec![
        fun(
            "main",
            &[],
            vec![call("helper", vec![], Some(p("f"))), read()],
        ),
        helper,
    ]));
    assert_eq!(a.findings.len(), 1);
    assert_eq!(a.findings[0].certainty, Certainty::Definite);
}
#[test]
fn returned_alias_changes_original() {
    let mut helper = fun("helper", &["x"], vec![]);
    helper.blocks[0].terminator = Terminator::Return {
        value: Some(p("x")),
        site: site("helper:return"),
    };
    let a = analyze(&program(vec![
        fun(
            "main",
            &[],
            vec![
                acquire(),
                call("helper", vec![p("f")], Some(p("g"))),
                i("main:close", Kind::Close { value: p("g") }),
                read(),
            ],
        ),
        helper,
    ]));
    assert_eq!(a.findings.len(), 1);
}
#[test]
fn unknown_between_close_and_read_is_not_verified() {
    let a = analyze(&program(vec![fun(
        "main",
        &[],
        vec![
            acquire(),
            close(),
            i(
                "main:unknown",
                Kind::Unknown {
                    affected: vec![],
                    reason: "dynamic effect".into(),
                },
            ),
            read(),
        ],
    )]));
    assert!(a.findings.is_empty());
    assert_eq!(
        a.obligations
            .iter()
            .find(|o| o.operation == "read")
            .unwrap()
            .status,
        ObligationStatus::Unverified
    );
}
#[test]
fn recursive_call_cannot_prove_safety() {
    let helper = fun(
        "helper",
        &["x"],
        vec![i(
            "helper:recursive",
            Kind::Call {
                target: None,
                callee: "helper".into(),
                args: vec![p("x")],
            },
        )],
    );
    let a = analyze(&program(vec![
        fun(
            "main",
            &[],
            vec![acquire(), call("helper", vec![p("f")], None), read()],
        ),
        helper,
    ]));
    assert!(
        a.obligations
            .iter()
            .any(|o| o.operation == "read" && o.status == ObligationStatus::Unverified)
    );
}
#[test]
fn branch_close_is_possible_not_definite() {
    let mut main = fun("main", &[], vec![acquire()]);
    main.blocks[0].terminator = Terminator::Branch {
        then_target: 1,
        else_target: 2,
    };
    main.blocks.extend([
        Block {
            operations: vec![close()],
            terminator: Terminator::Jump { target: 3 },
        },
        Block {
            operations: vec![],
            terminator: Terminator::Jump { target: 3 },
        },
        Block {
            operations: vec![read()],
            terminator: Terminator::Stop,
        },
    ]);
    let a = analyze(&program(vec![main]));
    assert_eq!(a.findings.len(), 1);
    assert_eq!(a.findings[0].certainty, Certainty::Possible);
}
#[test]
fn loop_close_reaches_fixed_point() {
    let mut main = fun("main", &[], vec![acquire()]);
    main.blocks[0].terminator = Terminator::Jump { target: 1 };
    main.blocks.extend([
        Block {
            operations: vec![read(), close()],
            terminator: Terminator::Branch {
                then_target: 1,
                else_target: 2,
            },
        },
        Block {
            operations: vec![],
            terminator: Terminator::Stop,
        },
    ]);
    let a = analyze(&program(vec![main]));
    assert_eq!(a.findings.len(), 1);
    assert_eq!(a.findings[0].certainty, Certainty::Possible);
    assert!(
        !a.obligations
            .iter()
            .any(|o| o.operation == "analysis_limit")
    );
}
#[test]
fn unknown_footprint_taints_all() {
    let a = analyze(&program(vec![fun(
        "main",
        &[],
        vec![
            acquire(),
            i(
                "main:unknown",
                Kind::Unknown {
                    affected: vec![p("unresolved")],
                    reason: "unknown binding".into(),
                },
            ),
            read(),
        ],
    )]));
    assert!(
        a.obligations
            .iter()
            .any(|o| o.operation == "read" && o.status == ObligationStatus::Unverified)
    );
}
#[test]
fn projected_places_are_not_falsely_verified() {
    let mut field = p("a");
    field.projections.push("file".into());
    let a = analyze(&program(vec![fun(
        "main",
        &[],
        vec![
            i(
                "main:acquire",
                Kind::Acquire {
                    target: field.clone(),
                },
            ),
            i("main:read", Kind::Read { value: field }),
        ],
    )]));
    assert!(!a.obligations.is_empty());
    assert!(
        a.obligations
            .iter()
            .all(|o| o.status == ObligationStatus::Unverified)
    );
}
#[test]
fn ownership_ir_is_explicitly_unverified() {
    let a = analyze(&program(vec![fun(
        "main",
        &[],
        vec![
            acquire(),
            i("main:transfer", Kind::Transfer { value: p("f") }),
            read(),
        ],
    )]));
    assert!(
        a.obligations
            .iter()
            .any(|o| o.operation == "ownership" && o.status == ObligationStatus::Unverified)
    );
    assert!(
        a.obligations
            .iter()
            .any(|o| o.operation == "read" && o.status == ObligationStatus::Unverified)
    );
}

#[test]
fn unresolved_close_in_helper_cannot_leave_caller_verified() {
    let a = analyze(&program(vec![
        fun(
            "main",
            &[],
            vec![acquire(), call("helper", vec![], None), read()],
        ),
        fun(
            "helper",
            &[],
            vec![i("helper:close", Kind::Close { value: p("global") })],
        ),
    ]));
    assert!(
        a.obligations
            .iter()
            .any(|o| o.operation == "read" && o.status == ObligationStatus::Unverified)
    );
    assert!(
        a.obligations
            .iter()
            .any(|o| o.operation == "call" && o.status == ObligationStatus::Unverified)
    );
}

#[test]
fn raising_helper_does_not_claim_verified_normal_call() {
    let mut helper = fun(
        "helper",
        &["x"],
        vec![i("helper:close", Kind::Close { value: p("x") })],
    );
    helper.blocks[0].terminator = Terminator::Raise {
        site: site("helper:raise"),
    };
    let a = analyze(&program(vec![
        fun(
            "main",
            &[],
            vec![acquire(), call("helper", vec![p("f")], None), read()],
        ),
        helper,
    ]));
    assert!(
        a.obligations
            .iter()
            .any(|o| o.operation == "call" && o.status == ObligationStatus::Unverified)
    );
    assert!(
        !a.obligations
            .iter()
            .any(|o| o.operation == "read" && o.status == ObligationStatus::Verified)
    );
}

#[test]
fn identity_aliases_survive_two_helper_levels() {
    let a = analyze(&program(vec![
        fun(
            "main",
            &[],
            vec![acquire(), call("outer", vec![p("f")], None), read()],
        ),
        fun(
            "outer",
            &["x"],
            vec![i(
                "outer:call",
                Kind::Call {
                    target: None,
                    callee: "inner".into(),
                    args: vec![p("x")],
                },
            )],
        ),
        fun(
            "inner",
            &["y"],
            vec![i("inner:close", Kind::Close { value: p("y") })],
        ),
    ]));
    assert_eq!(a.findings.len(), 1);
    assert_eq!(a.findings[0].certainty, Certainty::Definite);
    assert!(
        a.findings[0]
            .trace
            .iter()
            .any(|t| t.site.path == "inner.py")
    );
}

#[test]
fn invalid_root_is_explicitly_unverified() {
    let a = analyze(&Program {
        functions: vec![],
        roots: vec!["missing".into()],
    });
    assert_eq!(a.obligations.len(), 1);
    assert_eq!(a.obligations[0].status, ObligationStatus::Unverified);
}
#[test]
fn analysis_limit_invalidates_earlier_verified_obligations() {
    let mut main = fun("main", &[], vec![acquire(), read()]);
    main.blocks[0].terminator = Terminator::Jump { target: 1 };
    for n in 1..2050 {
        main.blocks.push(Block {
            operations: vec![],
            terminator: if n == 2049 {
                Terminator::Stop
            } else {
                Terminator::Jump { target: n + 1 }
            },
        });
    }
    let a = analyze(&program(vec![main]));
    assert!(
        a.obligations
            .iter()
            .any(|o| o.operation == "analysis_limit")
    );
    assert!(
        a.obligations
            .iter()
            .all(|o| o.status == ObligationStatus::Unverified)
    );
}

#[test]
fn raising_callee_has_no_normal_continuation() {
    let mut raising = fun("fail", &[], vec![]);
    raising.blocks[0].terminator = Terminator::Raise {
        site: site("fail:raise"),
    };
    let a = analyze(&program(vec![
        fun(
            "main",
            &[],
            vec![call("fail", vec![], None), acquire(), close(), read()],
        ),
        raising,
    ]));
    assert!(a.findings.is_empty(), "{a:#?}");
    assert!(
        !a.obligations.iter().any(|o| o.operation == "acquire"),
        "{a:#?}"
    );
    assert!(
        a.obligations
            .iter()
            .any(|o| o.operation == "call" && o.status == ObligationStatus::Unverified)
    );
}
#[test]
fn mixed_normal_exception_exits_are_not_a_verified_call() {
    let mut mixed = fun("mixed", &[], vec![]);
    mixed.blocks[0].terminator = Terminator::Branch {
        then_target: 1,
        else_target: 2,
    };
    mixed.blocks.push(Block {
        operations: vec![],
        terminator: Terminator::Stop,
    });
    mixed.blocks.push(Block {
        operations: vec![],
        terminator: Terminator::Raise {
            site: site("mixed:raise"),
        },
    });
    let a = analyze(&program(vec![
        fun(
            "main",
            &[],
            vec![acquire(), call("mixed", vec![], None), read()],
        ),
        mixed,
    ]));
    assert!(
        a.obligations
            .iter()
            .any(|o| o.operation == "call" && o.status == ObligationStatus::Unverified),
        "{a:#?}"
    );
    assert!(
        a.obligations
            .iter()
            .any(|o| o.operation == "read" && o.status == ObligationStatus::Unverified),
        "{a:#?}"
    );
}
#[test]
fn exceptional_exit_propagates_through_multiple_callers() {
    let mut raising = fun("fail", &[], vec![]);
    raising.blocks[0].terminator = Terminator::Raise {
        site: site("fail:raise"),
    };
    let middle = fun("middle", &[], vec![call("fail", vec![], None)]);
    let a = analyze(&program(vec![
        fun(
            "main",
            &[],
            vec![call("middle", vec![], None), acquire(), close(), read()],
        ),
        middle,
        raising,
    ]));
    assert!(a.findings.is_empty(), "{a:#?}");
    assert!(
        !a.obligations.iter().any(|o| o.operation == "acquire"),
        "{a:#?}"
    );
}

#[test]
fn invoke_transfers_closed_resource_to_handler_without_assigning_return_target() {
    let mut fail = fun(
        "fail",
        &["arg"],
        vec![i("fail:close", Kind::Close { value: p("arg") })],
    );
    fail.blocks[0].terminator = Terminator::Raise {
        site: site("fail:raise"),
    };
    let mut main = fun(
        "main",
        &[],
        vec![
            acquire(),
            i(
                "main:invoke",
                Kind::Invoke {
                    target: Some(p("f")),
                    callee: "fail".into(),
                    args: vec![p("f")],
                    unwind: 1,
                },
            ),
            i(
                "main:unreachable",
                Kind::Acquire {
                    target: p("unused"),
                },
            ),
        ],
    );
    main.blocks.push(Block {
        operations: vec![read()],
        terminator: Terminator::Stop,
    });
    let a = analyze(&program(vec![main, fail]));
    assert!(
        a.findings
            .iter()
            .any(|f| f.rule == "LIFE001" && f.certainty == Certainty::Definite),
        "{a:#?}"
    );
    assert_eq!(
        a.obligations
            .iter()
            .filter(|o| o.operation == "acquire")
            .count(),
        1,
        "{a:#?}"
    );
}
#[test]
fn invoke_keeps_normal_and_exceptional_resource_states_separate() {
    let mut mixed = fun("mixed", &["arg"], vec![]);
    mixed.blocks[0].terminator = Terminator::Branch {
        then_target: 1,
        else_target: 2,
    };
    mixed.blocks.push(Block {
        operations: vec![],
        terminator: Terminator::Stop,
    });
    mixed.blocks.push(Block {
        operations: vec![i("mixed:close", Kind::Close { value: p("arg") })],
        terminator: Terminator::Raise {
            site: site("mixed:raise"),
        },
    });
    let mut main = fun(
        "main",
        &[],
        vec![
            acquire(),
            i(
                "main:invoke",
                Kind::Invoke {
                    target: None,
                    callee: "mixed".into(),
                    args: vec![p("f")],
                    unwind: 1,
                },
            ),
            read(),
        ],
    );
    main.blocks.push(Block {
        operations: vec![i("main:handler-read", Kind::Read { value: p("f") })],
        terminator: Terminator::Stop,
    });
    let a = analyze(&program(vec![main, mixed]));
    assert!(
        a.obligations
            .iter()
            .any(|o| o.site.anchor == "main:read" && o.status == ObligationStatus::Verified),
        "{a:#?}"
    );
    assert!(a.findings.iter().any(|f| f.rule == "LIFE001"), "{a:#?}");
}
#[test]
fn invalid_invoke_successor_is_unverified() {
    let mut fail = fun("fail", &[], vec![]);
    fail.blocks[0].terminator = Terminator::Raise {
        site: site("fail:raise"),
    };
    let a = analyze(&program(vec![
        fun(
            "main",
            &[],
            vec![i(
                "main:invoke",
                Kind::Invoke {
                    target: None,
                    callee: "fail".into(),
                    args: vec![],
                    unwind: 99,
                },
            )],
        ),
        fail,
    ]));
    assert!(
        a.obligations
            .iter()
            .any(|o| o.operation == "invalid_cfg" && o.status == ObligationStatus::Unverified),
        "{a:#?}"
    );
}
