//! Language-neutral, path-sensitive analysis of the structured semantic IR.
//!
//! Paths are kept separate rather than merging unrelated object identities. Bounded
//! exploration is explicit in coverage: this is not a proof for arbitrary programs.
use crate::ir::*;
use std::collections::{BTreeMap, BTreeSet};

type Id = usize;
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
struct Binding {
    object: Id,
    loan: Option<Id>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Object {
    resource: bool,
    moved: Option<Span>,
    closed: Option<Span>,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct Loan {
    parent: Option<Id>,
    object: Id,
    mutable: bool,
    span: Span,
}
#[derive(Clone, Debug, PartialEq, Eq)]
enum Flow {
    Normal,
    Return(Option<Binding>, Span),
    Raise,
    Break,
    Continue,
}
#[derive(Clone, Debug, PartialEq, Eq)]
struct State {
    bindings: BTreeMap<String, Binding>,
    objects: BTreeMap<Id, Object>,
    loans: BTreeMap<Id, Loan>,
    flow: Flow,
    next_id: usize,
    pending_returns: Vec<Binding>,
}
impl Default for State {
    fn default() -> Self {
        Self {
            bindings: BTreeMap::new(),
            objects: BTreeMap::new(),
            loans: BTreeMap::new(),
            flow: Flow::Normal,
            next_id: 0,
            pending_returns: Vec::new(),
        }
    }
}
impl State {
    fn id(&mut self) -> Id {
        self.next_id += 1;
        self.next_id
    }
    fn permits(&self, mut own: Option<Id>, candidate: Id) -> bool {
        while let Some(id) = own {
            if id == candidate {
                return true;
            }
            own = self.loans.get(&id).and_then(|loan| loan.parent);
        }
        false
    }
    fn expire(&mut self, live: &BTreeSet<String>) {
        let live_loans: BTreeSet<_> = self
            .bindings
            .iter()
            .filter(|(n, _)| live.contains(*n))
            .filter_map(|(_, b)| b.loan)
            .collect();
        self.loans.retain(|id, _| live_loans.contains(id));
    }
}

pub fn analyze(unit: &Unit) -> Analysis {
    let mut runner = Runner {
        unit,
        result: Analysis::default(),
        remaining_steps: 10_000,
    };
    runner.seq(&unit.body, vec![State::default()], &BTreeSet::new());
    runner
        .result
        .diagnostics
        .sort_by(|a, b| (&a.span, &a.rule, &a.message).cmp(&(&b.span, &b.rule, &b.message)));
    runner
        .result
        .coverage
        .sort_by(|a, b| (&a.span, &a.reason).cmp(&(&b.span, &b.reason)));
    runner.result
}
struct Runner<'a> {
    unit: &'a Unit,
    result: Analysis,
    remaining_steps: usize,
}
impl Runner<'_> {
    fn gap(&mut self, span: Span, reason: impl Into<String>) {
        let gap = CoverageGap {
            path: self.unit.path.clone(),
            span,
            reason: reason.into(),
        };
        if !self.result.coverage.contains(&gap) {
            self.result.coverage.push(gap);
        }
    }
    fn emit(&mut self, span: Span, rule: &str, message: String, origin: Option<Span>) {
        let d = Diagnostic {
            path: self.unit.path.clone(),
            rule: rule.into(),
            message,
            span,
            confidence: Confidence::Definite,
            notes: origin
                .map(|span| Note {
                    message: "Relevant earlier operation".into(),
                    span,
                })
                .into_iter()
                .collect(),
        };
        // A batch consolidates these events, including their path confidence.
        self.result.diagnostics.push(d);
    }
    fn binding(&mut self, state: &State, value: &str, span: Span) -> Option<Binding> {
        match state.bindings.get(value) {
            Some(b) => Some(b.clone()),
            None => {
                self.gap(span, format!("Object identity of `{value}` is unresolved"));
                None
            }
        }
    }
    fn valid(
        &mut self,
        state: &State,
        binding: &Binding,
        value: &str,
        span: Span,
        moving: bool,
        inspect: bool,
    ) {
        if binding
            .loan
            .is_some_and(|id| !state.loans.contains_key(&id))
        {
            self.emit(
                span,
                "LIFE001",
                format!("Borrow handle `{value}` is used after its borrow ended"),
                None,
            );
        }
        if let Some(obj) = state.objects.get(&binding.object) {
            if let Some(origin) = obj.moved {
                self.emit(
                    span,
                    if moving { "OWN002" } else { "OWN001" },
                    format!("`{value}` refers to an object whose ownership was transferred"),
                    Some(origin),
                );
            }
            if let Some(origin) = obj.closed.filter(|_| !inspect) {
                self.emit(
                    span,
                    "LIFE001",
                    format!("`{value}` refers to a closed resource"),
                    Some(origin),
                );
            }
        }
    }
    fn access(
        &mut self,
        state: &State,
        b: &Binding,
        value: &str,
        span: Span,
        write: bool,
        inspect: bool,
    ) {
        self.valid(state, b, value, span, false, inspect);
        let shared_write = write
            && b.loan
                .and_then(|id| state.loans.get(&id))
                .is_some_and(|l| !l.mutable);
        if shared_write {
            self.emit(
                span,
                "BOR002",
                format!("Cannot modify `{value}` through a shared borrow"),
                b.loan.and_then(|id| state.loans.get(&id)).map(|l| l.span),
            );
        }
        if let Some((_, loan)) = state.loans.iter().find(|(id, l)| {
            l.object == b.object && !state.permits(b.loan, **id) && (write || l.mutable)
        }) {
            self.emit(
                span,
                "BOR002",
                format!(
                    "Access to `{value}` conflicts with a live {} borrow",
                    if loan.mutable { "exclusive" } else { "shared" }
                ),
                Some(loan.span),
            );
        }
    }
    fn escape(&mut self, state: &State, b: &Binding, value: &str, span: Span) {
        self.access(state, b, value, span, false, false);
        if b.loan.is_some() {
            self.gap(
                span,
                format!("Lifetime of escaping borrow `{value}` needs a caller contract"),
            );
        }
    }
    fn seq(&mut self, ops: &[Op], mut states: Vec<State>, after: &BTreeSet<String>) -> Vec<State> {
        for (i, op) in ops.iter().enumerate() {
            if self.remaining_steps == 0 || !states.iter().any(|s| s.flow == Flow::Normal) {
                break;
            }
            let mut live_after = after.clone();
            names(&ops[i + 1..], &mut live_after);
            states = self.batch(op, states, &live_after);
            dedup(&mut states);
            if states.len() > 128 {
                self.gap(
                    op.span,
                    "Path budget exceeded (128 states); remaining paths are unverified",
                );
                states.truncate(128);
            }
        }
        states
    }
    fn batch(&mut self, op: &Op, states: Vec<State>, after: &BTreeSet<String>) -> Vec<State> {
        let count = states.iter().filter(|s| s.flow == Flow::Normal).count();
        let mut events: Vec<(Diagnostic, usize)> = Vec::new();
        let mut out = Vec::new();
        for mut state in states {
            if state.flow != Flow::Normal {
                out.push(state);
                continue;
            }
            if self.remaining_steps == 0 {
                out.push(state);
                continue;
            }
            self.remaining_steps -= 1;
            if self.remaining_steps == 0 {
                self.gap(op.span, "Semantic step budget exhausted (10000); remaining control-flow paths are unverified");
                out.push(state);
                continue;
            }
            let mut live = after.clone();
            names(std::slice::from_ref(op), &mut live);
            state.expire(&live);
            let start = self.result.diagnostics.len();
            out.extend(self.step(op, state, after));
            let mut seen = BTreeSet::new();
            for d in self.result.diagnostics.drain(start..) {
                let key = (d.span, d.rule.clone(), d.message.clone());
                if !seen.insert(key) {
                    continue;
                }
                if let Some((existing, n)) = events
                    .iter_mut()
                    .find(|(e, _)| e.span == d.span && e.rule == d.rule && e.message == d.message)
                {
                    *n += 1;
                    if d.confidence == Confidence::Possible {
                        existing.confidence = Confidence::Possible;
                    }
                } else {
                    events.push((d, 1));
                }
            }
        }
        for (mut d, n) in events {
            if n < count {
                d.confidence = Confidence::Possible;
            }
            if let Some(old) = self
                .result
                .diagnostics
                .iter_mut()
                .find(|e| e.span == d.span && e.rule == d.rule && e.message == d.message)
            {
                if d.confidence == Confidence::Possible {
                    old.confidence = Confidence::Possible;
                }
            } else {
                self.result.diagnostics.push(d);
            }
        }
        out
    }
    fn step(&mut self, op: &Op, mut s: State, after: &BTreeSet<String>) -> Vec<State> {
        let span = op.span;
        match &op.kind {
            OpKind::New { target, resource } => {
                let id = s.id();
                s.objects.insert(
                    id,
                    Object {
                        resource: *resource,
                        moved: None,
                        closed: None,
                    },
                );
                s.bindings.insert(
                    target.clone(),
                    Binding {
                        object: id,
                        loan: None,
                    },
                );
            }
            OpKind::Alias { target, source } => {
                if let Some(b) = self.binding(&s, source, span) {
                    self.valid(&s, &b, source, span, false, false);
                    s.bindings.insert(target.clone(), b);
                } else {
                    s.bindings.remove(target);
                }
            }
            OpKind::Use { value } | OpKind::Inspect { value } | OpKind::Write { value, .. } => {
                if let Some(b) = self.binding(&s, value, span) {
                    self.access(
                        &s,
                        &b,
                        value,
                        span,
                        matches!(op.kind, OpKind::Write { .. }),
                        matches!(op.kind, OpKind::Inspect { .. }),
                    );
                }
            }
            OpKind::Move { value } | OpKind::Close { value } => {
                if let Some(b) = self.binding(&s, value, span) {
                    let moving = matches!(op.kind, OpKind::Move { .. });
                    // Closing twice is not universally invalid; later resource use is.
                    if moving {
                        self.valid(&s, &b, value, span, true, false);
                    } else if s.objects.get(&b.object).is_some_and(|o| o.moved.is_some()) {
                        self.valid(&s, &b, value, span, false, false);
                    }
                    if let Some(loan) = s.loans.values().find(|l| l.object == b.object) {
                        self.emit(
                            span,
                            "BOR003",
                            format!(
                                "Cannot {} `{value}` while it is borrowed",
                                if moving { "transfer" } else { "close" }
                            ),
                            Some(loan.span),
                        );
                    }
                    if let Some(obj) = s.objects.get_mut(&b.object) {
                        if moving {
                            obj.moved = Some(span);
                        } else {
                            obj.closed = Some(span);
                        }
                    }
                }
            }
            OpKind::Borrow {
                target,
                source,
                mutable,
            } => {
                if let Some(b) = self.binding(&s, source, span) {
                    self.valid(&s, &b, source, span, false, false);
                    let parent_shared = *mutable
                        && b.loan
                            .and_then(|id| s.loans.get(&id))
                            .is_some_and(|l| !l.mutable);
                    if parent_shared {
                        self.emit(
                            span,
                            "BOR001",
                            format!("Cannot exclusively borrow `{source}` through a shared borrow"),
                            None,
                        );
                    }
                    if let Some((_, loan)) = s.loans.iter().find(|(id, l)| {
                        l.object == b.object && !s.permits(b.loan, **id) && (*mutable || l.mutable)
                    }) {
                        self.emit(
                            span,
                            "BOR001",
                            format!("New borrow of `{source}` conflicts with a live borrow"),
                            Some(loan.span),
                        );
                    }
                    let id = s.id();
                    s.loans.insert(
                        id,
                        Loan {
                            parent: b.loan,
                            object: b.object,
                            mutable: *mutable,
                            span,
                        },
                    );
                    s.bindings.insert(
                        target.clone(),
                        Binding {
                            object: b.object,
                            loan: Some(id),
                        },
                    );
                } else {
                    s.bindings.remove(target);
                }
            }
            OpKind::EndBorrow { target } => {
                if let Some(id) = s.bindings.get(target).and_then(|b| b.loan) {
                    s.loans.remove(&id);
                }
            }
            OpKind::Escape { value } => {
                if let Some(b) = self.binding(&s, value, span) {
                    self.escape(&s, &b, value, span);
                }
            }
            OpKind::MustUse { callee } => self.emit(
                span,
                "ERR001",
                format!("Required return value of `{callee}` is discarded"),
                None,
            ),
            OpKind::Unknown { reason } => self.gap(span, reason.clone()),
            OpKind::Branch {
                then_body,
                else_body,
            } => {
                let mut out = self.seq(then_body, vec![s.clone()], after);
                out.extend(self.seq(else_body, vec![s], after));
                return out;
            }
            OpKind::Scope { body, cleanup } => {
                let mut live = after.clone();
                names(cleanup, &mut live);
                let paths = self.seq(body, vec![s], &live);
                return self.cleanup(cleanup, paths, after);
            }
            OpKind::Try {
                body,
                handlers,
                else_body,
                finally_body,
            } => {
                if !handlers.is_empty() {
                    self.gap(span, "Exception dispatch and partial-expression failure states are conservatively approximated");
                }
                let mut live = after.clone();
                names(finally_body, &mut live);
                names(else_body, &mut live);
                for h in handlers {
                    names(h, &mut live);
                }
                let mut prefixes = vec![s.clone()];
                let mut paths = vec![s];
                for (i, item) in body.iter().enumerate() {
                    if self.remaining_steps == 0 {
                        break;
                    }
                    let mut suffix = live.clone();
                    names(&body[i + 1..], &mut suffix);
                    paths = self.batch(item, paths, &suffix);
                    prefixes.extend(
                        paths
                            .iter()
                            .filter(|s| matches!(s.flow, Flow::Normal | Flow::Raise))
                            .cloned(),
                    );
                    dedup(&mut prefixes);
                    if prefixes.len() > 128 {
                        self.gap(
                            span,
                            "Exception path budget exceeded; remaining paths are unverified",
                        );
                        prefixes.truncate(128);
                    }
                }
                dedup(&mut prefixes);
                let mut normal = Vec::new();
                let mut exits = Vec::new();
                for p in paths {
                    if p.flow == Flow::Normal {
                        normal.push(p);
                    } else {
                        exits.push(p);
                    }
                }
                let mut finally_live = after.clone();
                names(finally_body, &mut finally_live);
                exits.extend(self.seq(else_body, normal, &finally_live));
                for h in handlers {
                    let entry = prefixes
                        .iter()
                        .cloned()
                        .map(|mut p| {
                            p.flow = Flow::Normal;
                            p
                        })
                        .collect();
                    exits.extend(self.seq(h, entry, &finally_live));
                }
                dedup(&mut exits);
                return self.cleanup(finally_body, exits, after);
            }
            OpKind::Loop { body, else_body } => {
                let mut live = after.clone();
                names(body, &mut live);
                names(else_body, &mut live);
                let mut heads = vec![s.clone()];
                let mut seen = vec![s.clone()];
                let mut exhausted = vec![s];
                let mut exits = Vec::new();
                for iteration in 0..8 {
                    if self.remaining_steps == 0 {
                        break;
                    }
                    let paths = self.seq(body, heads, &live);
                    let mut next = Vec::new();
                    for mut p in paths {
                        match p.flow {
                            Flow::Break => {
                                p.flow = Flow::Normal;
                                exits.push(p);
                            }
                            Flow::Normal | Flow::Continue => {
                                p.flow = Flow::Normal;
                                normalize(&mut p);
                                exhausted.push(p.clone());
                                if !seen.contains(&p) {
                                    seen.push(p.clone());
                                    next.push(p);
                                }
                            }
                            _ => exits.push(p),
                        }
                    }
                    dedup(&mut next);
                    if next.is_empty() {
                        break;
                    }
                    if iteration == 7 || next.len() > 128 {
                        self.gap(span,"Loop state did not converge within analysis budget; further iterations are unverified");
                        break;
                    }
                    heads = next;
                }
                dedup(&mut exhausted);
                if exhausted.len() > 128 {
                    self.gap(
                        span,
                        "Loop exit path budget exceeded; remaining paths are unverified",
                    );
                    exhausted.truncate(128);
                }
                exits.extend(self.seq(else_body, exhausted, after));
                return exits;
            }
            OpKind::Return { value } => {
                let b = value.as_ref().and_then(|v| self.binding(&s, v, span));
                if let (Some(v), Some(b)) = (value, b.as_ref()) {
                    self.escape(&s, b, v, span);
                }
                s.flow = Flow::Return(b, span);
            }
            OpKind::Raise => s.flow = Flow::Raise,
            OpKind::Break => s.flow = Flow::Break,
            OpKind::Continue => s.flow = Flow::Continue,
        }
        vec![s]
    }
    fn cleanup(&mut self, ops: &[Op], paths: Vec<State>, after: &BTreeSet<String>) -> Vec<State> {
        let mut out = Vec::new();
        for mut path in paths {
            let pending = path.flow.clone();
            if let Flow::Return(Some(b), _) = &pending {
                path.pending_returns.push(b.clone());
            }
            path.flow = Flow::Normal;
            for mut p in self.seq(ops, vec![path], after) {
                let mut restored = pending.clone();
                if let Flow::Return(Some(b), _) = &mut restored {
                    *b = p.pending_returns.pop().expect("pending return anchor");
                }
                if p.flow == Flow::Normal {
                    if let Flow::Return(Some(b), return_span) = &restored
                        && let Some(origin) = p.objects.get(&b.object).and_then(|o| o.closed)
                    {
                        self.emit(*return_span,"LIFE002","Returned resource is closed by scope cleanup before the caller can use it".into(),Some(origin));
                    }
                    p.flow = restored;
                }
                out.push(p);
            }
        }
        out
    }
}
fn normalize(s: &mut State) {
    // Object identities are relational, not allocation counters. Canonicalization
    // preserves alias partitions while allowing an overwritten loop allocation to
    // converge. Retained older allocations stay distinct through their aliases.
    let mut objects = BTreeMap::new();
    let mut loans = BTreeMap::new();
    let mut next = 0;
    let mut visit = |b: &Binding| {
        objects.entry(b.object).or_insert_with(|| {
            next += 1;
            next
        });
        if let Some(id) = b.loan {
            loans.entry(id).or_insert_with(|| {
                next += 1;
                next
            });
        }
    };
    for b in s.bindings.values() {
        visit(b);
    }
    for b in &s.pending_returns {
        visit(b);
    }
    if let Flow::Return(Some(b), _) = &s.flow {
        visit(b);
    }
    for loan in s.loans.values() {
        objects.entry(loan.object).or_insert_with(|| {
            next += 1;
            next
        });
    }
    s.objects = s
        .objects
        .iter()
        .filter_map(|(id, o)| objects.get(id).map(|new| (*new, o.clone())))
        .collect();
    s.loans = s
        .loans
        .iter()
        .filter_map(|(id, l)| {
            loans.get(id).map(|new| {
                (
                    *new,
                    Loan {
                        parent: l.parent.and_then(|id| loans.get(&id).copied()),
                        object: objects[&l.object],
                        mutable: l.mutable,
                        span: l.span,
                    },
                )
            })
        })
        .collect();
    let remap = |b: &mut Binding| {
        b.object = objects[&b.object];
        b.loan = b.loan.map(|id| loans[&id]);
    };
    for b in s.bindings.values_mut() {
        remap(b);
    }
    for b in &mut s.pending_returns {
        remap(b);
    }
    if let Flow::Return(Some(b), _) = &mut s.flow {
        remap(b);
    }
    s.next_id = next;
}
fn dedup(states: &mut Vec<State>) {
    let mut out = Vec::new();
    for mut s in states.drain(..) {
        normalize(&mut s);
        if !out.contains(&s) {
            out.push(s);
        }
    }
    *states = out;
}
// Backward name liveness ends unused loans, including when a handle is
// overwritten. Structured branches join live-in names conservatively.
fn names(ops: &[Op], into: &mut BTreeSet<String>) {
    for op in ops.iter().rev() {
        match &op.kind {
            OpKind::New { target, .. } => {
                into.remove(target);
            }
            OpKind::EndBorrow { .. }
            | OpKind::MustUse { .. }
            | OpKind::Unknown { .. }
            | OpKind::Raise
            | OpKind::Break
            | OpKind::Continue => {}
            OpKind::Alias { target, source } | OpKind::Borrow { target, source, .. } => {
                into.remove(target);
                into.insert(source.clone());
            }
            OpKind::Use { value }
            | OpKind::Inspect { value }
            | OpKind::Write { value, .. }
            | OpKind::Move { value }
            | OpKind::Close { value }
            | OpKind::Escape { value } => {
                into.insert(value.clone());
            }
            OpKind::Return { value } => {
                into.extend(value.clone());
            }
            OpKind::Branch {
                then_body,
                else_body,
            } => {
                let mut other = into.clone();
                names(then_body, into);
                names(else_body, &mut other);
                into.extend(other);
            }
            OpKind::Loop { body, else_body } => {
                names(else_body, into);
                loop {
                    let mut head = into.clone();
                    names(body, &mut head);
                    let old = into.len();
                    into.extend(head);
                    if into.len() == old {
                        break;
                    }
                }
            }
            OpKind::Scope { body, cleanup } => {
                names(cleanup, into);
                names(body, into);
            }
            OpKind::Try {
                body,
                handlers,
                else_body,
                finally_body,
            } => {
                names(finally_body, into);
                let exit = into.clone();
                names(else_body, into);
                for h in handlers {
                    let mut entry = exit.clone();
                    names(h, &mut entry);
                    into.extend(entry);
                }
                names(body, into);
            }
        }
    }
}
