//! Resource validity over a finite CFG domain. Calls instantiate parameter effects
//! in the caller's resource graph; recursive cycles conservatively lose precision.
//!
//! Domain: each place maps to a may-points-to set plus an unknown-identity bit;
//! each allocation site maps to a subset of {open, closed, unknown-effect}.
//! Joins use set union. A singleton closed set supports a definite diagnosis,
//! open+closed supports a possible diagnosis, and unknown blocks verification.
//! Repeated allocation sites summarize multiple concrete objects, so close uses
//! weak updates for those sites and for multi-target aliases. Strong close is
//! reserved for singleton concrete identities.
//!
//! Calls currently instantiate bodies with actual resource identities (bounded
//! context sensitivity), rather than caching symbolic reusable summaries. This
//! supports resource validity, explicit recipient-capability transfers and loan
//! lifecycle/conflicts. General ownership inference remains unimplemented.
//! Provenance is a canonical set of evidence, not a feasible-path witness.
use crate::v2_ir::{Function, Kind, Place, Program, Site, Terminator};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, VecDeque};

const OPEN: u8 = 1;
const CLOSED: u8 = 2;
const UNKNOWN: u8 = 4;
const AVAILABLE: u8 = 1;
const TRANSFERRED: u8 = 2;
const LIMIT: usize = 2048;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ObligationStatus {
    Verified,
    Violated,
    Unverified,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Certainty {
    Definite,
    Possible,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TraceStep {
    pub site: Site,
    pub message: String,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Obligation {
    pub id: String,
    pub function: String,
    pub site: Site,
    pub operation: String,
    pub object_anchor: String,
    pub status: ObligationStatus,
    pub reason: String,
    pub trace: Vec<TraceStep>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub rule: String,
    pub certainty: Certainty,
    pub obligation_id: String,
    pub site: Site,
    pub message: String,
    pub trace: Vec<TraceStep>,
}
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Analysis {
    pub findings: Vec<Finding>,
    pub obligations: Vec<Obligation>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct Binding {
    objects: BTreeSet<String>,
    capabilities: BTreeSet<String>,
    unknown: bool,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct Resource {
    states: u8,
    summary: bool,
    trace: Vec<TraceStep>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct Capability {
    resource: String,
    states: u8,
    loan: Option<String>,
    trace: Vec<TraceStep>,
}
#[derive(Debug, Clone, PartialEq, Eq)]
struct Loan {
    resource: String,
    parent: Option<String>,
    mutable: bool,
    states: u8, // AVAILABLE = active; TRANSFERRED = ended
    summary: bool,
    trace: Vec<TraceStep>,
}
#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct State {
    places: BTreeMap<Place, Binding>,
    resources: BTreeMap<String, Resource>,
    capabilities: BTreeMap<String, Capability>,
    ownership_mode: bool,
    loan_mode: bool,
    loans: BTreeMap<String, Loan>,
    objects: BTreeMap<String, bool>, // true means an allocation-site summary
    heap: BTreeMap<(String, String), Binding>,
    exceptions: u8,
}

fn add_trace(trace: &mut Vec<TraceStep>, site: &Site, message: &str) {
    let step = TraceStep {
        site: site.clone(),
        message: message.into(),
    };
    if !trace.contains(&step) && trace.len() < 64 {
        trace.push(step);
    }
    // Canonical order guarantees fixed-point equality independent of queue order.
    trace.sort_by(|a, b| {
        (&a.site.path, &a.site.anchor, &a.message).cmp(&(&b.site.path, &b.site.anchor, &b.message))
    });
}
fn missing() -> Binding {
    Binding {
        objects: BTreeSet::new(),
        capabilities: BTreeSet::new(),
        unknown: true,
    }
}
fn binding(state: &State, place: &Place) -> Binding {
    let mut current = state
        .places
        .get(&Place::local(&place.root))
        .cloned()
        .unwrap_or_else(missing);
    for field in &place.projections {
        let mut next = Binding {
            unknown: current.unknown || current.objects.is_empty(),
            ..Default::default()
        };
        for object in &current.objects {
            if !state.objects.contains_key(object) {
                next.unknown = true;
            } else {
                join_binding(
                    &mut next,
                    &state
                        .heap
                        .get(&(object.clone(), field.clone()))
                        .cloned()
                        .unwrap_or_else(missing),
                );
            }
        }
        current = next;
    }
    current
}
fn store_binding(state: &mut State, place: &Place, value: Binding) -> bool {
    let Some((field, parents)) = place.projections.split_last() else {
        state.places.insert(place.clone(), value);
        return true;
    };
    let parent = binding(
        state,
        &Place {
            root: place.root.clone(),
            projections: parents.to_vec(),
        },
    );
    let valid = !parent.unknown
        && !parent.objects.is_empty()
        && parent
            .objects
            .iter()
            .all(|id| state.objects.contains_key(id));
    if !valid {
        // An unresolved receiver can alias any existing heap object. Keep the
        // write visible and invalidate the corresponding field everywhere.
        for id in state.objects.keys() {
            state
                .heap
                .entry((id.clone(), field.clone()))
                .or_insert_with(missing)
                .unknown = true;
        }
    }
    let strong =
        valid && parent.objects.len() == 1 && parent.objects.iter().all(|id| !state.objects[id]);
    for id in parent.objects {
        if state.objects.contains_key(&id) {
            let key = (id, field.clone());
            if strong {
                state.heap.insert(key, value.clone());
            } else {
                join_binding(state.heap.entry(key).or_insert_with(missing), &value);
            }
        }
    }
    valid
}
fn join_binding(a: &mut Binding, b: &Binding) {
    a.objects.extend(b.objects.iter().cloned());
    a.capabilities.extend(b.capabilities.iter().cloned());
    a.unknown |= b.unknown;
}
fn join(a: &mut State, b: &State) -> bool {
    let old = a.clone();
    a.exceptions |= b.exceptions;
    a.ownership_mode |= b.ownership_mode;
    a.loan_mode |= b.loan_mode;
    let loan_ids: BTreeSet<_> = a.loans.keys().chain(b.loans.keys()).cloned().collect();
    for id in loan_ids {
        match (a.loans.get_mut(&id), b.loans.get(&id)) {
            (Some(x), Some(y)) => {
                x.states |= y.states;
                x.summary |= y.summary;
                for t in &y.trace {
                    add_trace(&mut x.trace, &t.site, &t.message);
                }
            }
            (Some(x), None) => x.states |= TRANSFERRED,
            (None, Some(y)) => {
                let mut y = y.clone();
                y.states |= TRANSFERRED;
                a.loans.insert(id, y);
            }
            _ => {}
        }
    }
    for (id, cap) in &b.capabilities {
        if let Some(old) = a.capabilities.get_mut(id) {
            old.states |= cap.states;
            for step in &cap.trace {
                add_trace(&mut old.trace, &step.site, &step.message);
            }
        } else {
            a.capabilities.insert(id.clone(), cap.clone());
        }
    }
    let keys: BTreeSet<_> = a.places.keys().chain(b.places.keys()).cloned().collect();
    for key in keys {
        let mut x = binding(a, &key);
        join_binding(&mut x, &binding(b, &key));
        a.places.insert(key, x);
    }
    for (id, summary) in &b.objects {
        *a.objects.entry(id.clone()).or_default() |= summary;
    }
    let fields: BTreeSet<_> = a.heap.keys().chain(b.heap.keys()).cloned().collect();
    for field in fields {
        let mut value = a.heap.get(&field).cloned().unwrap_or_else(missing);
        join_binding(
            &mut value,
            &b.heap.get(&field).cloned().unwrap_or_else(missing),
        );
        a.heap.insert(field, value);
    }
    for (id, resource) in &b.resources {
        if let Some(x) = a.resources.get_mut(id) {
            x.states |= resource.states;
            x.summary |= resource.summary;
            for t in &resource.trace {
                add_trace(&mut x.trace, &t.site, &t.message);
            }
        } else {
            a.resources.insert(id.clone(), resource.clone());
        }
    }
    *a != old
}
fn taint(state: &mut State, affected: &[Place], site: &Site, reason: &str) {
    let global = affected.is_empty() || affected.iter().any(|p| binding(state, p).unknown);
    let mut reachable: BTreeSet<String> = if global {
        state
            .resources
            .keys()
            .chain(state.objects.keys())
            .cloned()
            .collect()
    } else {
        affected
            .iter()
            .flat_map(|p| binding(state, p).objects)
            .collect()
    };
    loop {
        let mut more = BTreeSet::new();
        let mut unknown = false;
        for ((object, _), value) in &state.heap {
            if reachable.contains(object) {
                more.extend(value.objects.iter().cloned());
                unknown |= value.unknown;
            }
        }
        if unknown {
            more.extend(state.resources.keys().chain(state.objects.keys()).cloned());
        }
        let old = reachable.len();
        reachable.extend(more);
        if old == reachable.len() {
            break;
        }
    }
    for ((object, _), value) in &mut state.heap {
        if reachable.contains(object) {
            value.unknown = true;
        }
    }
    for loan in state.loans.values_mut() {
        if reachable.contains(&loan.resource) {
            loan.states |= UNKNOWN;
            add_trace(&mut loan.trace, site, reason);
        }
    }
    for cap in state.capabilities.values_mut() {
        if reachable.contains(&cap.resource) {
            cap.states |= UNKNOWN;
            add_trace(&mut cap.trace, site, reason);
        }
    }
    for id in reachable {
        if let Some(r) = state.resources.get_mut(&id) {
            r.states |= UNKNOWN;
            add_trace(&mut r.trace, site, reason);
        }
    }
}

struct RunOutcome {
    normal: Option<(State, Binding)>,
    exceptional: Option<State>,
}
struct Solver<'a> {
    program: &'a Program,
    function_index: BTreeMap<String, usize>,
    obligations: BTreeMap<String, Obligation>,
    findings: BTreeMap<String, Finding>,
    steps: usize,
    benign_unknowns: bool,
}
impl Solver<'_> {
    #[allow(clippy::too_many_arguments)]
    fn record(
        &mut self,
        frame: &str,
        function: &str,
        site: &Site,
        operation: &str,
        status: ObligationStatus,
        reason: &str,
        trace: Vec<TraceStep>,
        certainty: Option<Certainty>,
    ) {
        let id = format!("{frame}|{}|{operation}", site.anchor);
        self.obligations.insert(
            id.clone(),
            Obligation {
                id: id.clone(),
                function: function.into(),
                site: site.clone(),
                operation: operation.into(),
                object_anchor: String::new(),
                status,
                reason: reason.into(),
                trace: trace.clone(),
            },
        );
        self.findings.remove(&id);
        if let Some(certainty) = certainty {
            self.findings.insert(
                id.clone(),
                Finding {
                    rule: if operation.starts_with("permission_") {
                        "OWN001"
                    } else if operation.starts_with("loan_") {
                        "BORROW001"
                    } else {
                        "LIFE001"
                    }
                    .into(),
                    certainty,
                    obligation_id: id,
                    site: site.clone(),
                    message: reason.into(),
                    trace,
                },
            );
        }
    }
    fn permission_access(
        &mut self,
        frame: &str,
        function: &str,
        site: &Site,
        operation: &str,
        state: &State,
        place: &Place,
    ) -> bool {
        let b = binding(state, place);
        let mut mask = if b.unknown || b.capabilities.is_empty() {
            UNKNOWN
        } else {
            0
        };
        let mut trace = Vec::new();
        for id in &b.capabilities {
            if let Some(cap) = state.capabilities.get(id) {
                mask |= cap.states;
                for step in &cap.trace {
                    add_trace(&mut trace, &step.site, &step.message);
                }
            } else {
                mask |= UNKNOWN;
            }
        }
        let (status, reason, certainty) = if mask & UNKNOWN != 0 {
            (
                ObligationStatus::Unverified,
                "Access capability or its transfer effects are unresolved",
                None,
            )
        } else if mask & TRANSFERRED != 0 {
            (
                ObligationStatus::Violated,
                "Ownership policy forbids access through a transferred capability",
                Some(if mask == TRANSFERRED {
                    Certainty::Definite
                } else {
                    Certainty::Possible
                }),
            )
        } else {
            (
                ObligationStatus::Verified,
                "Access capability is available under the ownership policy",
                None,
            )
        };
        let op = format!("permission_{operation}");
        self.record(frame, function, site, &op, status, reason, trace, certainty);
        if let Some(o) = self
            .obligations
            .get_mut(&format!("{frame}|{}|{op}", site.anchor))
        {
            o.object_anchor = b.objects.iter().cloned().collect::<Vec<_>>().join("|");
        }
        status == ObligationStatus::Verified
    }
    // Returns a may-set: 1 = permitted, 2 = conflict, 4 = unresolved.
    fn loan_access(
        &mut self,
        frame: &str,
        function: &str,
        site: &Site,
        operation: &str,
        state: &State,
        place: &Place,
    ) -> u8 {
        if !state.loan_mode {
            return AVAILABLE;
        }
        let b = binding(state, place);
        let mut mask = if b.unknown || b.capabilities.is_empty() {
            UNKNOWN
        } else {
            0
        };
        let mut trace = Vec::new();
        let mut conflicts = BTreeSet::new();
        for cap_id in &b.capabilities {
            let Some(cap) = state.capabilities.get(cap_id) else {
                mask |= UNKNOWN;
                continue;
            };
            for t in &cap.trace {
                add_trace(&mut trace, &t.site, &t.message);
            }
            let mut candidate = AVAILABLE;
            let mut ancestors = BTreeSet::new();
            if let Some(id) = &cap.loan {
                if let Some(own) = state.loans.get(id) {
                    candidate = own.states;
                    if own.states & TRANSFERRED != 0 {
                        conflicts.insert("view may already be released");
                    }
                    if own.summary {
                        candidate |= UNKNOWN;
                    }
                    if (!own.mutable && matches!(operation, "write" | "borrow_mut"))
                        || matches!(operation, "close" | "transfer")
                    {
                        conflicts.insert(if matches!(operation, "close" | "transfer") {
                            "borrowed view cannot close or transfer owner authority"
                        } else {
                            "readonly view does not authorize mutable access"
                        });
                        candidate = (candidate & UNKNOWN) | TRANSFERRED;
                    }
                    let mut cursor = Some(id.clone());
                    while let Some(id) = cursor {
                        if !ancestors.insert(id.clone()) {
                            break;
                        }
                        cursor = state.loans.get(&id).and_then(|l| l.parent.clone());
                    }
                    for t in &own.trace {
                        add_trace(&mut trace, &t.site, &t.message);
                    }
                } else {
                    candidate = UNKNOWN;
                }
            }
            for (id, loan) in &state.loans {
                if loan.resource != cap.resource || ancestors.contains(id) {
                    continue;
                }
                if !(loan.mutable || !matches!(operation, "read" | "borrow_shared")) {
                    continue;
                }
                if loan.states & UNKNOWN != 0 {
                    candidate |= UNKNOWN;
                }
                if loan.states & AVAILABLE != 0 {
                    conflicts.insert(if loan.mutable {
                        "competing exclusive loan is active"
                    } else {
                        "shared loan prohibits this mutation or owner operation"
                    });
                    candidate |= TRANSFERRED;
                    if loan.states == AVAILABLE && !loan.summary {
                        candidate &= !AVAILABLE;
                    }
                    for t in &loan.trace {
                        add_trace(&mut trace, &t.site, &t.message);
                    }
                }
            }
            mask |= candidate;
        }
        let (status, reason, certainty) = if mask & UNKNOWN != 0 {
            (
                ObligationStatus::Unverified,
                "Loan identity or effects are unresolved".to_string(),
                None,
            )
        } else if mask & TRANSFERRED != 0 {
            (
                ObligationStatus::Violated,
                format!(
                    "Borrowing policy forbids {operation}: {}",
                    conflicts.into_iter().collect::<Vec<_>>().join("; ")
                ),
                Some(if mask == TRANSFERRED {
                    Certainty::Definite
                } else {
                    Certainty::Possible
                }),
            )
        } else {
            (
                ObligationStatus::Verified,
                "Access is compatible with all modeled loans".to_string(),
                None,
            )
        };
        let op = format!("loan_{operation}");
        self.record(
            frame, function, site, &op, status, &reason, trace, certainty,
        );
        if let Some(o) = self
            .obligations
            .get_mut(&format!("{frame}|{}|{op}", site.anchor))
        {
            o.object_anchor = b.objects.iter().cloned().collect::<Vec<_>>().join("|");
        }
        mask
    }
    fn access(
        &mut self,
        frame: &str,
        function: &str,
        site: &Site,
        operation: &str,
        state: &State,
        place: &Place,
    ) {
        if state.ownership_mode
            && !self.permission_access(frame, function, site, operation, state, place)
        {
            // Do not certify the resource protocol through a capability that
            // no longer authorizes this access (including wrapper finalization).
            return;
        }
        self.loan_access(frame, function, site, operation, state, place);
        let b = binding(state, place);
        let mut mask = if b.unknown || b.objects.is_empty() {
            UNKNOWN
        } else {
            0
        };
        let object_anchor = b.objects.iter().cloned().collect::<Vec<_>>().join("|");
        let mut trace = vec![];
        for id in b.objects {
            if let Some(r) = state.resources.get(&id) {
                mask |= r.states;
                for t in &r.trace {
                    add_trace(&mut trace, &t.site, &t.message);
                }
            } else {
                mask |= UNKNOWN;
            }
        }
        add_trace(&mut trace, site, "resource access");
        let (status, reason, certainty) = if mask & UNKNOWN != 0 {
            (
                ObligationStatus::Unverified,
                "Resource identity or effects are unresolved; validity cannot be proved",
                None,
            )
        } else if mask & CLOSED != 0 {
            (
                ObligationStatus::Violated,
                if mask == CLOSED {
                    "Resource is closed in every represented incoming state"
                } else {
                    "Resource may be closed in the joined incoming states"
                },
                Some(if mask == CLOSED {
                    Certainty::Definite
                } else {
                    Certainty::Possible
                }),
            )
        } else {
            (
                ObligationStatus::Verified,
                "Tracked resource is open on every analyzed incoming path",
                None,
            )
        };
        self.record(
            frame, function, site, operation, status, reason, trace, certainty,
        );
        if let Some(o) = self
            .obligations
            .get_mut(&format!("{frame}|{}|{operation}", site.anchor))
        {
            o.object_anchor = object_anchor;
        }
    }
    fn run(
        &mut self,
        function: &Function,
        initial: State,
        frame: &str,
        stack: &[String],
    ) -> RunOutcome {
        if function.entry >= function.blocks.len() {
            return RunOutcome {
                normal: None,
                exceptional: None,
            };
        }
        let mut exceptional: Option<State> = None;
        let mut incoming: Vec<Option<State>> = vec![None; function.blocks.len()];
        incoming[function.entry] = Some(initial);
        let mut queue = VecDeque::from([function.entry]);
        let mut exits: BTreeMap<usize, (State, Binding)> = BTreeMap::new();
        let mut visits = 0;
        while let Some(index) = queue.pop_front() {
            visits += 1;
            self.steps += 1;
            let block = &function.blocks[index];
            let mut state = incoming[index].clone().expect("queued block has input");
            if visits > LIMIT || self.steps > LIMIT * 16 {
                let site = block
                    .operations
                    .first()
                    .map(|i| i.site.clone())
                    .unwrap_or(Site {
                        path: function.id.clone(),
                        span: Default::default(),
                        anchor: "analysis-limit".into(),
                    });
                taint(&mut state, &[], &site, "CFG analysis limit exceeded");
                for (id, obligation) in &mut self.obligations {
                    if id.starts_with(frame) {
                        obligation.status = ObligationStatus::Unverified;
                        obligation.reason = "Analysis did not reach a fixed point".into();
                        self.findings.remove(id);
                    }
                }
                self.record(
                    frame,
                    &function.id,
                    &site,
                    "analysis_limit",
                    ObligationStatus::Unverified,
                    "CFG analysis limit exceeded",
                    vec![],
                    None,
                );
                exits.insert(
                    index,
                    (
                        state,
                        Binding {
                            unknown: true,
                            ..Default::default()
                        },
                    ),
                );
                break;
            }
            let mut normal_continuation = true;
            for instruction in &block.operations {
                let site = &instruction.site;
                let target = match &instruction.kind {
                    Kind::Acquire { target }
                    | Kind::AllocateObject { target }
                    | Kind::Assign { target, .. }
                    | Kind::Borrow { target, .. }
                    | Kind::TransferTo { target, .. } => Some(target),
                    Kind::Call { target, .. } | Kind::Invoke { target, .. } => target.as_ref(),
                    _ => None,
                };
                if let Some(target) = target
                    && !target.projections.is_empty()
                {
                    let mut parent = target.clone();
                    parent.projections.pop();
                    let b = binding(&state, &parent);
                    if b.unknown
                        || b.objects.is_empty()
                        || b.objects.iter().any(|id| !state.objects.contains_key(id))
                    {
                        store_binding(&mut state, target, missing());
                        if !self.benign_unknowns {
                            taint(&mut state, &[], site, "Unresolved field write receiver");
                            self.record(
                                frame,
                                &function.id,
                                site,
                                "field_store",
                                ObligationStatus::Unverified,
                                "Field write receiver identity is unresolved",
                                vec![],
                                None,
                            );
                        }
                        continue;
                    }
                }
                match &instruction.kind {
                    Kind::LibraryEffect { model } => {
                        self.record(
                            frame,
                            &function.id,
                            site,
                            "library_effect",
                            ObligationStatus::Verified,
                            &format!("Resolved effect model: {model}"),
                            vec![],
                            None,
                        );
                    }
                    Kind::SetException { categories } => {
                        state.exceptions = *categories;
                    }
                    Kind::AllocateObject { target } => {
                        let id = format!("{frame}|object:{}", site.anchor);
                        let repeated = state.objects.contains_key(&id);
                        state.objects.insert(id.clone(), repeated);
                        let valid = store_binding(
                            &mut state,
                            target,
                            Binding {
                                objects: BTreeSet::from([id]),
                                capabilities: BTreeSet::new(),
                                unknown: false,
                            },
                        );
                        self.record(
                            frame,
                            &function.id,
                            site,
                            "allocate_object",
                            if valid {
                                ObligationStatus::Verified
                            } else {
                                ObligationStatus::Unverified
                            },
                            if valid {
                                "Plain object identity allocated"
                            } else {
                                "Allocation destination is unresolved"
                            },
                            vec![],
                            None,
                        );
                    }
                    Kind::Acquire { target } => {
                        let id = format!("{frame}|{}", site.anchor);
                        let r = state.resources.entry(id.clone()).or_insert(Resource {
                            states: 0,
                            summary: false,
                            trace: vec![],
                        });
                        // An allocation site inside a loop can stand for several objects.
                        r.summary |= r.states != 0;
                        r.states |= OPEN;
                        add_trace(&mut r.trace, site, "resource acquired");
                        let cap_id = format!("{id}|initial-capability");
                        let cap = state
                            .capabilities
                            .entry(cap_id.clone())
                            .or_insert(Capability {
                                resource: id.clone(),
                                loan: None,
                                states: 0,
                                trace: vec![],
                            });
                        cap.states |= AVAILABLE;
                        add_trace(&mut cap.trace, site, "initial access capability created");
                        store_binding(
                            &mut state,
                            target,
                            Binding {
                                objects: BTreeSet::from([id]),
                                capabilities: BTreeSet::from([cap_id]),
                                unknown: false,
                            },
                        );
                        self.record(
                            frame,
                            &function.id,
                            site,
                            "acquire",
                            ObligationStatus::Verified,
                            "Resource acquisition is modeled",
                            vec![],
                            None,
                        );
                    }
                    Kind::Assign { target, source } => {
                        let b = binding(&state, source);
                        for id in &b.capabilities {
                            if let Some(cap) = state.capabilities.get_mut(id) {
                                add_trace(
                                    &mut cap.trace,
                                    site,
                                    "alias preserves the same access capability",
                                );
                            }
                        }
                        for id in &b.objects {
                            if let Some(r) = state.resources.get_mut(id) {
                                add_trace(&mut r.trace, site, "alias assigned");
                            }
                        }
                        if !store_binding(&mut state, target, b) {
                            taint(&mut state, &[], site, "Unresolved field write receiver");
                            self.record(
                                frame,
                                &function.id,
                                site,
                                "field_store",
                                ObligationStatus::Unverified,
                                "Field write receiver identity is unresolved",
                                vec![],
                                None,
                            );
                        }
                    }
                    Kind::Read { value } => {
                        self.access(frame, &function.id, site, "read", &state, value)
                    }
                    Kind::Write { value } => {
                        self.access(frame, &function.id, site, "write", &state, value)
                    }
                    Kind::Close { value }
                    | Kind::CloseIfOwned { value }
                    | Kind::CloseIfUnborrowed { value } => {
                        if state.ownership_mode {
                            self.permission_access(
                                frame,
                                &function.id,
                                site,
                                "close",
                                &state,
                                value,
                            );
                        }
                        let loan_mask =
                            self.loan_access(frame, &function.id, site, "close", &state, value);
                        if matches!(instruction.kind, Kind::CloseIfUnborrowed { .. })
                            && loan_mask == TRANSFERRED
                        {
                            continue;
                        }
                        let b = binding(&state, value);
                        let capability_mask = b.capabilities.iter().fold(
                            if b.unknown || b.capabilities.is_empty() {
                                UNKNOWN
                            } else {
                                0
                            },
                            |mask, id| {
                                mask | state.capabilities.get(id).map_or(UNKNOWN, |cap| cap.states)
                            },
                        );
                        let guarded = matches!(instruction.kind, Kind::CloseIfOwned { .. });
                        if guarded && capability_mask == TRANSFERRED {
                            self.record(
                                frame,
                                &function.id,
                                site,
                                "close",
                                ObligationStatus::Verified,
                                "Detached wrapper cannot close its former underlying resource",
                                vec![],
                                None,
                            );
                            continue;
                        }
                        let unverified = (guarded && capability_mask & UNKNOWN != 0)
                            || b.unknown
                            || b.objects.is_empty()
                            || b.objects.iter().any(|id| !state.resources.contains_key(id));
                        let weak = b.objects.len() > 1
                            || (guarded && capability_mask & TRANSFERRED != 0)
                            || (matches!(instruction.kind, Kind::CloseIfUnborrowed { .. })
                                && loan_mask != AVAILABLE);
                        if unverified && self.benign_unknowns {
                            continue;
                        }
                        if unverified {
                            taint(
                                &mut state,
                                &[],
                                site,
                                "Unresolved close receiver may alias reachable resources",
                            );
                        }
                        for id in b.objects {
                            if let Some(r) = state.resources.get_mut(&id) {
                                // Multiple possible targets require a weak update.
                                r.states = if weak || r.summary {
                                    r.states | CLOSED
                                } else {
                                    (r.states & UNKNOWN) | CLOSED
                                };
                                add_trace(&mut r.trace, site, "resource closed");
                            }
                        }
                        self.record(
                            frame,
                            &function.id,
                            site,
                            "close",
                            if unverified {
                                ObligationStatus::Unverified
                            } else {
                                ObligationStatus::Verified
                            },
                            if unverified {
                                "Close receiver is unresolved"
                            } else {
                                "Close effect propagated to resource aliases"
                            },
                            vec![],
                            None,
                        );
                    }
                    Kind::GlobalMutationUnknown { reason } => {
                        if self.benign_unknowns {
                            continue;
                        }
                        taint(&mut state, &[], site, reason);
                        self.record(
                            frame,
                            &function.id,
                            site,
                            "unknown",
                            ObligationStatus::Unverified,
                            reason,
                            vec![],
                            None,
                        );
                    }
                    Kind::Unknown { affected, reason } => {
                        if self.benign_unknowns {
                            continue;
                        }
                        taint(&mut state, affected, site, reason);
                        self.record(
                            frame,
                            &function.id,
                            site,
                            "unknown",
                            ObligationStatus::Unverified,
                            reason,
                            vec![],
                            None,
                        );
                    }
                    Kind::TransferTo { source, target } => {
                        state.ownership_mode = true;
                        let valid = self.permission_access(
                            frame,
                            &function.id,
                            site,
                            "transfer",
                            &state,
                            source,
                        );
                        let loan_valid =
                            self.loan_access(frame, &function.id, site, "transfer", &state, source)
                                == AVAILABLE;
                        let b = binding(&state, source);
                        if !valid || !loan_valid {
                            taint(
                                &mut state,
                                std::slice::from_ref(source),
                                site,
                                "Invalid or unresolved ownership transfer",
                            );
                            store_binding(&mut state, target, missing());
                        } else {
                            let weak = b.capabilities.len() != 1
                                || b.objects.len() != 1
                                || b.objects
                                    .iter()
                                    .any(|id| state.resources.get(id).is_none_or(|r| r.summary));
                            let mut recipient = Binding {
                                objects: b.objects.clone(),
                                ..Default::default()
                            };
                            for id in &b.capabilities {
                                let cap = state.capabilities.get_mut(id).unwrap();
                                cap.states = if weak {
                                    cap.states | TRANSFERRED
                                } else {
                                    TRANSFERRED
                                };
                                add_trace(
                                    &mut cap.trace,
                                    site,
                                    "access capability transferred to recipient",
                                );
                                let resource = cap.resource.clone();
                                let inherited_trace = cap.trace.clone();
                                let new_id =
                                    format!("{frame}|{}|recipient:{resource}", site.anchor);
                                let new_cap = state.capabilities.entry(new_id.clone()).or_insert(
                                    Capability {
                                        resource,
                                        loan: None,
                                        states: 0,
                                        trace: inherited_trace,
                                    },
                                );
                                new_cap.states |= AVAILABLE;
                                add_trace(
                                    &mut new_cap.trace,
                                    site,
                                    "recipient capability created by transfer",
                                );
                                recipient.capabilities.insert(new_id);
                            }
                            store_binding(&mut state, target, recipient);
                        }
                    }
                    Kind::Borrow {
                        source,
                        target,
                        mutable,
                    } => {
                        state.loan_mode = true;
                        state.ownership_mode = true;
                        self.access(
                            frame,
                            &function.id,
                            site,
                            if *mutable {
                                "borrow_mut"
                            } else {
                                "borrow_shared"
                            },
                            &state,
                            source,
                        );
                        let b = binding(&state, source);
                        if b.unknown || b.capabilities.is_empty() {
                            store_binding(&mut state, target, missing());
                        } else {
                            let mut borrowed = Binding {
                                objects: b.objects.clone(),
                                ..Default::default()
                            };
                            for cap_id in &b.capabilities {
                                let Some(cap) = state.capabilities.get(cap_id).cloned() else {
                                    borrowed.unknown = true;
                                    continue;
                                };
                                let id = format!("{frame}|{}|loan:{cap_id}", site.anchor);
                                let uncertain = b.capabilities.len() != 1
                                    || state.resources.get(&cap.resource).is_none_or(|r| r.summary);
                                let parent_unresolved = cap.loan.as_ref().is_some_and(|p| {
                                    state
                                        .loans
                                        .get(p)
                                        .is_none_or(|l| l.states != AVAILABLE || l.summary)
                                });
                                let repeated = state.loans.contains_key(&id);
                                // Repeated sites summarize multiple concrete views; releases must stay weak.
                                let loan = state.loans.entry(id.clone()).or_insert(Loan {
                                    resource: cap.resource.clone(),
                                    parent: cap.loan.clone(),
                                    mutable: *mutable,
                                    states: 0,
                                    summary: uncertain,
                                    trace: cap.trace.clone(),
                                });
                                loan.summary |= repeated || uncertain;
                                loan.states |= AVAILABLE;
                                if uncertain {
                                    loan.states |= TRANSFERRED;
                                }
                                if cap.states != AVAILABLE || parent_unresolved {
                                    loan.states |= UNKNOWN;
                                }
                                add_trace(
                                    &mut loan.trace,
                                    site,
                                    if *mutable {
                                        "exclusive loan created"
                                    } else {
                                        "shared loan created"
                                    },
                                );
                                let new_cap = format!("{id}|view-capability");
                                state.capabilities.insert(
                                    new_cap.clone(),
                                    Capability {
                                        resource: cap.resource,
                                        states: cap.states,
                                        loan: Some(id),
                                        trace: loan.trace.clone(),
                                    },
                                );
                                borrowed.capabilities.insert(new_cap);
                            }
                            store_binding(&mut state, target, borrowed);
                        }
                    }
                    Kind::EndBorrow { value } | Kind::AssumeBorrowEnded { value } => {
                        let assumed = matches!(instruction.kind, Kind::AssumeBorrowEnded { .. });
                        let b = binding(&state, value);
                        let known = !b.unknown
                            && !b.capabilities.is_empty()
                            && b.capabilities.iter().all(|id| {
                                state.capabilities.get(id).is_some_and(|c| {
                                    c.states == AVAILABLE
                                        && c.loan.as_ref().is_some_and(|l| {
                                            state
                                                .loans
                                                .get(l)
                                                .is_some_and(|l| l.states & UNKNOWN == 0)
                                        })
                                })
                            });
                        if known {
                            for id in &b.capabilities {
                                let id = state.capabilities[id].loan.as_ref().unwrap();
                                let loan = state.loans.get_mut(id).unwrap();
                                loan.states = if loan.summary || b.capabilities.len() > 1 {
                                    loan.states | TRANSFERRED
                                } else {
                                    TRANSFERRED
                                };
                                add_trace(
                                    &mut loan.trace,
                                    site,
                                    if assumed {
                                        "protocol failure establishes this view was already ended"
                                    } else {
                                        "loan released; aliases retain the ended view"
                                    },
                                );
                            }
                        } else {
                            taint(
                                &mut state,
                                std::slice::from_ref(value),
                                site,
                                "Release receiver or effects unresolved",
                            );
                        }
                        self.record(frame, &function.id, site, "loan_release", if known { ObligationStatus::Verified } else { ObligationStatus::Unverified }, if known { "Release ends this view, preserving independently derived views and the owner resource" } else { "Release receiver or effects unresolved" }, vec![], None);
                    }
                    Kind::Transfer { .. } => {
                        if self.benign_unknowns {
                            continue;
                        }
                        taint(
                            &mut state,
                            &[],
                            site,
                            "Operation lacks a supported transfer-recipient or loan model",
                        );
                        self.record(
                            frame,
                            &function.id,
                            site,
                            "ownership",
                            ObligationStatus::Unverified,
                            "Operation lacks a supported transfer-recipient or loan model",
                            vec![],
                            None,
                        );
                    }
                    Kind::Call {
                        target,
                        callee,
                        args,
                    }
                    | Kind::Invoke {
                        target,
                        callee,
                        args,
                        ..
                    } => {
                        let unwind = match &instruction.kind {
                            Kind::Invoke { unwind, .. } => Some(*unwind),
                            _ => None,
                        };
                        if let Some(called) = self
                            .function_index
                            .get(callee)
                            .map(|index| &self.program.functions[*index])
                            .cloned()
                            .filter(|called| {
                                !stack.contains(callee)
                                    && stack.len() < 32
                                    && called.params.len() == args.len()
                            })
                        {
                            let bindings: Vec<_> =
                                args.iter().map(|a| binding(&state, a)).collect();
                            let mut local = State {
                                places: BTreeMap::new(),
                                resources: state.resources.clone(),
                                capabilities: state.capabilities.clone(),
                                ownership_mode: state.ownership_mode,
                                loan_mode: state.loan_mode,
                                loans: state.loans.clone(),
                                objects: state.objects.clone(),
                                heap: state.heap.clone(),
                                exceptions: 0,
                            };
                            for (i, p) in called.params.iter().enumerate() {
                                local.places.insert(
                                    p.clone(),
                                    bindings.get(i).cloned().unwrap_or(Binding {
                                        unknown: true,
                                        ..Default::default()
                                    }),
                                );
                            }
                            for b in &bindings {
                                for id in &b.capabilities {
                                    if let Some(cap) = local.capabilities.get_mut(id) {
                                        add_trace(
                                            &mut cap.trace,
                                            site,
                                            &format!("capability passed to {callee}"),
                                        );
                                    }
                                }
                                for id in &b.objects {
                                    if let Some(r) = local.resources.get_mut(id) {
                                        add_trace(
                                            &mut r.trace,
                                            site,
                                            &format!("argument passed to {callee}"),
                                        );
                                    }
                                }
                            }
                            let mut next_stack = stack.to_vec();
                            next_stack.push(callee.clone());
                            let child_frame = format!("{frame}/{}:{callee}", site.anchor);
                            let result = self.run(&called, local, &child_frame, &next_stack);
                            let no_return = result.normal.is_none();
                            let child_may_raise = result.exceptional.is_some();
                            if let Some(exit) = result.exceptional {
                                let mut caller_exception = state.clone();
                                caller_exception.resources = exit.resources;
                                caller_exception.capabilities = exit.capabilities;
                                caller_exception.ownership_mode = exit.ownership_mode;
                                caller_exception.loan_mode = exit.loan_mode;
                                caller_exception.loans = exit.loans;
                                caller_exception.objects = exit.objects;
                                caller_exception.heap = exit.heap;
                                caller_exception.exceptions = exit.exceptions;
                                if let Some(destination) = unwind {
                                    if let Some(slot) = incoming.get_mut(destination) {
                                        let changed = if let Some(old) = slot {
                                            join(old, &caller_exception)
                                        } else {
                                            *slot = Some(caller_exception);
                                            true
                                        };
                                        if changed && !queue.contains(&destination) {
                                            queue.push_back(destination);
                                        }
                                    } else {
                                        self.record(
                                            frame,
                                            &function.id,
                                            site,
                                            "invalid_cfg",
                                            ObligationStatus::Unverified,
                                            "Invoke unwind references a missing block",
                                            vec![],
                                            None,
                                        );
                                    }
                                } else if let Some(old) = &mut exceptional {
                                    join(old, &caller_exception);
                                } else {
                                    exceptional = Some(caller_exception);
                                }
                            }
                            if let Some((exit, returned)) = result.normal {
                                state.resources = exit.resources;
                                state.capabilities = exit.capabilities;
                                state.ownership_mode = exit.ownership_mode;
                                state.loan_mode = exit.loan_mode;
                                state.loans = exit.loans;
                                state.objects = exit.objects;
                                state.heap = exit.heap;
                                if let Some(target) = target {
                                    store_binding(&mut state, target, returned);
                                }
                            } else if !self.benign_unknowns {
                                taint(
                                    &mut state,
                                    args,
                                    site,
                                    "Callee has no modeled normal return",
                                );
                                if let Some(target) = target {
                                    store_binding(
                                        &mut state,
                                        target,
                                        Binding {
                                            unknown: true,
                                            ..Default::default()
                                        },
                                    );
                                }
                            }
                            if child_may_raise && unwind.is_none() && !self.benign_unknowns {
                                // Legacy Call has no exceptional successor. Keep that
                                // coverage gap visible even when a normal return exists.
                                taint(
                                    &mut state,
                                    &[],
                                    site,
                                    "Callee exceptional exit is not dispatched",
                                );
                            }
                            let incomplete = (no_return && !child_may_raise)
                                || (child_may_raise && unwind.is_none())
                                || self.obligations.iter().any(|(key, o)| {
                                    key.starts_with(&child_frame)
                                        && o.status == ObligationStatus::Unverified
                                });
                            // Expose nested gaps at the call site for coverage-sensitive comparison.
                            if !self.benign_unknowns || !incomplete {
                                self.record(
                                    frame,
                                    &function.id,
                                    site,
                                    "call",
                                    if incomplete {
                                        ObligationStatus::Unverified
                                    } else {
                                        ObligationStatus::Verified
                                    },
                                    if child_may_raise && unwind.is_none() {
                                        "Callee exceptional exit is not dispatched"
                                    } else if incomplete {
                                        "Callee contains unverified operations"
                                    } else {
                                        "Resolved callee resource effects propagated"
                                    },
                                    vec![],
                                    None,
                                );
                            }
                            if no_return && child_may_raise {
                                normal_continuation = false;
                                break;
                            }
                        } else {
                            if self.benign_unknowns {
                                if let Some(target) = target {
                                    store_binding(&mut state, target, missing());
                                }
                                continue;
                            }
                            taint(
                                &mut state,
                                &[],
                                site,
                                "Unresolved or recursive call effects",
                            );
                            if let Some(target) = target {
                                store_binding(
                                    &mut state,
                                    target,
                                    Binding {
                                        unknown: true,
                                        ..Default::default()
                                    },
                                );
                            }
                            self.record(
                                frame,
                                &function.id,
                                site,
                                "call",
                                ObligationStatus::Unverified,
                                "Unresolved or recursive call effects",
                                vec![],
                                None,
                            );
                        }
                    }
                }
            }
            if !normal_continuation {
                continue;
            }
            let successors = match &block.terminator {
                Terminator::ExceptionMatch {
                    categories,
                    matched,
                    unmatched,
                    site,
                } => {
                    if state.exceptions & 4 != 0 || state.exceptions == 0 {
                        self.record(
                            frame,
                            &function.id,
                            site,
                            "exception_match",
                            ObligationStatus::Unverified,
                            "Exception category is unknown",
                            vec![],
                            None,
                        );
                        vec![*matched, *unmatched]
                    } else {
                        let mut targets = vec![];
                        if state.exceptions & categories != 0 {
                            targets.push(*matched);
                        }
                        if state.exceptions & !categories != 0 {
                            targets.push(*unmatched);
                        }
                        targets
                    }
                }
                Terminator::Return { value, .. } => {
                    let returned = value
                        .as_ref()
                        .map(|p| binding(&state, p))
                        .unwrap_or_default();
                    exits.insert(index, (state.clone(), returned));
                    vec![]
                }
                Terminator::Stop => {
                    exits.insert(index, (state.clone(), Binding::default()));
                    vec![]
                }
                Terminator::Raise { .. } => {
                    if let Some(old) = &mut exceptional {
                        join(old, &state);
                    } else {
                        exceptional = Some(state.clone());
                    }
                    vec![]
                }
                Terminator::Jump { target } => vec![*target],
                Terminator::Branch {
                    then_target,
                    else_target,
                } => vec![*then_target, *else_target],
            };
            if !successors.is_empty() {
                for successor in successors {
                    if successor >= incoming.len() {
                        let site = Site {
                            path: function.id.clone(),
                            span: Default::default(),
                            anchor: format!("invalid-edge-{index}-{successor}"),
                        };
                        self.record(
                            frame,
                            &function.id,
                            &site,
                            "invalid_cfg",
                            ObligationStatus::Unverified,
                            "CFG edge references a missing block",
                            vec![],
                            None,
                        );
                        continue;
                    }
                    let mut outgoing = state.clone();
                    if let Terminator::ExceptionMatch {
                        categories,
                        matched,
                        ..
                    } = &block.terminator
                        && outgoing.exceptions & 4 == 0
                        && outgoing.exceptions != 0
                    {
                        outgoing.exceptions &= if successor == *matched {
                            *categories
                        } else {
                            !categories
                        };
                    }
                    let changed = if let Some(prior) = incoming[successor].as_mut() {
                        join(prior, &outgoing)
                    } else {
                        incoming[successor] = Some(outgoing);
                        true
                    };
                    if changed && !queue.contains(&successor) {
                        queue.push_back(successor);
                    }
                }
            }
        }
        let mut merged: Option<(State, Binding)> = None;
        for (_, (state, returned)) in exits {
            if let Some((prior, ret)) = merged.as_mut() {
                join(prior, &state);
                join_binding(ret, &returned);
            } else {
                merged = Some((state, returned));
            }
        }
        RunOutcome {
            normal: merged,
            exceptional,
        }
    }
}
/// Analyze entry roots without running target-language code. Unbound root
/// parameters are unknown; resolved callers instantiate those parameters.
pub fn analyze(program: &Program) -> Analysis {
    analyze_with_unknown_policy(program, false)
}

/// Unsound high-precision lint mode. Unsupported and unresolved behavior is a
/// no-op over tracked state, so only findings supported by modeled effects are
/// returned. This function intentionally makes no completeness claim.
pub fn lint(program: &Program) -> Analysis {
    analyze_with_unknown_policy(program, true)
}

fn analyze_with_unknown_policy(program: &Program, benign_unknowns: bool) -> Analysis {
    let function_index: BTreeMap<_, _> = program
        .functions
        .iter()
        .enumerate()
        .map(|(index, function)| (function.id.clone(), index))
        .collect();
    let loan_mode = program
        .functions
        .iter()
        .flat_map(|function| &function.blocks)
        .flat_map(|block| &block.operations)
        .any(|instruction| matches!(instruction.kind, Kind::Borrow { .. }));
    let ownership_mode = loan_mode
        || program
            .functions
            .iter()
            .flat_map(|function| &function.blocks)
            .flat_map(|block| &block.operations)
            .any(|instruction| matches!(instruction.kind, Kind::TransferTo { .. }));
    let mut solver = Solver {
        program,
        function_index,
        obligations: BTreeMap::new(),
        findings: BTreeMap::new(),
        steps: 0,
        benign_unknowns,
    };
    for root in &program.roots {
        if let Some(function) = solver
            .function_index
            .get(root)
            .map(|index| &program.functions[*index])
        {
            if function.entry >= function.blocks.len() {
                let site = Site {
                    path: root.clone(),
                    span: Default::default(),
                    anchor: "invalid-entry".into(),
                };
                solver.record(
                    root,
                    root,
                    &site,
                    "invalid_cfg",
                    ObligationStatus::Unverified,
                    "Function entry references a missing block",
                    vec![],
                    None,
                );
                continue;
            }
            solver.steps = 0;
            // Inventory permission obligations before as well as after transfer,
            // so moving an access before transfer yields an explicit repair proof.
            solver.run(
                function,
                State {
                    ownership_mode,
                    loan_mode,
                    ..Default::default()
                },
                root,
                std::slice::from_ref(root),
            );
        } else {
            let site = Site {
                path: root.clone(),
                span: Default::default(),
                anchor: "missing-root".into(),
            };
            solver.record(
                root,
                root,
                &site,
                "invalid_cfg",
                ObligationStatus::Unverified,
                "Requested root function was not found",
                vec![],
                None,
            );
        }
    }
    Analysis {
        findings: solver.findings.into_values().collect(),
        obligations: solver.obligations.into_values().collect(),
    }
}
