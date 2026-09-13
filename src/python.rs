//! Python syntax and library models. Scanned programs are never executed.
use crate::{
    config::{Config, Contract},
    ir::{Op, OpKind, Span, Unit},
};
use std::collections::{BTreeMap, BTreeSet};
use tree_sitter::{Node, Parser};

fn children(n: Node<'_>) -> Vec<Node<'_>> {
    let mut c = n.walk();
    n.named_children(&mut c).collect()
}
fn field<'a>(n: Node<'a>, name: &str) -> Option<Node<'a>> {
    n.child_by_field_name(name)
}
fn span(n: Node<'_>) -> Span {
    let a = n.start_position();
    let b = n.end_position();
    Span {
        line: a.row + 1,
        column: a.column + 1,
        end_line: b.row + 1,
        end_column: b.column + 1,
    }
}
fn op(n: Node<'_>, kind: OpKind) -> Op {
    Op {
        span: span(n),
        kind,
    }
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum Kind {
    Resource,
    File,
    RawFd,
    Collection,
    Other,
}
struct Front<'a> {
    source: &'a str,
    config: &'a Config,
    imports: BTreeMap<String, String>,
    shadow: BTreeSet<String>,
    kinds: BTreeMap<String, Kind>,
    summaries: BTreeMap<String, Contract>,
    counter: usize,
}
impl<'a> Front<'a> {
    fn text(&self, n: Node<'_>) -> &str {
        &self.source[n.byte_range()]
    }
    fn temp(&mut self) -> String {
        self.counter += 1;
        format!("$tmp{}", self.counter)
    }
    fn unknown(&self, n: Node<'_>, reason: impl Into<String>, out: &mut Vec<Op>) {
        out.push(op(
            n,
            OpKind::Unknown {
                reason: reason.into(),
            },
        ));
    }
    fn name(&self, n: Node<'_>) -> String {
        if n.kind() == "identifier" {
            let t = self.text(n);
            if let Some(v) = self.imports.get(t) {
                v.clone()
            } else if matches!(t, "os" | "io" | "builtins") {
                format!("$local.{t}")
            } else if !self.shadow.contains(t)
                && matches!(
                    t,
                    "open"
                        | "list"
                        | "dict"
                        | "set"
                        | "tuple"
                        | "iter"
                        | "memoryview"
                        | "len"
                        | "str"
                        | "int"
                        | "float"
                        | "bool"
                        | "print"
                        | "range"
                        | "enumerate"
                        | "zip"
                )
            {
                format!("builtins.{t}")
            } else {
                t.into()
            }
        } else if n.kind() == "attribute" {
            format!(
                "{}.{}",
                self.name(field(n, "object").unwrap()),
                self.text(field(n, "attribute").unwrap())
            )
        } else {
            self.text(n).into()
        }
    }
    fn import(&mut self, n: Node<'_>) {
        let module = field(n, "module_name").map(|x| self.text(x).to_string());
        for ch in children(n) {
            if Some(ch) == field(n, "module_name") {
                continue;
            }
            let (original, alias) = if ch.kind() == "aliased_import" {
                (field(ch, "name").unwrap(), field(ch, "alias"))
            } else {
                (ch, None)
            };
            if !matches!(original.kind(), "dotted_name" | "identifier") {
                continue;
            }
            let orig = self.text(original).to_string();
            let key = alias
                .map(|a| self.text(a).to_string())
                .unwrap_or_else(|| orig.split('.').next().unwrap_or(&orig).into());
            let val = if let Some(m) = &module {
                format!("{m}.{orig}")
            } else if alias.is_none() {
                key.clone()
            } else {
                orig
            };
            self.imports.insert(key, val);
        }
    }
    fn expr(&mut self, n: Node<'_>, out: &mut Vec<Op>) -> String {
        match n.kind() {
            "identifier" => {
                let v = self.text(n).to_string();
                out.push(op(n, OpKind::Use { value: v.clone() }));
                v
            }
            "call" => self.call(n, out),
            "attribute" | "subscript" => {
                if n.kind() == "attribute"
                    && field(n, "attribute").is_some_and(|a| self.text(a) == "closed")
                    && let Some(object) = field(n, "object")
                    && object.kind() == "identifier"
                    && self.kinds.get(self.text(object)) == Some(&Kind::File)
                {
                    out.push(op(
                        object,
                        OpKind::Inspect {
                            value: self.text(object).into(),
                        },
                    ));
                    let t = self.temp();
                    out.push(op(
                        n,
                        OpKind::New {
                            target: t.clone(),
                            resource: false,
                        },
                    ));
                    return t;
                }
                let object = field(n, "object")
                    .or_else(|| field(n, "value"))
                    .or_else(|| children(n).first().copied());
                if let Some(o) = object {
                    self.expr(o, out);
                }
                if let Some(s) = field(n, "subscript") {
                    self.expr(s, out);
                }
                let t = self.temp();
                out.push(op(
                    n,
                    OpKind::New {
                        target: t.clone(),
                        resource: false,
                    },
                ));
                self.unknown(n, "Attribute or element identity is not resolved", out);
                t
            }
            "list" | "dictionary" | "set" | "tuple" => {
                for c in children(n) {
                    self.expr(c, out);
                }
                let t = self.temp();
                out.push(op(
                    n,
                    OpKind::New {
                        target: t.clone(),
                        resource: false,
                    },
                ));
                self.kinds.insert(t.clone(), Kind::Collection);
                t
            }
            "integer" | "float" | "string" | "true" | "false" | "none" => {
                let t = self.temp();
                out.push(op(
                    n,
                    OpKind::New {
                        target: t.clone(),
                        resource: false,
                    },
                ));
                t
            }
            "parenthesized_expression" => {
                if let Some(c) = children(n).first() {
                    self.expr(*c, out)
                } else {
                    self.temp()
                }
            }
            _ => {
                for c in children(n) {
                    self.expr(c, out);
                }
                if !matches!(
                    n.kind(),
                    "boolean_operator"
                        | "comparison_operator"
                        | "pair"
                        | "string_content"
                        | "interpolation"
                ) {
                    self.unknown(n, format!("Unsupported expression: {}", n.kind()), out);
                }
                let t = self.temp();
                out.push(op(
                    n,
                    OpKind::New {
                        target: t.clone(),
                        resource: false,
                    },
                ));
                t
            }
        }
    }
    fn call(&mut self, n: Node<'_>, out: &mut Vec<Op>) -> String {
        let fun = field(n, "function").unwrap();
        let name = self.name(fun);
        let receiver = if fun.kind() == "attribute" {
            field(fun, "object").map(|o| {
                if field(fun, "attribute").is_some_and(|a| self.text(a) == "close")
                    && o.kind() == "identifier"
                    && self.kinds.get(self.text(o)) == Some(&Kind::File)
                {
                    let value = self.text(o).to_string();
                    out.push(op(
                        o,
                        OpKind::Inspect {
                            value: value.clone(),
                        },
                    ));
                    value
                } else {
                    self.expr(o, out)
                }
            })
        } else {
            None
        };
        let args = field(n, "arguments").map(children).unwrap_or_default();
        let mut values = Vec::new();
        for a in &args {
            if matches!(
                a.kind(),
                "keyword_argument" | "list_splat" | "dictionary_splat"
            ) {
                self.unknown(
                    *a,
                    "Keyword or variadic argument binding is not resolved",
                    out,
                );
            }
            values.push(self.expr(field(*a, "value").unwrap_or(*a), out));
        }
        let method = field(fun, "attribute")
            .map(|x| self.text(x).to_string())
            .unwrap_or_default();
        let t = self.temp();
        let mut contract = self
            .config
            .contracts
            .get(&name)
            .or_else(|| self.summaries.get(&name))
            .cloned();
        if args.iter().any(|a| {
            matches!(
                a.kind(),
                "keyword_argument" | "list_splat" | "dictionary_splat"
            )
        }) {
            contract = None;
        }
        if !self.config.contracts.contains_key(&name)
            && contract.as_ref().is_some_and(|c| {
                c.mutable.iter().any(|i| {
                    values
                        .get(*i)
                        .is_none_or(|v| self.kinds.get(v) != Some(&Kind::Collection))
                })
            })
        {
            self.unknown(
                n,
                format!("Inferred mutator effects require a known builtin collection: {name}"),
                out,
            );
            contract = None;
        }
        if let Some(c) = contract {
            if !self.config.contracts.contains_key(&name)
                && values
                    .iter()
                    .enumerate()
                    .any(|(i, _)| !c.readonly.contains(&i) && !c.mutable.contains(&i))
            {
                self.unknown(
                    n,
                    format!("Incomplete inferred parameter effects for {name}"),
                    out,
                );
            }

            let mut loans = Vec::new();
            for (indices, mutable) in [(&c.readonly, false), (&c.mutable, true)] {
                for i in indices {
                    if let Some(v) = values.get(*i) {
                        let b = self.temp();
                        out.push(op(
                            n,
                            OpKind::Borrow {
                                target: b.clone(),
                                source: v.clone(),
                                mutable,
                            },
                        ));
                        loans.push((b, mutable));
                    }
                }
            }
            for (b, mutable) in &loans {
                out.push(op(
                    n,
                    if *mutable {
                        OpKind::Write {
                            value: b.clone(),
                            structural: true,
                        }
                    } else {
                        OpKind::Use { value: b.clone() }
                    },
                ));
            }
            for (b, _) in loans {
                out.push(op(n, OpKind::EndBorrow { target: b }));
            }
            for i in &c.consumes {
                if let Some(v) = values.get(*i) {
                    out.push(op(n, OpKind::Move { value: v.clone() }));
                }
            }
            for i in &c.closes {
                if let Some(v) = values.get(*i) {
                    out.push(op(n, OpKind::Close { value: v.clone() }));
                }
            }
            if let Some(i) = c.returns_alias
                && let Some(v) = values.get(i)
            {
                out.push(op(
                    n,
                    OpKind::Alias {
                        target: t.clone(),
                        source: v.clone(),
                    },
                ));
                return t;
            }
            if let Some((i, mutable)) = c
                .returns_borrow
                .map(|i| (i, false))
                .or(c.returns_mut_borrow.map(|i| (i, true)))
                && let Some(v) = values.get(i)
            {
                out.push(op(
                    n,
                    OpKind::Borrow {
                        target: t.clone(),
                        source: v.clone(),
                        mutable,
                    },
                ));
                return t;
            }
            if !self.config.contracts.contains_key(&name) && !c.pure {
                self.unknown(
                    n,
                    format!("Return identity/effects are not fully resolved for {name}"),
                    out,
                );
            }
            out.push(op(
                n,
                OpKind::New {
                    target: t.clone(),
                    resource: c.returns_resource,
                },
            ));
            if c.returns_resource {
                self.kinds.insert(t.clone(), Kind::Resource);
            }
            return t;
        }
        if name == "os.open" {
            out.push(op(
                n,
                OpKind::New {
                    target: t.clone(),
                    resource: true,
                },
            ));
            self.kinds.insert(t.clone(), Kind::RawFd);
            return t;
        }
        if matches!(
            name.as_str(),
            "os.read" | "os.write" | "os.fsync" | "os.close"
        ) {
            if let Some(v) = values.first() {
                if self.kinds.get(v) == Some(&Kind::RawFd) {
                    if name == "os.close" {
                        out.push(op(n, OpKind::Close { value: v.clone() }));
                    }
                } else {
                    self.unknown(n, "Raw file descriptor identity is not resolved", out);
                }
            }
            out.push(op(
                n,
                OpKind::New {
                    target: t.clone(),
                    resource: false,
                },
            ));
            return t;
        }
        if name == "os.fdopen" {
            // Python transfers ownership only on successful wrapper construction.
            // Exceptional construction outcomes are deliberately not certified.
            self.unknown(
                n,
                "os.fdopen exceptional ownership outcome is not modeled",
                out,
            );
            let mut closefd = if args
                .iter()
                .filter(|a| a.kind() != "keyword_argument")
                .count()
                > 2
            {
                None
            } else {
                Some(true)
            };
            for a in &args {
                match a.kind() {
                    "keyword_argument" => {
                        let key = field(*a, "name").map(|x| self.text(x)).unwrap_or("");
                        if key == "closefd" {
                            closefd = field(*a, "value").and_then(|v| match self.text(v) {
                                "False" => Some(false),
                                "True" => Some(true),
                                _ => None,
                            });
                        } else {
                            closefd = None;
                        }
                    }
                    "list_splat" | "dictionary_splat" => closefd = None,
                    _ => {}
                }
            }
            if let Some(v) = values.first() {
                if self.kinds.get(v) == Some(&Kind::RawFd) {
                    if closefd == Some(true) {
                        out.push(op(n, OpKind::Move { value: v.clone() }));
                    } else if closefd.is_none() {
                        self.unknown(
                            n,
                            "os.fdopen closefd policy is not statically resolved",
                            out,
                        );
                    }
                } else {
                    self.unknown(
                        n,
                        "os.fdopen input descriptor identity is not resolved",
                        out,
                    );
                }
            }
            out.push(op(
                n,
                OpKind::New {
                    target: t.clone(),
                    resource: true,
                },
            ));
            self.kinds.insert(t.clone(), Kind::File);
            return t;
        }
        if matches!(name.as_str(), "builtins.open" | "io.open") {
            if args.first().is_some_and(|a| a.kind() == "integer")
                || values
                    .first()
                    .is_some_and(|v| self.kinds.get(v) == Some(&Kind::RawFd))
            {
                self.unknown(
                    n,
                    "open(integer descriptor) ownership is not modeled; use os.fdopen",
                    out,
                );
            }

            out.push(op(
                n,
                OpKind::New {
                    target: t.clone(),
                    resource: true,
                },
            ));
            self.kinds.insert(t.clone(), Kind::File);
            return t;
        }
        if matches!(name.as_str(), "builtins.iter" | "builtins.memoryview")
            && let Some(v) = values.first()
        {
            out.push(op(
                n,
                OpKind::Borrow {
                    target: t.clone(),
                    source: v.clone(),
                    mutable: false,
                },
            ));
            return t;
        }
        if let Some(r) = &receiver {
            let known = self.kinds.get(r).copied();
            if method == "close" && matches!(known, Some(Kind::Resource | Kind::File)) {
                out.push(op(n, OpKind::Close { value: r.clone() }));
            } else if matches!(method.as_str(), "items" | "keys" | "values")
                && known == Some(Kind::Collection)
            {
                out.push(op(
                    n,
                    OpKind::Borrow {
                        target: t.clone(),
                        source: r.clone(),
                        mutable: false,
                    },
                ));
                return t;
            } else if matches!(
                method.as_str(),
                "append"
                    | "extend"
                    | "insert"
                    | "pop"
                    | "remove"
                    | "clear"
                    | "update"
                    | "add"
                    | "discard"
                    | "sort"
                    | "reverse"
                    | "setdefault"
            ) && known == Some(Kind::Collection)
            {
                out.push(op(
                    n,
                    OpKind::Write {
                        value: r.clone(),
                        structural: true,
                    },
                ));
            } else if matches!(known, Some(Kind::Resource | Kind::File))
                && matches!(
                    method.as_str(),
                    "read" | "readline" | "readlines" | "write" | "flush" | "seek" | "tell"
                )
            {
            } else {
                self.unknown(n, format!("Unresolved call: {name}"), out);
            }
        } else if !matches!(
            name.as_str(),
            "builtins.list"
                | "builtins.tuple"
                | "builtins.set"
                | "builtins.dict"
                | "builtins.len"
                | "builtins.str"
                | "builtins.int"
                | "builtins.float"
                | "builtins.bool"
                | "builtins.print"
                | "builtins.range"
        ) {
            self.unknown(n, format!("Unresolved call: {name}"), out);
        }
        out.push(op(
            n,
            OpKind::New {
                target: t.clone(),
                resource: false,
            },
        ));
        if matches!(
            name.as_str(),
            "builtins.list" | "builtins.tuple" | "builtins.set" | "builtins.dict"
        ) {
            self.kinds.insert(t.clone(), Kind::Collection);
            for v in values {
                if v.starts_with("$tmp") {
                    out.push(op(n, OpKind::EndBorrow { target: v }));
                }
            }
        }
        t
    }
    fn bind(&mut self, n: Node<'_>, v: &str, out: &mut Vec<Op>) {
        if n.kind() == "identifier" {
            let target = self.text(n).to_string();
            self.shadow.insert(target.clone());
            self.imports.remove(&target);
            self.kinds.insert(
                target.clone(),
                self.kinds.get(v).copied().unwrap_or(Kind::Other),
            );
            out.push(op(
                n,
                OpKind::Alias {
                    target,
                    source: v.into(),
                },
            ));
        } else if n.kind() == "subscript" || n.kind() == "attribute" {
            if let Some(o) = field(n, "object")
                .or_else(|| field(n, "value"))
                .or_else(|| children(n).first().copied())
            {
                let value = self.expr(o, out);
                out.push(op(
                    n,
                    OpKind::Write {
                        value,
                        structural: true,
                    },
                ));
            }
            out.push(op(n, OpKind::Escape { value: v.into() }));
            self.unknown(n, "Stored value identity is not tracked", out);
        } else {
            for c in children(n) {
                let t = self.temp();
                out.push(op(
                    c,
                    OpKind::New {
                        target: t.clone(),
                        resource: false,
                    },
                ));
                self.bind(c, &t, out);
            }
            self.unknown(n, "Destructuring element identities are not tracked", out);
        }
    }
    fn block(&mut self, n: Node<'_>) -> Vec<Op> {
        let mut out = Vec::new();
        for s in children(n) {
            self.stmt(s, &mut out);
        }
        out
    }
    fn stmt(&mut self, n: Node<'_>, out: &mut Vec<Op>) {
        match n.kind() {
            "import_statement" | "import_from_statement" => self.import(n),
            "function_definition" | "class_definition" | "decorated_definition" => {}
            "expression_statement" => {
                for c in children(n) {
                    if c.kind() == "assignment" || c.kind() == "augmented_assignment" {
                        self.stmt(c, out);
                    } else {
                        if c.kind() == "call" {
                            let name = self.name(field(c, "function").unwrap());
                            if self
                                .config
                                .contracts
                                .get(&name)
                                .or_else(|| self.summaries.get(&name))
                                .is_some_and(|c| c.must_use || c.pure)
                            {
                                out.push(op(c, OpKind::MustUse { callee: name }));
                            }
                        }
                        let v = self.expr(c, out);
                        if v.starts_with("$tmp") {
                            out.push(op(c, OpKind::EndBorrow { target: v }));
                        }
                    }
                }
            }
            "assignment" => {
                if let (Some(l), Some(r)) = (field(n, "left"), field(n, "right")) {
                    let v = if r.kind() == "assignment" {
                        self.stmt(r, out);
                        field(r, "left")
                            .map(|x| self.text(x).to_string())
                            .unwrap_or_default()
                    } else {
                        self.expr(r, out)
                    };
                    self.bind(l, &v, out);
                }
            }
            "augmented_assignment" => {
                if let Some(r) = field(n, "right") {
                    self.expr(r, out);
                }
                if let Some(l) = field(n, "left") {
                    let value = self.expr(l, out);
                    out.push(op(
                        n,
                        OpKind::Write {
                            value,
                            structural: false,
                        },
                    ));
                }
            }
            "if_statement" | "elif_clause" => {
                if let Some(c) = field(n, "condition") {
                    self.expr(c, out);
                }
                let before_kinds = self.kinds.clone();
                let before_imports = self.imports.clone();
                let before_shadow = self.shadow.clone();
                let then_body = field(n, "consequence")
                    .map(|b| self.block(b))
                    .unwrap_or_default();
                let then_kinds = self.kinds.clone();
                let then_imports = self.imports.clone();
                let then_shadow = self.shadow.clone();
                self.kinds = before_kinds;
                self.imports = before_imports;
                self.shadow = before_shadow;
                let else_body = field(n, "alternative")
                    .map(|b| {
                        if b.kind() == "elif_clause" {
                            let mut o = vec![];
                            self.stmt(b, &mut o);
                            o
                        } else {
                            self.block(b)
                        }
                    })
                    .unwrap_or_default();
                self.kinds.retain(|k, v| then_kinds.get(k) == Some(v));
                self.imports.retain(|k, v| then_imports.get(k) == Some(v));
                self.shadow.extend(then_shadow);
                out.push(op(
                    n,
                    OpKind::Branch {
                        then_body,
                        else_body,
                    },
                ));
            }
            "else_clause" | "finally_clause" => {
                for c in children(n) {
                    if c.kind() == "block" {
                        out.extend(self.block(c));
                    }
                }
            }
            "for_statement" => {
                let mut body = vec![];
                let mut loan = None;
                if let Some(r) = field(n, "right") {
                    let value = self.expr(r, out);
                    let target = self.temp();
                    out.push(op(
                        r,
                        OpKind::Borrow {
                            target: target.clone(),
                            source: value,
                            mutable: false,
                        },
                    ));
                    loan = Some(target);
                }
                if let Some(l) = field(n, "left") {
                    let t = self.temp();
                    body.push(op(
                        l,
                        OpKind::New {
                            target: t.clone(),
                            resource: false,
                        },
                    ));
                    self.bind(l, &t, &mut body);
                }
                if let Some(b) = field(n, "body") {
                    body.extend(self.block(b));
                }
                if let Some(target) = &loan {
                    body.push(op(
                        n,
                        OpKind::Use {
                            value: target.clone(),
                        },
                    ));
                }
                let else_body = field(n, "alternative")
                    .map(|b| self.block(b))
                    .unwrap_or_default();
                out.push(op(n, OpKind::Loop { body, else_body }));
                if let Some(target) = loan {
                    out.push(op(n, OpKind::EndBorrow { target }));
                }
            }
            "while_statement" => {
                let mut body = vec![];
                if let Some(c) = field(n, "condition") {
                    self.expr(c, &mut body);
                }
                if let Some(b) = field(n, "body") {
                    body.extend(self.block(b));
                }
                let else_body = field(n, "alternative")
                    .map(|b| self.block(b))
                    .unwrap_or_default();
                out.push(op(n, OpKind::Loop { body, else_body }));
            }
            "with_statement" => {
                let mut cleanup = vec![];
                let mut body = vec![];
                for clause in children(n) {
                    if clause.kind() == "with_clause" {
                        for item in children(clause) {
                            let value = field(item, "value").unwrap_or(item);
                            let (expr, alias) = if value.kind() == "as_pattern" {
                                (children(value)[0], field(value, "alias"))
                            } else {
                                (value, None)
                            };
                            let v = self.expr(expr, &mut body);
                            if let Some(a) = alias {
                                let a = if a.kind() == "as_pattern_target" {
                                    children(a).first().copied().unwrap_or(a)
                                } else {
                                    a
                                };
                                self.bind(a, &v, &mut body);
                            }
                            if matches!(self.kinds.get(&v), Some(Kind::Resource | Kind::File)) {
                                cleanup.push(op(item, OpKind::Close { value: v }));
                            } else {
                                self.unknown(
                                    item,
                                    "Unknown context-manager enter/exit effects",
                                    &mut body,
                                );
                            }
                        }
                    }
                }
                if let Some(b) = field(n, "body") {
                    body.extend(self.block(b));
                }
                out.push(op(n, OpKind::Scope { body, cleanup }));
            }
            "try_statement" => {
                let body = field(n, "body").map(|b| self.block(b)).unwrap_or_default();
                let mut handlers = vec![];
                let mut else_body = vec![];
                let mut finally_body = vec![];
                for c in children(n) {
                    match c.kind() {
                        "except_clause" | "except_group_clause" => {
                            let mut h = vec![];
                            for b in children(c) {
                                if b.kind() == "block" {
                                    h.extend(self.block(b));
                                }
                            }
                            handlers.push(h);
                        }
                        "else_clause" => self.stmt(c, &mut else_body),
                        "finally_clause" => self.stmt(c, &mut finally_body),
                        _ => {}
                    }
                }
                out.push(op(
                    n,
                    OpKind::Try {
                        body,
                        handlers,
                        else_body,
                        finally_body,
                    },
                ));
            }
            "return_statement" => {
                let value = children(n).first().map(|x| self.expr(*x, out));
                out.push(op(n, OpKind::Return { value }));
            }
            "raise_statement" => {
                for c in children(n) {
                    self.expr(c, out);
                }
                out.push(op(n, OpKind::Raise));
            }
            "break_statement" => out.push(op(n, OpKind::Break)),
            "continue_statement" => out.push(op(n, OpKind::Continue)),
            "pass_statement" | "comment" => {}
            _ => {
                self.unknown(n, format!("Unsupported statement: {}", n.kind()), out);
                for c in children(n) {
                    if c.kind() == "block" {
                        out.extend(self.block(c));
                    } else {
                        self.expr(c, out);
                    }
                }
            }
        }
    }
}
fn visit<'a>(n: Node<'a>, kind: &str, out: &mut Vec<Node<'a>>) {
    if n.kind() == kind {
        out.push(n);
    }
    for c in children(n) {
        visit(c, kind, out);
    }
}
fn parameters(n: Node<'_>, source: &str) -> Vec<String> {
    field(n, "parameters")
        .map(children)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| {
            if p.kind() == "identifier" {
                Some(source[p.byte_range()].into())
            } else {
                field(p, "name")
                    .map(|x| source[x.byte_range()].into())
                    .or_else(|| {
                        children(p)
                            .into_iter()
                            .find(|x| x.kind() == "identifier")
                            .map(|x| source[x.byte_range()].into())
                    })
            }
        })
        .collect()
}
pub fn lower_source(path: &str, source: &str, config: &Config) -> Result<Vec<Unit>, String> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .map_err(|e| e.to_string())?;
    let tree = parser.parse(source, None).ok_or("Python parser failed")?;
    let root = tree.root_node();
    if root.has_error() {
        let mut errors = vec![];
        fn errors_at(n: Node<'_>, out: &mut Vec<String>) {
            if n.is_error() || n.is_missing() {
                out.push(format!(
                    "{}:{} {}",
                    n.start_position().row + 1,
                    n.start_position().column + 1,
                    n.kind()
                ));
            }
            for c in children(n) {
                errors_at(c, out);
            }
        }
        errors_at(root, &mut errors);
        return Err(format!("{path}: Python parse error: {}", errors.join(", ")));
    }
    let mut f = Front {
        source,
        config,
        imports: BTreeMap::new(),
        shadow: BTreeSet::new(),
        kinds: BTreeMap::new(),
        summaries: BTreeMap::new(),
        counter: 0,
    };
    for c in children(root) {
        if matches!(c.kind(), "import_statement" | "import_from_statement") {
            f.import(c);
        }
    }
    let mut funcs = vec![];
    visit(root, "function_definition", &mut funcs);
    for fun in funcs
        .iter()
        .filter(|n| n.parent().is_some_and(|p| p.kind() == "module"))
    {
        let name = field(*fun, "name").map(|n| f.text(n).to_string()).unwrap();
        let params = parameters(*fun, source);
        let body = field(*fun, "body").unwrap();
        let mut c = Contract::default();
        let mut calls = vec![];
        visit(body, "call", &mut calls);
        let mut assignments = vec![];
        visit(body, "assignment", &mut assignments);
        let mut returns = vec![];
        visit(body, "return_statement", &mut returns);
        for (i, p) in params.iter().enumerate() {
            let mut writes = false;
            for call in &calls {
                let callee = field(*call, "function").unwrap();
                if callee.kind() == "attribute"
                    && field(callee, "object").is_some_and(|o| f.text(o) == p)
                    && field(callee, "attribute").is_some_and(|a| {
                        matches!(
                            f.text(a),
                            "append"
                                | "extend"
                                | "pop"
                                | "clear"
                                | "update"
                                | "add"
                                | "remove"
                                | "insert"
                                | "sort"
                                | "reverse"
                        )
                    })
                {
                    writes = true;
                }
            }
            for a in &assignments {
                if let Some(l) = field(*a, "left")
                    && l.kind() == "subscript"
                    && children(l).first().is_some_and(|o| f.text(*o) == p)
                {
                    writes = true;
                }
            }
            if writes {
                c.mutable.push(i);
            } else if !calls.iter().any(|call| {
                let callee = field(*call, "function").unwrap();
                let receiver_unknown = callee.kind() == "attribute"
                    && field(callee, "object").is_some_and(|o| f.text(o) == p);
                let passed = field(*call, "arguments").is_some_and(|a| {
                    children(a)
                        .iter()
                        .any(|arg| arg.kind() == "identifier" && f.text(*arg) == p)
                });
                receiver_unknown || passed
            }) {
                c.readonly.push(i);
            }
            if !returns.is_empty()
                && returns.iter().all(|r| {
                    children(*r)
                        .first()
                        .is_some_and(|x| x.kind() == "identifier" && f.text(*x) == p)
                })
            {
                c.returns_alias = Some(i);
            }
        }
        // Infer purity only for a single scalar literal return; no overloaded operators or hidden calls.
        let stmts: Vec<_> = children(body)
            .into_iter()
            .filter(|n| n.kind() != "comment")
            .collect();
        c.pure = stmts.len() == 1
            && stmts[0].kind() == "return_statement"
            && children(stmts[0]).first().is_some_and(|n| {
                matches!(
                    n.kind(),
                    "integer" | "float" | "string" | "true" | "false" | "none"
                )
            });
        f.summaries.insert(name, c);
    }
    for c in children(root) {
        if matches!(c.kind(), "function_definition" | "class_definition")
            && let Some(n) = field(c, "name")
        {
            f.shadow.insert(f.text(n).into());
        }
    }
    let module_shadow = f.shadow.clone();
    let module_body = f.block(root);
    let mut units = vec![Unit {
        name: "<module>".into(),
        path: path.into(),
        body: module_body,
    }];
    let imports = f.imports.clone();
    let summaries = f.summaries.clone();
    for fun in funcs {
        f.imports = imports.clone();
        f.shadow = module_shadow.clone();
        f.kinds.clear();
        f.summaries = summaries.clone();
        let mut body = vec![];
        for p in parameters(fun, source) {
            f.shadow.insert(p.clone());
            body.push(op(
                fun,
                OpKind::New {
                    target: p,
                    resource: false,
                },
            ));
        } // Python function locals shadow builtins throughout the function.
        let b = field(fun, "body").unwrap();
        let mut assigns = vec![];
        visit(b, "assignment", &mut assigns);
        for a in assigns {
            if let Some(l) = field(a, "left")
                && l.kind() == "identifier"
            {
                f.shadow.insert(f.text(l).into());
            }
        }
        body.extend(f.block(b));
        units.push(Unit {
            name: field(fun, "name").map(|n| f.text(n).to_string()).unwrap(),
            path: path.into(),
            body,
        });
    }
    Ok(units)
}
