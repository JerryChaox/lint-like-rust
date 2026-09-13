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

fn borrow(to: &str, from: &str, mutable: bool) -> Instruction {
    i(
        &format!("borrow:{to}"),
        Kind::Borrow {
            target: p(to),
            source: p(from),
            mutable,
        },
    )
}
fn release(n: &str) -> Instruction {
    i(&format!("release:{n}"), Kind::EndBorrow { value: p(n) })
}
fn write(n: &str) -> Instruction {
    i(&format!("write:{n}"), Kind::Write { value: p(n) })
}
fn loan_status(a: &Analysis, anchor: &str, operation: &str) -> ObligationStatus {
    a.obligations
        .iter()
        .find(|o| o.site.anchor == anchor && o.operation == operation)
        .unwrap()
        .status
}
#[test]
fn shared_loans_allow_reads_and_block_owner_writes() {
    let a = solve(vec![
        resource("r"),
        borrow("a", "r", false),
        borrow("b", "r", false),
        read("a"),
        read("r"),
        write("r"),
        release("a"),
        read("b"),
        release("b"),
        i("restored", Kind::Write { value: p("r") }),
    ]);
    assert_eq!(
        loan_status(&a, "read:r", "loan_read"),
        ObligationStatus::Verified
    );
    assert_eq!(
        loan_status(&a, "write:r", "loan_write"),
        ObligationStatus::Violated
    );
    assert_eq!(
        loan_status(&a, "restored", "loan_write"),
        ObligationStatus::Verified
    );
}
#[test]
fn borrow_blocks_transfer_and_close() {
    let a = solve(vec![
        resource("r"),
        borrow("v", "r", false),
        close("r"),
        i(
            "transfer",
            Kind::TransferTo {
                source: p("r"),
                target: p("new"),
            },
        ),
    ]);
    assert_eq!(
        loan_status(&a, "close:r", "loan_close"),
        ObligationStatus::Violated
    );
    assert_eq!(
        loan_status(&a, "transfer", "loan_transfer"),
        ObligationStatus::Violated
    );
}
#[test]
fn mutable_child_freezes_parent_until_released() {
    let a = solve(vec![
        resource("r"),
        borrow("a", "r", true),
        borrow("b", "a", true),
        read("b"),
        read("a"),
        release("b"),
        i("restored", Kind::Write { value: p("a") }),
    ]);
    assert_eq!(
        loan_status(&a, "read:b", "loan_read"),
        ObligationStatus::Verified
    );
    assert_eq!(
        loan_status(&a, "read:a", "loan_read"),
        ObligationStatus::Violated
    );
    assert_eq!(
        loan_status(&a, "restored", "loan_write"),
        ObligationStatus::Verified
    );
}
#[test]
fn multiple_possible_sources_do_not_create_definite_conflict() {
    let a = analyze(&Program {
        roots: vec!["main".into()],
        functions: vec![fun(
            "main",
            &[],
            vec![
                block(
                    vec![resource("r"), resource("s")],
                    Terminator::Branch {
                        then_target: 1,
                        else_target: 2,
                    },
                ),
                block(vec![assign("x", "r")], Terminator::Jump { target: 3 }),
                block(vec![assign("x", "s")], Terminator::Jump { target: 3 }),
                block(vec![borrow("v", "x", true), read("r")], Terminator::Stop),
            ],
        )],
    });
    let finding = a
        .findings
        .iter()
        .find(|f| f.site.anchor == "read:r" && f.rule == "BORROW001")
        .unwrap();
    assert_eq!(finding.certainty, Certainty::Possible);
}
#[test]
fn releasing_ambiguous_view_cannot_end_both_loans() {
    let a = analyze(&Program {
        roots: vec!["main".into()],
        functions: vec![fun(
            "main",
            &[],
            vec![
                block(
                    vec![
                        resource("r"),
                        borrow("a", "r", false),
                        borrow("b", "r", false),
                    ],
                    Terminator::Branch {
                        then_target: 1,
                        else_target: 2,
                    },
                ),
                block(vec![assign("x", "a")], Terminator::Jump { target: 3 }),
                block(vec![assign("x", "b")], Terminator::Jump { target: 3 }),
                block(vec![release("x"), write("r")], Terminator::Stop),
            ],
        )],
    });
    assert_eq!(
        loan_status(&a, "write:r", "loan_write"),
        ObligationStatus::Violated
    );
}
#[test]
fn heap_rebinding_does_not_end_extracted_view() {
    let a = solve(vec![
        object("h"),
        resource("r"),
        borrow("v", "r", true),
        assign("h.v", "v"),
        assign("old", "h.v"),
        resource("s"),
        borrow("other", "s", true),
        assign("h.v", "other"),
        release("h.v"),
        read("r"),
        release("old"),
        i("restored", Kind::Read { value: p("r") }),
    ]);
    assert_eq!(
        loan_status(&a, "read:r", "loan_read"),
        ObligationStatus::Violated
    );
    assert_eq!(
        loan_status(&a, "restored", "loan_read"),
        ObligationStatus::Verified
    );
}
#[test]
fn ended_parent_cannot_mint_verified_live_child() {
    let a = solve(vec![
        resource("r"),
        borrow("v", "r", true),
        release("v"),
        borrow("child", "v", false),
        read("child"),
    ]);
    assert_eq!(
        loan_status(&a, "read:child", "loan_read"),
        ObligationStatus::Unverified
    );
}
#[test]
fn loop_loan_sites_do_not_strongly_release_multiple_views() {
    let a = analyze(&Program {
        roots: vec!["main".into()],
        functions: vec![fun(
            "main",
            &[],
            vec![
                block(vec![resource("r")], Terminator::Jump { target: 1 }),
                block(
                    vec![borrow("v", "r", false)],
                    Terminator::Branch {
                        then_target: 1,
                        else_target: 2,
                    },
                ),
                block(vec![release("v"), write("r")], Terminator::Stop),
            ],
        )],
    });
    assert_ne!(
        loan_status(&a, "write:r", "loan_write"),
        ObligationStatus::Verified
    );
}
