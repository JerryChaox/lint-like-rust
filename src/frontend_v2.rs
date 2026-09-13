//! Closed-subset Python semantic provider and CFG lowering.
//!
//! No Python is executed. Symbol/type facts are a separate input seam: the
//! bundled provider resolves project functions and a narrow set of library
//! types. Unsupported effects become explicit `Unknown` instructions.
use crate::{ir::Span, v2_ir::*};
use std::{
    collections::{BTreeMap, BTreeSet},
    path::{Path, PathBuf},
};
use tree_sitter::{Node, Parser, Tree};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum ValueType {
    Path,
    File,
    BytesBuffer,
    TextWrapper,
    BufferView {
        mutable: bool,
    },
    /// Exact builtin Unicode string, inferred from modeled construction.
    Text,
    /// Recursively builtin JSON-compatible data; excludes subclasses/callbacks.
    JsonData,
    Scalar,
    FileData,
    HashDigest,
    Instance(String),
    #[default]
    Unknown,
}
#[derive(Clone, Debug, Default)]
pub struct FunctionFacts {
    pub parameters: Vec<ValueType>,
    pub returns: ValueType,
}
/// Replaceable semantic-provider output. Keys are resolved `module::function`
/// identities, never bare method names. Unknown facts do not imply purity.
#[derive(Clone, Debug, Default)]
pub struct SemanticFacts {
    pub classes: BTreeSet<String>,
    pub blocked_class_dispatch: BTreeSet<String>,
    pub functions: BTreeMap<String, FunctionFacts>,
    pub fields: BTreeMap<(String, String), ValueType>,
    pub standard_models_blocked: bool,
    /// Explicit CLI roots have no supplied caller values.
    pub unbound_entries: BTreeSet<String>,
    /// Best-effort linting treats unsupported behavior as having no tracked
    /// effect. This is deliberately unsound and must never be enabled by the
    /// proof-oriented `analyze` command.
    pub benign_unknowns: bool,
}

struct Source {
    path: String,
    module: String,
    text: String,
    tree: Tree,
    package: bool,
}
fn kids(n: Node<'_>) -> Vec<Node<'_>> {
    let mut c = n.walk();
    n.named_children(&mut c).collect()
}
fn field<'a>(n: Node<'a>, s: &str) -> Option<Node<'a>> {
    n.child_by_field_name(s)
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
fn text<'a>(s: &'a str, n: Node<'_>) -> &'a str {
    &s[n.byte_range()]
}
// A syntax-derived exact-int fact, not a scalar annotation or an evaluation
// of target Python. Restrict operations to ones closed over builtin integers.
fn exact_integer_expression(n: Node<'_>, source: &str) -> bool {
    match n.kind() {
        "integer" => true,
        "parenthesized_expression" => {
            let children = kids(n);
            children.len() == 1 && exact_integer_expression(children[0], source)
        }
        "unary_operator" => {
            field(n, "operator").is_some_and(|op| matches!(text(source, op), "+" | "-" | "~"))
                && field(n, "argument").is_some_and(|v| exact_integer_expression(v, source))
        }
        "binary_operator" => {
            field(n, "operator")
                .is_some_and(|op| matches!(text(source, op), "+" | "-" | "*" | "&" | "|" | "^"))
                && field(n, "left").is_some_and(|v| exact_integer_expression(v, source))
                && field(n, "right").is_some_and(|v| exact_integer_expression(v, source))
        }
        _ => false,
    }
}
fn common_root(sources: &[(String, String)]) -> PathBuf {
    let mut root = sources
        .first()
        .and_then(|x| Path::new(&x.0).parent())
        .unwrap_or(Path::new(""))
        .to_path_buf();
    for (p, _) in sources {
        while !Path::new(p).starts_with(&root) && root.pop() {}
    }
    root
}
fn parse_sources(sources: &[(String, String)], root: &Path) -> Result<Vec<Source>, String> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .map_err(|e| e.to_string())?;
    let mut parsed = Vec::new();
    for (path, source) in sources {
        let tree = parser
            .parse(source, None)
            .ok_or("Python parser returned no tree")?;
        if tree.root_node().has_error() {
            return Err(format!("{path}: invalid Python syntax"));
        }
        let rel = Path::new(path)
            .strip_prefix(root)
            .map_err(|_| format!("{path}: outside import root {}", root.display()))?;
        let package = rel.file_stem().is_some_and(|s| s == "__init__");
        let mut parts: Vec<String> = rel
            .with_extension("")
            .components()
            .map(|s| s.as_os_str().to_string_lossy().into_owned())
            .collect();
        if package {
            parts.pop();
        }
        let module = if parts.is_empty() {
            "__root__".into()
        } else {
            parts.join(".")
        };
        parsed.push(Source {
            path: path.clone(),
            module,
            text: source.clone(),
            tree,
            package,
        });
    }
    Ok(parsed)
}
#[derive(Debug)]
pub struct Declaration {
    pub path: String,
    pub symbol: String,
    pub name: String,
    pub start: usize,
    pub end: usize,
    pub name_start: usize,
    pub name_end: usize,
}
/// Only definitions admitted by this frontend; a location match grants no dispatch proof.
pub fn declaration_index(
    sources: &[(String, String)],
    root: &Path,
) -> Result<Vec<Declaration>, String> {
    let parsed = parse_sources(sources, root)?;
    let mut out = Vec::new();
    for source in &parsed {
        for (qualified, node) in definitions(source) {
            let name = field(node, "name").unwrap();
            out.push(Declaration {
                path: source.path.clone(),
                symbol: format!("{}::{qualified}", source.module),
                name: text(&source.text, name).into(),
                start: node.start_byte(),
                end: node.end_byte(),
                name_start: name.start_byte(),
                name_end: name.end_byte(),
            });
        }
    }
    Ok(out)
}
pub fn lower_project(sources: &[(String, String)]) -> Result<Program, String> {
    lower_project_with_root(sources, &common_root(sources))
}
/// Explicit import root avoids ambiguity when scanning only part of a package.
pub fn lower_project_with_root(
    sources: &[(String, String)],
    root: &Path,
) -> Result<Program, String> {
    lower_project_with_facts(sources, root, &SemanticFacts::default())
}
pub fn lower_project_with_facts(
    sources: &[(String, String)],
    root: &Path,
    supplied: &SemanticFacts,
) -> Result<Program, String> {
    let parsed = parse_sources(sources, root)?;
    let mut facts = supplied.clone();
    for s in &parsed {
        for n in kids(s.tree.root_node()) {
            if plain_class(s, n) {
                facts.classes.insert(format!(
                    "{}::{}",
                    s.module,
                    text(&s.text, field(n, "name").unwrap())
                ));
            }
        }
        for (name, n) in definitions(s) {
            let id = format!("{}::{name}", s.module);
            facts.functions.entry(id).or_insert_with(|| FunctionFacts {
                parameters: params(s, n).iter().map(|_| ValueType::Unknown).collect(),
                returns: ValueType::Unknown,
            });
        }
    }
    // Infer argument/return facts monotonically. Conflicting contexts widen to
    // Unknown rather than picking whichever call happened to be visited last.
    let mut conflicts = BTreeSet::new();
    for entry in &supplied.unbound_entries {
        if let Some(f) = facts.functions.get_mut(entry) {
            for (i, ty) in f.parameters.iter_mut().enumerate() {
                *ty = ValueType::Unknown;
                conflicts.insert((entry.clone(), Some(i)));
            }
        }
    }
    let mut field_conflicts = BTreeSet::new();
    for _ in 0..16 {
        let (_, observations, field_observations) = lower_all(&parsed, &facts);
        let mut changed = false;
        for (id, index, ty) in observations.clone() {
            if ty == ValueType::Unknown || conflicts.contains(&(id.clone(), index)) {
                continue;
            }
            if let Some(f) = facts.functions.get_mut(&id) {
                let slot = if let Some(i) = index {
                    f.parameters.get_mut(i)
                } else {
                    Some(&mut f.returns)
                };
                if let Some(slot) = slot {
                    if *slot == ValueType::Unknown {
                        *slot = ty;
                        changed = true;
                    } else if *slot != ty {
                        *slot = ValueType::Unknown;
                        conflicts.insert((id, index));
                        changed = true;
                    }
                }
            }
        }
        for (key, ty) in &field_observations {
            if *ty == ValueType::Unknown || field_conflicts.contains(key) {
                continue;
            }
            let slot = facts.fields.entry(key.clone()).or_default();
            if *slot == ValueType::Unknown {
                *slot = ty.clone();
                changed = true;
            } else if *slot != *ty {
                *slot = ValueType::Unknown;
                field_conflicts.insert(key.clone());
                changed = true;
            }
        }
        if !changed {
            for (key, ty) in &field_observations {
                if *ty == ValueType::Unknown
                    && let Some(slot) = facts.fields.get_mut(key)
                    && *slot != ValueType::Unknown
                {
                    *slot = ValueType::Unknown;
                    field_conflicts.insert(key.clone());
                    changed = true;
                }
            }
            // Unknown actuals/returns are real alternative contexts, not
            // permission to reuse a type learned from another callsite.
            for (id, index, ty) in observations {
                if ty != ValueType::Unknown {
                    continue;
                }
                if let Some(f) = facts.functions.get_mut(&id) {
                    let slot = if let Some(i) = index {
                        f.parameters.get_mut(i)
                    } else {
                        Some(&mut f.returns)
                    };
                    if let Some(slot) = slot
                        && *slot != ValueType::Unknown
                    {
                        *slot = ValueType::Unknown;
                        conflicts.insert((id, index));
                        changed = true;
                    }
                }
            }
            if !changed {
                let mut program = lower_all(&parsed, &facts).0;
                if !facts.benign_unknowns
                    && program
                        .functions
                        .iter()
                        .flat_map(|f| &f.blocks)
                        .flat_map(|b| &b.operations)
                        .any(|i| matches!(i.kind, Kind::GlobalMutationUnknown { .. }))
                {
                    facts.standard_models_blocked = true;
                    program = lower_all(&parsed, &facts).0;
                }
                // Unknown effects contaminate both callers and callees: a
                // caller may mutate methods before invoking an otherwise pure
                // helper that constructs an instance and acquires a new file.
                // Disconnected entry components need not lose exact dispatch.
                if !facts.benign_unknowns && !facts.classes.is_empty() {
                    let blocked = class_effect_components(&program);
                    if !blocked.is_empty() {
                        facts.blocked_class_dispatch.extend(blocked);
                        return Ok(lower_all(&parsed, &facts).0);
                    }
                }
                return Ok(program);
            }
        }
    }
    let mut p = lower_all(&parsed, &facts).0;
    for f in &mut p.functions {
        if let Some(b) = f.blocks.get_mut(f.entry) {
            b.operations.insert(
                0,
                Instruction {
                    site: Site {
                        path: "<project>".into(),
                        span: Span::default(),
                        anchor: "inference-limit".into(),
                    },
                    kind: Kind::Unknown {
                        affected: vec![],
                        reason: "Project type inference iteration limit reached".into(),
                    },
                },
            );
        }
    }
    Ok(p)
}
fn class_effect_components(program: &Program) -> BTreeSet<String> {
    let mut edges: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut blocked = BTreeSet::new();
    for function in &program.functions {
        for op in function.blocks.iter().flat_map(|b| &b.operations) {
            match &op.kind {
                Kind::Unknown { .. } | Kind::GlobalMutationUnknown { .. } => {
                    blocked.insert(function.id.clone());
                }
                Kind::Call { callee, .. } | Kind::Invoke { callee, .. } => {
                    edges
                        .entry(function.id.clone())
                        .or_default()
                        .insert(callee.clone());
                    edges
                        .entry(callee.clone())
                        .or_default()
                        .insert(function.id.clone());
                }
                _ => {}
            }
        }
    }
    let mut work: Vec<_> = blocked.iter().cloned().collect();
    while let Some(id) = work.pop() {
        for neighbour in edges.get(&id).into_iter().flatten() {
            if blocked.insert(neighbour.clone()) {
                work.push(neighbour.clone());
            }
        }
    }
    blocked
}
// Plain classes with unique methods and directly modeled data fields.
// No bases, decorators, descriptors, fields or allocation hooks.
// A plain __init__ is a resolved call, never an assumed pure constructor.
fn valid_initializer(n: Node<'_>) -> bool {
    // Python rejects non-None __init__ returns; never manufacture a usable
    // instance on that path. Nested definitions are conservatively excluded.
    fn body(n: Node<'_>) -> bool {
        if matches!(
            n.kind(),
            "function_definition" | "class_definition" | "lambda"
        ) {
            return false;
        }
        if n.kind() == "return_statement" {
            return kids(n).iter().all(|v| v.kind() == "none");
        }
        kids(n).into_iter().all(body)
    }
    field(n, "body").is_some_and(body)
}
fn plain_class(s: &Source, n: Node<'_>) -> bool {
    if n.kind() != "class_definition" || field(n, "superclasses").is_some() {
        return false;
    }
    let Some(body) = field(n, "body") else {
        return false;
    };
    let mut names = BTreeSet::new();
    kids(body).into_iter().all(|child| match child.kind() {
        "comment" | "pass_statement" => true,
        "expression_statement" => kids(child).into_iter().all(inert_literal),
        "function_definition" => {
            let name = text(&s.text, field(child, "name").unwrap());
            (!name.starts_with("__") || name == "__init__" && valid_initializer(child))
                && names.insert(name.to_string())
                && !definition_has_effects(child)
                && !deferred_body(child)
                && !params(s, child).is_empty()
        }
        _ => false,
    })
}
fn definitions(s: &Source) -> Vec<(String, Node<'_>)> {
    let mut out = Vec::new();
    for n in kids(s.tree.root_node()) {
        if n.kind() == "function_definition" {
            out.push((text(&s.text, field(n, "name").unwrap()).to_string(), n));
        } else if plain_class(s, n) {
            let class = text(&s.text, field(n, "name").unwrap());
            for method in kids(field(n, "body").unwrap()) {
                if method.kind() == "function_definition" {
                    out.push((
                        format!("{class}.{}", text(&s.text, field(method, "name").unwrap())),
                        method,
                    ));
                }
            }
        }
    }
    out
}
fn import_stmt(s: &Source, n: Node<'_>, out: &mut BTreeMap<String, String>) {
    let module = field(n, "module_name").map(|x| text(&s.text, x).to_string());
    let module = module.map(|m| {
        let count = m.chars().take_while(|c| *c == '.').count();
        if count == 0 {
            return m;
        }
        let mut base: Vec<_> = s.module.split('.').collect();
        if !s.package {
            base.pop();
        }
        for _ in 1..count {
            base.pop();
        }
        let tail = &m[count..];
        if !tail.is_empty() {
            base.push(tail);
        }
        base.join(".")
    });
    for ch in kids(n) {
        if Some(ch) == field(n, "module_name") {
            continue;
        }
        let original = if ch.kind() == "aliased_import" {
            field(ch, "name").unwrap()
        } else {
            ch
        };
        if !matches!(original.kind(), "dotted_name" | "identifier") {
            continue;
        }
        let name = text(&s.text, original);
        let alias = field(ch, "alias").map(|x| text(&s.text, x));
        let key = alias.unwrap_or_else(|| {
            if module.is_some() {
                name
            } else {
                name.split('.').next().unwrap()
            }
        });
        let value = if let Some(m) = &module {
            format!("{m}.{name}")
        } else if alias.is_some() {
            name.into()
        } else {
            key.into()
        };
        out.insert(key.into(), value);
    }
}
fn imports(s: &Source) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for n in kids(s.tree.root_node()) {
        if matches!(n.kind(), "import_statement" | "import_from_statement") {
            import_stmt(s, n, &mut out);
        }
    }
    out
}
fn annotation_type(s: &Source, n: Node<'_>, im: &BTreeMap<String, String>) -> ValueType {
    let raw = text(&s.text, n);
    let first = raw.split('.').next().unwrap_or(raw);
    let resolved = im
        .get(first)
        .map(|v| format!("{v}{}", &raw[first.len()..]))
        .unwrap_or_else(|| raw.into());
    match resolved.as_str() {
        // A declared base class does not prove exact dispatch: subclasses may override.
        "pathlib.Path" | "pathlib.PosixPath" | "pathlib.WindowsPath" => ValueType::Unknown,
        "typing.IO" | "typing.BinaryIO" | "typing.TextIO" | "io.IOBase" | "io.TextIOBase"
        | "io.BufferedIOBase" => ValueType::Unknown,
        "str" | "int" | "bytes" | "bool" | "float" | "None" => ValueType::Scalar,
        _ => ValueType::Unknown,
    }
}
fn params(s: &Source, n: Node<'_>) -> Vec<(String, ValueType)> {
    let im = imports(s);
    field(n, "parameters")
        .map(kids)
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| {
            if p.kind() == "identifier" {
                Some((text(&s.text, p).into(), ValueType::Unknown))
            } else {
                let name = field(p, "name")
                    .or_else(|| kids(p).into_iter().find(|x| x.kind() == "identifier"))?;
                Some((
                    text(&s.text, name).into(),
                    field(p, "type")
                        .map(|t| annotation_type(s, t, &im))
                        .unwrap_or_default(),
                ))
            }
        })
        .collect()
}
fn assignment_names(s: &Source, n: Node<'_>, out: &mut BTreeSet<String>) {
    if matches!(n.kind(), "function_definition" | "lambda") {
        return;
    }
    if n.kind() == "class_definition" {
        if plain_class(s, n) {
            return;
        }
        if let Some(name) = field(n, "name") {
            out.insert(text(&s.text, name).into());
        }
        return;
    }
    if matches!(
        n.kind(),
        "assignment" | "augmented_assignment" | "for_statement"
    ) && let Some(left) = field(n, "left")
    {
        fn targets(s: &Source, n: Node<'_>, out: &mut BTreeSet<String>) {
            if n.kind() == "identifier" {
                out.insert(text(&s.text, n).into());
            } else if matches!(n.kind(), "tuple_pattern" | "list_pattern" | "pattern_list") {
                for c in kids(n) {
                    targets(s, c, out);
                }
            }
        }
        targets(s, left, out);
    }
    for c in kids(n) {
        assignment_names(s, c, out);
    }
}
fn trusted_import(name: &str) -> bool {
    matches!(
        name.split('.').next().unwrap_or(name),
        "pathlib" | "io" | "typing" | "builtins" | "__future__" | "hashlib" | "json"
    )
}
fn import_is_inert(
    s: &Source,
    n: Node<'_>,
    inert: &BTreeSet<String>,
    local_modules: &BTreeSet<String>,
) -> bool {
    let mut names = BTreeMap::new();
    import_stmt(s, n, &mut names);
    !names.is_empty()
        && names.values().all(|name| {
            (trusted_import(name)
                && !local_modules.contains(name.split('.').next().unwrap_or(name)))
                || inert
                    .iter()
                    .any(|m| name == m || name.starts_with(&format!("{m}.")))
        })
}
fn contains_call(n: Node<'_>) -> bool {
    n.kind() == "call" || kids(n).into_iter().any(contains_call)
}
fn deferred_body(n: Node<'_>) -> bool {
    fn has_yield(n: Node<'_>) -> bool {
        if matches!(
            n.kind(),
            "function_definition" | "lambda" | "class_definition"
        ) {
            return false;
        }
        n.kind() == "yield" || kids(n).into_iter().any(has_yield)
    }
    n.child(0).is_some_and(|n| n.kind() == "async") || field(n, "body").is_some_and(has_yield)
}
fn definition_has_effects(n: Node<'_>) -> bool {
    field(n, "parameters").is_some_and(|p| {
        kids(p)
            .into_iter()
            .any(|p| field(p, "value").is_some() || field(p, "type").is_some_and(contains_call))
    }) || field(n, "return_type").is_some_and(contains_call)
}
fn rebound(n: Node<'_>, source: &str, names: &mut BTreeSet<String>) {
    if matches!(n.kind(), "function_definition" | "class_definition") {
        if let Some(name) = field(n, "name") {
            names.insert(text(source, name).into());
        }
        return;
    }
    if n.kind() == "lambda" {
        return;
    }
    if matches!(
        n.kind(),
        "assignment" | "augmented_assignment" | "named_expression" | "for_statement"
    ) && let Some(left) = field(n, "left").or_else(|| field(n, "name"))
    {
        fn targets(n: Node<'_>, source: &str, names: &mut BTreeSet<String>) {
            if n.kind() == "identifier" {
                names.insert(text(source, n).into());
            } else if !matches!(n.kind(), "attribute" | "subscript") {
                for c in kids(n) {
                    targets(c, source, names);
                }
            }
        }
        targets(left, source, names);
    }
    // Names bound through context aliases, imports, or deletion also
    // invalidate header facts, even when nested in a pattern.
    if matches!(
        n.kind(),
        "import_statement" | "import_from_statement" | "as_pattern_target" | "delete_statement"
    ) {
        fn identifiers(n: Node<'_>, source: &str, names: &mut BTreeSet<String>) {
            if n.kind() == "identifier" {
                names.insert(text(source, n).into());
            }
            for c in kids(n) {
                identifiers(c, source, names);
            }
        }
        identifiers(n, source, names);
    }
    for c in kids(n) {
        rebound(c, source, names);
    }
}

// Literal binding cannot invoke target code. It still shadows symbols via
// assignment_names; purity is not evidence that a global retains a type.
fn inert_literal(n: Node<'_>) -> bool {
    match n.kind() {
        "integer" | "float" | "true" | "false" | "none" => true,
        "string" => !kids(n).iter().any(|n| n.kind() == "interpolation"),
        _ => false,
    }
}
fn inert_module_expression(n: Node<'_>, source: &str) -> bool {
    inert_literal(n)
        || (n.kind() == "assignment"
            && field(n, "type").is_none()
            && field(n, "left").is_some_and(|left| {
                left.kind() == "identifier" && text(source, left) != "__builtins__"
            })
            && field(n, "right").is_some_and(inert_literal))
}
fn inert_modules(sources: &[Source]) -> BTreeSet<String> {
    let local_modules: BTreeSet<_> = sources
        .iter()
        .map(|s| s.module.split('.').next().unwrap().to_string())
        .collect();
    let mut inert = BTreeSet::new();
    loop {
        let old = inert.len();
        for s in sources {
            let safe = kids(s.tree.root_node())
                .into_iter()
                .all(|n| match n.kind() {
                    "comment" | "pass_statement" => true,
                    "function_definition" => !definition_has_effects(n),
                    "class_definition" => plain_class(s, n),
                    "import_statement" | "import_from_statement" => {
                        import_is_inert(s, n, &inert, &local_modules)
                    }
                    "expression_statement" => kids(n)
                        .into_iter()
                        .all(|n| inert_module_expression(n, &s.text)),
                    _ => false,
                });
            if safe {
                inert.insert(s.module.clone());
            }
        }
        if inert.len() == old {
            return inert;
        }
    }
}
fn unstable_bindings(s: &Source) -> BTreeSet<String> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for n in kids(s.tree.root_node()) {
        if matches!(n.kind(), "function_definition" | "class_definition") {
            if let Some(name) = field(n, "name") {
                *counts.entry(text(&s.text, name).into()).or_default() += 1;
            }
        } else if matches!(n.kind(), "import_statement" | "import_from_statement") {
            let mut names = BTreeMap::new();
            import_stmt(s, n, &mut names);
            for name in names.keys() {
                *counts.entry(name.clone()).or_default() += 1;
            }
        }
    }
    counts
        .into_iter()
        .filter_map(|(name, n)| (n > 1).then_some(name))
        .collect()
}
type Observation = (String, Option<usize>, ValueType);
type FieldObservation = ((String, String), ValueType);
fn lower_all(
    sources: &[Source],
    facts: &SemanticFacts,
) -> (Program, Vec<Observation>, Vec<FieldObservation>) {
    let mut p = Program::default();
    let mut obs = vec![];
    let mut field_obs = vec![];
    let inert = inert_modules(sources);
    let local_modules: BTreeSet<_> = sources
        .iter()
        .map(|s| s.module.split('.').next().unwrap().to_string())
        .collect();
    for s in sources {
        let nodes = kids(s.tree.root_node());
        let mut base = BTreeMap::new();
        let unstable = unstable_bindings(s);
        for n in &nodes {
            if matches!(n.kind(), "import_statement" | "import_from_statement") {
                import_stmt(s, *n, &mut base);
            }
            if n.kind() == "function_definition" || plain_class(s, *n) {
                let name = text(&s.text, field(*n, "name").unwrap());
                base.insert(name.into(), format!("{}::{name}", s.module));
            }
        }
        let mut global_shadow = BTreeSet::new();
        assignment_names(s, s.tree.root_node(), &mut global_shadow);
        global_shadow.extend(unstable.iter().cloned());
        for name in &global_shadow {
            base.remove(name);
        }
        let mut emitted = BTreeSet::new();
        for (qualified_name, node) in definitions(s) {
            let n = &node;
            {
                let id = format!("{}::{qualified_name}", s.module);
                if !emitted.insert(id.clone()) {
                    continue;
                }
                let parameters = params(s, *n);
                let mut f = Front::new(s, facts, id.clone(), base.clone());
                f.inert = inert.clone();
                f.local_modules = local_modules.clone();
                f.unstable = unstable.clone();
                if !inert.contains(&s.module) && !facts.benign_unknowns {
                    f.unknown(
                        *n,
                        "Executable module initialization is not modeled for function entry",
                        vec![],
                    );
                    f.symbols.clear();
                    f.types.insert("open".into(), ValueType::Unknown);
                }
                for name in &global_shadow {
                    f.types.insert(name.clone(), ValueType::Unknown);
                }
                for (i, (name, _)) in parameters.iter().enumerate() {
                    f.symbols.remove(name);
                    f.types.insert(
                        name.clone(),
                        facts.functions[&id]
                            .parameters
                            .get(i)
                            .cloned()
                            .unwrap_or_default(),
                    );
                }
                f.shadow_locals(field(*n, "body").unwrap());
                let name = text(&s.text, field(*n, "name").unwrap());
                if (unstable.contains(name) || deferred_body(*n)) && !facts.benign_unknowns {
                    f.unknown(
                        *n,
                        "Function temporal binding or deferred coroutine/generator execution is unresolved",
                        vec![],
                    );
                } else {
                    f.statements(field(*n, "body").unwrap());
                }
                p.roots.push(id.clone());
                p.functions.push(Function {
                    id,
                    params: parameters
                        .into_iter()
                        .map(|(n, _)| Place::local(n))
                        .collect(),
                    blocks: f.blocks,
                    entry: 0,
                });
                obs.extend(f.observations);
                field_obs.extend(f.field_observations);
            }
        }
        let id = format!("{}::<module>", s.module);
        let mut f = Front::new(s, facts, id.clone(), BTreeMap::new());
        f.inert = inert.clone();
        f.local_modules = local_modules.clone();
        f.unstable = unstable;
        for n in nodes {
            if n.kind() == "function_definition" {
                f.definition_effects(n);
                let name = text(&s.text, field(n, "name").unwrap());
                f.symbols
                    .insert(name.into(), format!("{}::{name}", s.module));
            } else if plain_class(s, n)
                && !facts.blocked_class_dispatch.contains(&f.id)
                && facts.classes.contains(&format!(
                    "{}::{}",
                    s.module,
                    text(&s.text, field(n, "name").unwrap())
                ))
            {
                let name = text(&s.text, field(n, "name").unwrap());
                f.symbols
                    .insert(name.into(), format!("{}::{name}", s.module));
                f.emit(
                    n,
                    Kind::LibraryEffect {
                        model: "python.class.plain_definition.v1".into(),
                    },
                );
            } else if plain_class(s, n) {
                f.unknown(
                    n,
                    "Plain class dispatch requires a closed call component",
                    vec![],
                );
            } else {
                f.statement(n);
            }
        }
        p.roots.push(id.clone());
        p.functions.push(Function {
            id,
            params: vec![],
            blocks: f.blocks,
            entry: 0,
        });
        obs.extend(f.observations);
        field_obs.extend(f.field_observations);
    }
    let calls: BTreeMap<String, Vec<String>> = p
        .functions
        .iter()
        .map(|f| {
            (
                f.id.clone(),
                f.blocks
                    .iter()
                    .flat_map(|b| &b.operations)
                    .filter_map(|op| {
                        if let Kind::Call { callee, .. } | Kind::Invoke { callee, .. } = &op.kind {
                            Some(callee.clone())
                        } else {
                            None
                        }
                    })
                    .collect(),
            )
        })
        .collect();
    let called: BTreeSet<_> = calls.values().flatten().cloned().collect();
    p.roots.retain(|id| !called.contains(id));
    let mut reachable = BTreeSet::new();
    let mut work = p.roots.clone();
    while let Some(id) = work.pop() {
        if reachable.insert(id.clone()) {
            work.extend(calls.get(&id).into_iter().flatten().cloned());
        }
    }
    // Retain an entry for disconnected recursive components as well.
    for f in &p.functions {
        if !reachable.contains(&f.id) {
            p.roots.push(f.id.clone());
            work.push(f.id.clone());
            while let Some(id) = work.pop() {
                if reachable.insert(id.clone()) {
                    work.extend(calls.get(&id).into_iter().flatten().cloned());
                }
            }
        }
    }
    (p, obs, field_obs)
}
#[derive(Clone)]
struct Value {
    place: Place,
    ty: ValueType,
}
#[derive(Clone, Copy)]
struct LoopTargets {
    header: usize,
    exit: usize,
    cleanup_depth: usize,
}
struct Front<'a> {
    s: &'a Source,
    facts: &'a SemanticFacts,
    id: String,
    symbols: BTreeMap<String, String>,
    types: BTreeMap<String, ValueType>,
    blocks: Vec<Block>,
    at: usize,
    counter: usize,
    cleanup: Vec<Kind>,
    observations: Vec<Observation>,
    field_observations: Vec<FieldObservation>,
    anchors: BTreeMap<String, usize>,
    inert: BTreeSet<String>,
    local_modules: BTreeSet<String>,
    unstable: BTreeSet<String>,
    loops: Vec<LoopTargets>,
    exception_targets: Vec<(usize, usize)>,
}
impl<'a> Front<'a> {
    fn new(
        s: &'a Source,
        facts: &'a SemanticFacts,
        id: String,
        symbols: BTreeMap<String, String>,
    ) -> Self {
        Self {
            s,
            facts,
            id,
            symbols,
            types: BTreeMap::new(),
            blocks: vec![Block {
                operations: vec![],
                terminator: Terminator::Stop,
            }],
            at: 0,
            counter: 0,
            cleanup: vec![],
            observations: vec![],
            field_observations: vec![],
            anchors: BTreeMap::new(),
            inert: BTreeSet::new(),
            local_modules: BTreeSet::new(),
            unstable: BTreeSet::new(),
            loops: vec![],
            exception_targets: vec![],
        }
    }
    fn site(&mut self, n: Node<'_>) -> Site {
        let key = format!("{}:{}", self.id, n.kind());
        let seq = self.anchors.entry(key.clone()).or_default();
        *seq += 1;
        Site {
            path: self.s.path.clone(),
            span: span(n),
            anchor: format!("{key}:{}", *seq),
        }
    }
    fn emit(&mut self, n: Node<'_>, kind: Kind) {
        let identity = match &kind {
            Kind::Acquire { .. } => format!("acquire:{}", text(&self.s.text, n)),
            Kind::AllocateObject { .. } => format!("object:{}", text(&self.s.text, n)),
            Kind::Assign { target, .. } => format!("assign:{}", target.root),
            Kind::Read { value } => format!("read:{}:{:?}", value.root, value.projections),
            Kind::Write { value } => format!("write:{}:{:?}", value.root, value.projections),
            Kind::Close { value }
            | Kind::CloseIfOwned { value }
            | Kind::CloseIfUnborrowed { value } => {
                format!("close:{}:{:?}", value.root, value.projections)
            }
            Kind::Borrow {
                source, mutable, ..
            } => format!("borrow:{}:{mutable}", source.root),
            Kind::EndBorrow { value } => format!("release:{}", value.root),
            Kind::Call { callee, .. } | Kind::Invoke { callee, .. } => format!("call:{callee}"),
            Kind::Unknown { reason, .. } => format!("unknown:{reason}"),
            _ => format!("operation:{}", n.kind()),
        };
        let seq = self.anchors.entry(identity.clone()).or_default();
        *seq += 1;
        let site = Site {
            path: self.s.path.clone(),
            span: span(n),
            anchor: format!("{}:{identity}:{}", self.id, *seq),
        };
        self.blocks[self.at]
            .operations
            .push(Instruction { site, kind });
    }
    fn unknown(&mut self, n: Node<'_>, reason: impl Into<String>, affected: Vec<Place>) {
        self.emit(
            n,
            Kind::Unknown {
                affected,
                reason: reason.into(),
            },
        );
    }
    fn temp(&mut self, ty: ValueType) -> Value {
        self.counter += 1;
        Value {
            place: Place::local(format!("$v{}", self.counter)),
            ty,
        }
    }
    fn symbol(&self, n: Node<'_>) -> Option<String> {
        match n.kind() {
            "identifier" => {
                let name = text(&self.s.text, n);
                if self.unstable.contains(name) {
                    return None;
                }
                self.symbols.get(name).cloned().or_else(|| {
                    if matches!(
                        name,
                        "open"
                            | "memoryview"
                            | "setattr"
                            | "delattr"
                            | "iter"
                            | "Exception"
                            | "BaseException"
                            | "ValueError"
                            | "TypeError"
                            | "KeyboardInterrupt"
                            | "SystemExit"
                    ) && !self.types.contains_key(name)
                    {
                        Some(format!("builtins.{name}"))
                    } else {
                        None
                    }
                })
            }
            "attribute" => Some(format!(
                "{}.{}",
                self.symbol(field(n, "object")?)?,
                text(&self.s.text, field(n, "attribute")?)
            )),
            _ => None,
        }
    }
    fn resolve_function(&self, name: &str) -> Option<String> {
        if self.facts.functions.contains_key(name) {
            return Some(name.into());
        }
        let (module, fun) = name.rsplit_once('.')?;
        let id = format!("{module}::{fun}");
        self.facts.functions.contains_key(&id).then_some(id)
    }
    fn shadow_locals(&mut self, n: Node<'_>) {
        if matches!(
            n.kind(),
            "function_definition" | "class_definition" | "lambda"
        ) {
            return;
        }
        if n.kind() == "assignment"
            && let Some(l) = field(n, "left")
            && l.kind() == "identifier"
        {
            let name = text(&self.s.text, l).to_string();
            self.symbols.remove(&name);
            self.types.entry(name).or_default();
        }
        for c in kids(n) {
            self.shadow_locals(c);
        }
    }
    fn expr(&mut self, n: Node<'_>) -> Value {
        match n.kind() {
            "identifier" => Value {
                place: Place::local(text(&self.s.text, n)),
                ty: self
                    .types
                    .get(text(&self.s.text, n))
                    .cloned()
                    .unwrap_or_default(),
            },
            "string" if kids(n).iter().any(|c| c.kind() == "interpolation") => {
                let mut plain = true;
                for child in kids(n) {
                    if child.kind() == "interpolation" {
                        if let Some(expr) = field(child, "expression") {
                            let value = self.expr(expr);
                            plain &= matches!(value.ty, ValueType::Text | ValueType::JsonData)
                                && kids(child).len() == 1;
                        } else {
                            plain = false;
                        }
                    }
                }
                if plain {
                    self.emit_fallible_effect(
                        n,
                        Kind::LibraryEffect {
                            model: "python.format.plain_builtin_data.v1".into(),
                        },
                    );
                    self.temp(ValueType::Text)
                } else {
                    self.unknown(
                        n,
                        "String formatting/conversion effects are unresolved",
                        vec![],
                    );
                    self.temp(ValueType::Unknown)
                }
            }
            "binary_operator" | "unary_operator" if exact_integer_expression(n, &self.s.text) => {
                self.temp(ValueType::JsonData)
            }
            "binary_operator" => {
                let left = field(n, "left").unwrap();
                let right = field(n, "right").unwrap();
                let lhs = self.expr(left);
                let rhs = self.expr(right);
                let raw = text(&self.s.text, right);
                let text_literal = right.kind() == "string"
                    && !kids(right).iter().any(|c| c.kind() == "interpolation")
                    && raw.find(['\'', '"']).is_some_and(|i| {
                        !raw[..i].chars().any(|c| matches!(c, 'b' | 'B' | 'f' | 'F'))
                    });
                if field(n, "operator").is_some_and(|op| text(&self.s.text, op) == "+")
                    && lhs.ty == ValueType::Text
                    && rhs.ty == ValueType::Text
                {
                    self.emit_fallible_effect(
                        n,
                        Kind::LibraryEffect {
                            model: "python.text.concat.exact.v1".into(),
                        },
                    );
                    self.temp(ValueType::Text)
                } else if field(n, "operator").is_some_and(|op| text(&self.s.text, op) == "/")
                    && lhs.ty == ValueType::Path
                    && (text_literal || matches!(rhs.ty, ValueType::Path | ValueType::Text))
                {
                    self.emit_fallible_effect(
                        n,
                        Kind::LibraryEffect {
                            model: "stdlib.path.join.exact.v1".into(),
                        },
                    );
                    self.temp(ValueType::Path)
                } else {
                    self.unknown(
                        n,
                        "Binary operator dispatch/conversion is unresolved",
                        vec![lhs.place, rhs.place],
                    );
                    self.temp(ValueType::Unknown)
                }
            }
            "string" => self.temp(
                if text(&self.s.text, n).find(['\'', '\"']).is_some_and(|i| {
                    text(&self.s.text, n)[..i]
                        .chars()
                        .any(|c| matches!(c, 'b' | 'B'))
                }) {
                    ValueType::Scalar
                } else {
                    ValueType::Text
                },
            ),
            "integer" | "float" | "true" | "false" | "none" => self.temp(ValueType::JsonData),
            "dictionary" | "list" | "tuple" => {
                let mut plain = true;
                for child in kids(n) {
                    if n.kind() == "dictionary" && child.kind() == "pair" {
                        for name in ["key", "value"] {
                            if let Some(expr) = field(child, name) {
                                plain &= matches!(
                                    self.expr(expr).ty,
                                    ValueType::Text | ValueType::JsonData
                                );
                            } else {
                                plain = false;
                            }
                        }
                    } else if n.kind() != "dictionary"
                        && !matches!(child.kind(), "list_splat" | "dictionary_splat")
                    {
                        plain &=
                            matches!(self.expr(child).ty, ValueType::Text | ValueType::JsonData);
                    } else {
                        self.expr(child);
                        plain = false;
                    }
                }
                if plain {
                    self.emit_fallible_effect(
                        n,
                        Kind::LibraryEffect {
                            model: "python.container.plain_json_data.v1".into(),
                        },
                    );
                    self.temp(ValueType::JsonData)
                } else {
                    self.unknown(
                        n,
                        "Container contents/hash/iteration effects are unresolved",
                        vec![],
                    );
                    self.temp(ValueType::Unknown)
                }
            }
            "parenthesized_expression" => kids(n)
                .first()
                .map(|c| self.expr(*c))
                .unwrap_or_else(|| self.temp(ValueType::Unknown)),
            "call" => self.call(n),
            "lambda" => {
                // Defaults run now; the body runs only when the callable is invoked.
                if let Some(parameters) = field(n, "parameters") {
                    self.expr(parameters);
                }
                self.unknown(
                    n,
                    "Escaping lambda invocation effects are not modeled",
                    vec![],
                );
                self.temp(ValueType::Unknown)
            }
            "subscript" => {
                let obj = self.expr(field(n, "value").unwrap());
                let index = field(n, "subscript").unwrap();
                if matches!(obj.ty, ValueType::BufferView { .. })
                    && exact_integer_expression(index, &self.s.text)
                {
                    self.emit_fallible_effect(n, Kind::Read { value: obj.place });
                    self.temp(ValueType::Scalar)
                } else {
                    self.expr(index);
                    self.unknown(
                        n,
                        "Indexing protocol or view slice identity is unresolved",
                        vec![obj.place],
                    );
                    self.temp(ValueType::Unknown)
                }
            }
            "attribute" => {
                let obj = self.expr(field(n, "object").unwrap());
                if obj.ty == ValueType::Path
                    && text(&self.s.text, field(n, "attribute").unwrap()) == "parent"
                {
                    self.emit_fallible_effect(
                        n,
                        Kind::LibraryEffect {
                            model: "stdlib.path.parent.exact.v1".into(),
                        },
                    );
                    return self.temp(ValueType::Path);
                }
                let name = text(&self.s.text, field(n, "attribute").unwrap()).to_string();
                if let Some(class) = self.direct_field_class(&obj.ty, &name) {
                    let ty = self
                        .facts
                        .fields
                        .get(&(class, name.clone()))
                        .cloned()
                        .unwrap_or_default();
                    let value = self.temp(ty);
                    let mut source = obj.place;
                    source.projections.push(name);
                    // Attribute lookup can fail before yielding a value. The
                    // loaded reference is a snapshot, not a delayed field access.
                    self.emit_fallible_effect(
                        n,
                        Kind::LibraryEffect {
                            model: "python.plain_field.load.v1".into(),
                        },
                    );
                    self.emit(
                        n,
                        Kind::Assign {
                            target: value.place.clone(),
                            source,
                        },
                    );
                    return value;
                }
                let mut p = obj.place;
                p.projections
                    .push(text(&self.s.text, field(n, "attribute").unwrap()).into());
                self.unknown(
                    n,
                    "Attribute identity/type requires provider facts",
                    vec![p.clone()],
                );
                Value {
                    place: p,
                    ty: ValueType::Unknown,
                }
            }
            _ => {
                let mut affected = vec![];
                for c in kids(n) {
                    if !matches!(c.kind(), "type" | "comment") {
                        affected.push(self.expr(c).place);
                    }
                }
                self.unknown(n, format!("Unsupported expression: {}", n.kind()), affected);
                self.temp(ValueType::Unknown)
            }
        }
    }
    fn exception_destination(&self) -> Option<(usize, usize)> {
        self.exception_targets
            .iter()
            .rev()
            .copied()
            .find(|(_, depth)| *depth <= self.cleanup.len())
    }
    fn unwind_block(
        &mut self,
        n: Node<'_>,
        destination: Option<(usize, usize)>,
        categories: Option<u8>,
    ) -> usize {
        let normal = self.at;
        let unwind = self.block();
        self.at = unwind;
        if let Some(categories) = categories {
            self.emit(n, Kind::SetException { categories });
        }
        self.cleanup_to(n, destination.map_or(0, |(_, depth)| depth));
        self.blocks[self.at].terminator = if let Some((handler, _)) = destination {
            Terminator::Jump { target: handler }
        } else {
            Terminator::Raise { site: self.site(n) }
        };
        self.at = normal;
        unwind
    }
    fn emit_fallible_effect(&mut self, n: Node<'_>, kind: Kind) {
        let unwind = self.unwind_block(n, self.exception_destination(), Some(3));
        let success = self.block();
        self.blocks[self.at].terminator = Terminator::Branch {
            then_target: success,
            else_target: unwind,
        };
        self.at = success;
        let close = matches!(&kind, Kind::Close { .. } | Kind::CloseIfOwned { .. });
        self.emit(n, kind);
        if close {
            let continuation = self.block();
            self.blocks[self.at].terminator = Terminator::Branch {
                then_target: continuation,
                else_target: unwind,
            };
            self.at = continuation;
        }
    }
    fn call(&mut self, n: Node<'_>) -> Value {
        let Some(callee) = field(n, "function") else {
            self.unknown(n, "Call has no target", vec![]);
            return self.temp(ValueType::Unknown);
        };
        let mut symbol = self.symbol(callee);
        let receiver = if callee.kind() == "attribute" && symbol.is_none() {
            Some(self.expr(field(callee, "object").unwrap()))
        } else {
            None
        };
        let arg_nodes = field(n, "arguments").map(kids).unwrap_or_default();
        let mut args = vec![];
        let mut keyword = false;
        let unsafe_library_binding = arg_nodes.iter().any(|a| {
            matches!(a.kind(), "list_splat" | "dictionary_splat")
                || a.kind() == "keyword_argument"
                    && field(*a, "name").is_some_and(|name| {
                        !matches!(
                            text(&self.s.text, name),
                            "mode" | "buffering" | "encoding" | "errors" | "newline" | "closefd"
                        )
                    })
        });
        for a in arg_nodes.clone() {
            if a.kind() == "keyword_argument" {
                keyword = true;
                if let Some(v) = field(a, "value") {
                    args.push(self.expr(v));
                }
            } else {
                if matches!(a.kind(), "list_splat" | "dictionary_splat") {
                    keyword = true;
                }
                args.push(self.expr(a));
            }
        }
        let target = self.temp(ValueType::Unknown);
        if let Some(r) = &receiver
            && let ValueType::Instance(class) = &r.ty
            && self.facts.classes.contains(class)
            && !self.facts.blocked_class_dispatch.contains(&self.id)
        {
            let method = text(&self.s.text, field(callee, "attribute").unwrap());
            let id = format!("{class}.{method}");
            if self.facts.functions.contains_key(&id) {
                symbol = Some(id);
                args.insert(0, r.clone());
            }
        }

        if let Some(name) = symbol.as_deref() {
            let class_id = if self.facts.classes.contains(name) {
                Some(name.to_string())
            } else {
                name.rsplit_once('.')
                    .map(|(m, c)| format!("{m}::{c}"))
                    .filter(|id| self.facts.classes.contains(id))
            };
            if let Some(class) = class_id
                && !self.facts.blocked_class_dispatch.contains(&self.id)
                && !keyword
            {
                let initializer = format!("{class}.__init__");
                let signature = self.facts.functions.get(&initializer);
                if signature.is_some_and(|f| f.parameters.len() != args.len() + 1)
                    || signature.is_none() && !args.is_empty()
                {
                    self.unknown(n, "Constructor argument binding is unresolved", vec![]);
                    return target;
                }
                self.emit_fallible_effect(
                    n,
                    Kind::AllocateObject {
                        target: target.place.clone(),
                    },
                );
                let instance = Value {
                    ty: ValueType::Instance(class),
                    ..target
                };
                if signature.is_some() {
                    args.insert(0, instance.clone());
                    for (index, argument) in args.iter().enumerate() {
                        self.observations.push((
                            initializer.clone(),
                            Some(index),
                            argument.ty.clone(),
                        ));
                    }
                    let unwind = self.unwind_block(n, self.exception_destination(), None);
                    self.emit(
                        n,
                        Kind::Invoke {
                            // __init__ returns None; the expression keeps the newly
                            // allocated instance only on normal completion.
                            target: None,
                            callee: initializer,
                            args: args.into_iter().map(|a| a.place).collect(),
                            unwind,
                        },
                    );
                }
                return instance;
            }

            if matches!(name, "builtins.setattr" | "builtins.delattr") {
                self.emit(
                    n,
                    Kind::GlobalMutationUnknown {
                        reason: "Dynamic attribute mutation may replace library bindings".into(),
                    },
                );
                return target;
            }
            let standard = !self.facts.standard_models_blocked
                && !self
                    .local_modules
                    .contains(name.split('.').next().unwrap_or(name));
            // Default json.load calls read on an exact standard file. Custom
            // decoders/hooks or unknown receiver protocols stay unresolved.
            if standard
                && name == "json.load"
                && args.len() == 1
                && args[0].ty == ValueType::File
                && !keyword
            {
                self.emit_fallible_effect(
                    n,
                    Kind::Read {
                        value: args[0].place.clone(),
                    },
                );
                self.emit_fallible_effect(
                    n,
                    Kind::LibraryEffect {
                        model: "stdlib.json.load.default.v1".into(),
                    },
                );
                return Value {
                    ty: ValueType::JsonData,
                    ..target
                };
            }
            if standard
                && name == "json.loads"
                && !keyword
                && args.len() == 1
                && matches!(args[0].ty, ValueType::Text | ValueType::FileData)
            {
                self.emit_fallible_effect(
                    n,
                    Kind::LibraryEffect {
                        model: "stdlib.json.loads.default.plain_data.v1".into(),
                    },
                );
                return Value {
                    ty: ValueType::JsonData,
                    ..target
                };
            }
            if standard
                && name == "json.dumps"
                && !args.is_empty()
                && matches!(args[0].ty, ValueType::Text | ValueType::JsonData)
                && arg_nodes[0].kind() != "keyword_argument"
                && (arg_nodes.len() == 1
                    || arg_nodes.len() == 2
                        && arg_nodes[1].kind() == "keyword_argument"
                        && field(arg_nodes[1], "name")
                            .is_some_and(|x| text(&self.s.text, x) == "ensure_ascii")
                        && field(arg_nodes[1], "value")
                            .is_some_and(|x| matches!(x.kind(), "true" | "false")))
            {
                self.emit_fallible_effect(
                    n,
                    Kind::LibraryEffect {
                        model: "stdlib.json.dumps.plain_data.default_encoder.v1".into(),
                    },
                );
                return Value {
                    ty: ValueType::Text,
                    ..target
                };
            }
            if standard
                && name == "io.BytesIO"
                && !keyword
                && (args.is_empty()
                    || args.len() == 1
                        && arg_nodes[0].kind() == "string"
                        && text(&self.s.text, arg_nodes[0]).starts_with(['b', 'B'])
                        && !kids(arg_nodes[0])
                            .iter()
                            .any(|n| n.kind() == "interpolation"))
            {
                self.emit_fallible_effect(
                    n,
                    Kind::Acquire {
                        target: target.place.clone(),
                    },
                );
                return Value {
                    ty: ValueType::BytesBuffer,
                    ..target
                };
            }
            if standard
                && name == "builtins.memoryview"
                && !keyword
                && args.len() == 1
                && arg_nodes[0].kind() == "string"
                && text(&self.s.text, arg_nodes[0]).starts_with(['b', 'B'])
                && !kids(arg_nodes[0])
                    .iter()
                    .any(|n| n.kind() == "interpolation")
            {
                let owner = self.temp(ValueType::Unknown);
                self.emit_fallible_effect(
                    n,
                    Kind::Acquire {
                        target: owner.place.clone(),
                    },
                );
                self.emit(
                    n,
                    Kind::Borrow {
                        source: owner.place,
                        target: target.place.clone(),
                        mutable: false,
                    },
                );
                return Value {
                    ty: ValueType::BufferView { mutable: false },
                    ..target
                };
            }
            // Explicit management-ownership policy for an exact built-in buffer
            // and fixed UTF-8 codec. Other buffer protocols stay unresolved.
            if standard
                && name == "io.TextIOWrapper"
                && !keyword
                && args.len() == 2
                && args[0].ty == ValueType::BytesBuffer
                && matches!(text(&self.s.text, arg_nodes[1]), "\"utf-8\"" | "'utf-8'")
            {
                self.emit_fallible_effect(
                    n,
                    Kind::Read {
                        value: args[0].place.clone(),
                    },
                );
                self.emit_fallible_effect(
                    n,
                    Kind::LibraryEffect {
                        model: "stdlib.text_wrapper.bytesio.utf8.ownership_policy.v1".into(),
                    },
                );
                self.emit(
                    n,
                    Kind::TransferTo {
                        source: args[0].place.clone(),
                        target: target.place.clone(),
                    },
                );
                return Value {
                    ty: ValueType::TextWrapper,
                    ..target
                };
            }
            if standard && name == "hashlib.sha256" && args.is_empty() && !keyword {
                self.emit_fallible_effect(
                    n,
                    Kind::LibraryEffect {
                        model: "stdlib.hash.sha256.empty.v1".into(),
                    },
                );
                return Value {
                    ty: ValueType::HashDigest,
                    ..target
                };
            }
            if standard
                && matches!(
                    name,
                    "pathlib.Path" | "pathlib.PosixPath" | "pathlib.WindowsPath"
                )
            {
                if keyword
                    || args.iter().zip(&arg_nodes).any(|(a, node)| {
                        !matches!(a.ty, ValueType::Path | ValueType::Text)
                            && !(matches!(
                                node.kind(),
                                "string" | "integer" | "float" | "true" | "false" | "none"
                            ) && !kids(*node).iter().any(|c| c.kind() == "interpolation"))
                    })
                {
                    self.unknown(
                        n,
                        "Path construction argument protocol is unresolved",
                        vec![],
                    );
                }
                self.emit_fallible_effect(
                    n,
                    Kind::LibraryEffect {
                        model: "stdlib.path.construct.v1".into(),
                    },
                );
                return Value {
                    ty: ValueType::Path,
                    ..target
                };
            }
            if standard && matches!(name, "builtins.open" | "io.open") {
                if unsafe_library_binding {
                    self.unknown(
                        n,
                        "File opening custom callback/variadic binding effects are unresolved",
                        vec![],
                    );
                }
                self.emit_fallible_effect(
                    n,
                    Kind::Acquire {
                        target: target.place.clone(),
                    },
                );
                return Value {
                    ty: ValueType::File,
                    ..target
                };
            }
            if let Some(id) = self.resolve_function(name) {
                if keyword {
                    self.unknown(
                        n,
                        "Project call keyword/variadic binding is not modeled",
                        args.iter().map(|a| a.place.clone()).collect(),
                    );
                    return target;
                }
                let signature = &self.facts.functions[&id];
                if args.len() != signature.parameters.len() {
                    self.unknown(
                        n,
                        "Project call arity/default binding is not modeled",
                        args.iter().map(|a| a.place.clone()).collect(),
                    );
                    return target;
                }
                let ty = signature.returns.clone();
                for (i, a) in args.iter().enumerate() {
                    self.observations.push((id.clone(), Some(i), a.ty.clone()));
                }
                let args = args.into_iter().map(|a| a.place).collect();
                let unwind = self.unwind_block(n, self.exception_destination(), None);
                let operation = Kind::Invoke {
                    target: Some(target.place.clone()),
                    callee: id,
                    args,
                    unwind,
                };
                self.emit(n, operation);
                return Value { ty, ..target };
            }
        }
        if let Some(r) = receiver {
            let method = text(&self.s.text, field(callee, "attribute").unwrap());
            if r.ty == ValueType::HashDigest
                && !keyword
                && ((method == "update" && args.len() == 1 && args[0].ty == ValueType::FileData)
                    || (method == "hexdigest" && args.is_empty()))
            {
                self.emit_fallible_effect(
                    n,
                    Kind::LibraryEffect {
                        model: format!("stdlib.hash.{method}.v1"),
                    },
                );
                return Value {
                    ty: ValueType::Scalar,
                    ..target
                };
            }
            if r.ty == ValueType::Path && method == "resolve" && args.is_empty() && !keyword {
                self.emit_fallible_effect(
                    n,
                    Kind::LibraryEffect {
                        model: "stdlib.path.resolve.exact.v1".into(),
                    },
                );
                return Value {
                    ty: ValueType::Path,
                    ..target
                };
            }
            if r.ty == ValueType::Path && method == "mkdir" {
                let mut names = BTreeSet::new();
                let safe = arg_nodes.iter().all(|arg| {
                    if arg.kind() != "keyword_argument" {
                        return false;
                    }
                    let (Some(name), Some(value)) = (field(*arg, "name"), field(*arg, "value"))
                    else {
                        return false;
                    };
                    let name = text(&self.s.text, name);
                    names.insert(name)
                        && match name {
                            "parents" | "exist_ok" => matches!(value.kind(), "true" | "false"),
                            "mode" => exact_integer_expression(value, &self.s.text),
                            _ => false,
                        }
                });
                if safe {
                    self.emit_fallible_effect(
                        n,
                        Kind::LibraryEffect {
                            model: "stdlib.path.mkdir.literal_options.v1".into(),
                        },
                    );
                    return Value {
                        ty: ValueType::Scalar,
                        ..target
                    };
                }
            }
            if r.ty == ValueType::Path && method == "open" {
                if unsafe_library_binding {
                    self.unknown(
                        n,
                        "Path.open unsupported argument binding effects are unresolved",
                        vec![],
                    );
                }
                self.emit_fallible_effect(
                    n,
                    Kind::Acquire {
                        target: target.place.clone(),
                    },
                );
                return Value {
                    ty: ValueType::File,
                    ..target
                };
            }
            if r.ty == ValueType::BytesBuffer
                && method == "getbuffer"
                && args.is_empty()
                && !keyword
            {
                self.emit_fallible_effect(
                    n,
                    Kind::Borrow {
                        source: r.place,
                        target: target.place.clone(),
                        mutable: true,
                    },
                );
                return Value {
                    ty: ValueType::BufferView { mutable: true },
                    ..target
                };
            }
            if matches!(r.ty, ValueType::BufferView { .. }) && args.is_empty() && !keyword {
                match method {
                    "release" => {
                        self.emit(n, Kind::EndBorrow { value: r.place });
                        return Value {
                            ty: ValueType::Scalar,
                            ..target
                        };
                    }
                    "toreadonly" => {
                        self.emit_fallible_effect(
                            n,
                            Kind::Borrow {
                                source: r.place,
                                target: target.place.clone(),
                                mutable: false,
                            },
                        );
                        return Value {
                            ty: ValueType::BufferView { mutable: false },
                            ..target
                        };
                    }
                    "tobytes" | "tolist" | "hex" => {
                        self.emit_fallible_effect(n, Kind::Read { value: r.place });
                        return Value {
                            ty: ValueType::Scalar,
                            ..target
                        };
                    }
                    _ => {}
                }
            }
            if r.ty == ValueType::TextWrapper && method == "detach" && args.is_empty() && !keyword {
                if !self.cleanup.is_empty() {
                    self.unknown(n,"Detach inside active context cleanup has unresolved cleanup receiver protocol",vec![r.place.clone()]);
                }
                self.emit_fallible_effect(
                    n,
                    Kind::Read {
                        value: r.place.clone(),
                    },
                );
                self.emit_fallible_effect(
                    n,
                    Kind::LibraryEffect {
                        model: "stdlib.text_wrapper.detach.ownership_policy.v1".into(),
                    },
                );
                self.emit(
                    n,
                    Kind::TransferTo {
                        source: r.place.clone(),
                        target: target.place.clone(),
                    },
                );
                return Value {
                    ty: ValueType::BytesBuffer,
                    ..target
                };
            }
            if matches!(
                r.ty,
                ValueType::File | ValueType::BytesBuffer | ValueType::TextWrapper
            ) {
                let kind = match method {
                    "close" => Some(if r.ty == ValueType::TextWrapper {
                        Kind::CloseIfOwned {
                            value: r.place.clone(),
                        }
                    } else if r.ty == ValueType::BytesBuffer {
                        Kind::CloseIfUnborrowed {
                            value: r.place.clone(),
                        }
                    } else {
                        Kind::Close {
                            value: r.place.clone(),
                        }
                    }),
                    "read" | "readline" | "readlines" | "seek" | "tell" | "flush" | "fileno" => {
                        Some(Kind::Read {
                            value: r.place.clone(),
                        })
                    }
                    "write" | "writelines" | "truncate" => Some(Kind::Write {
                        value: r.place.clone(),
                    }),
                    _ => None,
                };
                if let Some(kind) = kind {
                    if args.iter().any(|a| a.ty == ValueType::Unknown) || keyword {
                        self.unknown(
                            n,
                            "File argument conversion effects are unresolved",
                            vec![r.place.clone()],
                        );
                    }
                    self.emit_fallible_effect(n, kind);
                    return Value {
                        ty: if matches!(method, "read" | "readline") {
                            ValueType::FileData
                        } else {
                            ValueType::Scalar
                        },
                        ..target
                    };
                }
            }
            args.push(r);
        }
        self.unknown(
            n,
            format!(
                "Unresolved call effect: {}",
                symbol.unwrap_or_else(|| text(&self.s.text, callee).into())
            ),
            args.into_iter().map(|a| a.place).collect(),
        );
        target
    }
    fn statements(&mut self, n: Node<'_>) {
        for c in kids(n) {
            if !matches!(self.blocks[self.at].terminator, Terminator::Stop) {
                break;
            }
            self.statement(c);
        }
    }
    fn direct_field_class(&self, ty: &ValueType, name: &str) -> Option<String> {
        let ValueType::Instance(class) = ty else {
            return None;
        };
        (self.facts.classes.contains(class)
            && !self.facts.blocked_class_dispatch.contains(&self.id)
            && !name.starts_with("__")
            && !self
                .facts
                .functions
                .contains_key(&format!("{class}.{name}")))
        .then(|| class.clone())
    }
    fn bind(&mut self, n: Node<'_>, v: Value) {
        if n.kind() == "identifier" {
            let name = text(&self.s.text, n).to_string();
            self.symbols.remove(&name);
            self.types.insert(name.clone(), v.ty);
            self.emit(
                n,
                Kind::Assign {
                    target: Place::local(name),
                    source: v.place,
                },
            );
        } else if n.kind() == "subscript" {
            let obj = self.expr(field(n, "value").unwrap());
            let index = field(n, "subscript").unwrap();
            if matches!(obj.ty, ValueType::BufferView { .. })
                && exact_integer_expression(index, &self.s.text)
                && matches!(
                    v.ty,
                    ValueType::Scalar | ValueType::Text | ValueType::JsonData
                )
            {
                self.emit_fallible_effect(n, Kind::Write { value: obj.place });
            } else {
                self.expr(index);
                self.emit(
                    n,
                    Kind::GlobalMutationUnknown {
                        reason: "Unresolved indexed write may replace bindings or invoke user code"
                            .into(),
                    },
                );
            }
        } else if n.kind() == "attribute" {
            let obj = self.expr(field(n, "object").unwrap());
            let name = text(&self.s.text, field(n, "attribute").unwrap()).to_string();
            if let Some(class) = self.direct_field_class(&obj.ty, &name) {
                self.field_observations.push(((class, name.clone()), v.ty));
                let mut target = obj.place;
                target.projections.push(name);
                // Ordinary slot insertion may fail before updating the heap;
                // handlers must retain the old field on that exceptional edge.
                self.emit_fallible_effect(
                    n,
                    Kind::LibraryEffect {
                        model: "python.plain_field.store.v1".into(),
                    },
                );
                self.emit(
                    n,
                    Kind::Assign {
                        target,
                        source: v.place,
                    },
                );
            } else {
                self.emit(n,Kind::GlobalMutationUnknown {reason:"Field write protocol or method binding is unresolved; library replacement cannot be excluded".into()});
            }
        } else {
            self.emit(
                n,
                Kind::GlobalMutationUnknown {
                    reason: "Projected/destructured assignment may replace unresolved bindings"
                        .into(),
                },
            );
        }
    }
    fn cleanup_to(&mut self, n: Node<'_>, depth: usize) {
        let saved = self.cleanup.clone();
        if saved.len().saturating_sub(depth) > 8 {
            self.unknown(n, "Cleanup expansion bound exceeded", vec![]);
            for effect in saved[depth..].iter().rev() {
                self.emit(n, effect.clone());
            }
            return;
        }
        while self.cleanup.len() > depth {
            let effect = self.cleanup.pop().unwrap();
            if matches!(effect, Kind::EndBorrow { .. }) {
                self.emit(n, effect);
            } else {
                self.emit_fallible_effect(n, effect);
            }
        }
        self.cleanup = saved;
    }
    fn cleanup(&mut self, n: Node<'_>) {
        self.cleanup_to(n, 0);
    }
    fn block(&mut self) -> usize {
        let i = self.blocks.len();
        self.blocks.push(Block {
            operations: vec![],
            terminator: Terminator::Stop,
        });
        i
    }
    fn definition_effects(&mut self, n: Node<'_>) {
        if let Some(parameters) = field(n, "parameters") {
            for p in kids(parameters) {
                if let Some(value) = field(p, "value") {
                    self.expr(value);
                }
                if let Some(ty) = field(p, "type")
                    && contains_call(ty)
                {
                    self.unknown(ty, "Annotation evaluation effects are not modeled", vec![]);
                }
            }
        }
        if let Some(ty) = field(n, "return_type")
            && contains_call(ty)
        {
            self.unknown(
                ty,
                "Return annotation evaluation effects are not modeled",
                vec![],
            );
        }
    }
    fn statement(&mut self, n: Node<'_>) {
        match n.kind() {
            "comment" | "pass_statement" => {}
            "import_statement" | "import_from_statement" => {
                if !import_is_inert(self.s, n, &self.inert, &self.local_modules) {
                    self.unknown(
                        n,
                        "Imported module initialization effects are unresolved",
                        vec![],
                    );
                }
                import_stmt(self.s, n, &mut self.symbols);
            }
            "expression_statement" => {
                for c in kids(n) {
                    if matches!(c.kind(), "assignment" | "augmented_assignment") {
                        self.statement(c);
                    } else {
                        self.expr(c);
                    }
                }
            }
            "delete_statement" | "augmented_assignment" => {
                self.emit(n,Kind::GlobalMutationUnknown{reason:"Deletion or augmented assignment has unresolved binding mutation effects".into()});
            }
            "assignment" => {
                if let (Some(l), Some(r)) = (field(n, "left"), field(n, "right")) {
                    let v = self.expr(r);
                    self.bind(l, v);
                } else {
                    self.unknown(n, "Incomplete assignment", vec![]);
                }
            }
            "return_statement" => {
                let v = kids(n).first().map(|c| self.expr(*c));
                if let Some(v) = &v {
                    self.observations
                        .push((self.id.clone(), None, v.ty.clone()));
                }
                self.cleanup(n);
                let site = self.site(n);
                self.blocks[self.at].terminator = Terminator::Return {
                    value: v.map(|v| v.place),
                    site,
                };
            }
            "raise_statement" => {
                let values = kids(n);
                if let Some(value) = values.first() {
                    let target = if value.kind() == "call" {
                        field(*value, "function").unwrap_or(*value)
                    } else {
                        *value
                    };
                    let category = match self.symbol(target).as_deref() {
                        Some(
                            "builtins.Exception" | "builtins.ValueError" | "builtins.TypeError",
                        ) => Some(1),
                        Some(
                            "builtins.BaseException"
                            | "builtins.KeyboardInterrupt"
                            | "builtins.SystemExit",
                        ) => Some(2),
                        _ => None,
                    };
                    if let Some(categories) = category {
                        if let Some(args) = field(*value, "arguments") {
                            for arg in kids(args) {
                                self.expr(arg);
                            }
                        }
                        self.emit(n, Kind::SetException { categories });
                    } else {
                        for value in values {
                            self.expr(value);
                        }
                        self.unknown(n, "Raised exception identity is unresolved", vec![]);
                        self.emit(n, Kind::SetException { categories: 4 });
                    }
                }
                self.raise_edge(n);
            }
            "with_statement" => self.with_statement(n),
            "if_statement" => self.if_statement(n),
            "while_statement" => self.while_statement(n),
            "for_statement" => self.sentinel_for(n),
            "break_statement" => self.loop_jump(n, false),
            "continue_statement" => self.loop_jump(n, true),
            "try_statement" => self.try_statement(n),
            "function_definition" | "class_definition" | "decorated_definition" => {
                self.unknown(
                    n,
                    "Nested/decorated/class definition requires project provider",
                    vec![],
                );
            }
            _ => {
                self.unknown(
                    n,
                    format!("Unsupported statement/control flow: {}", n.kind()),
                    vec![],
                );
            }
        }
    }
    fn raise_edge(&mut self, n: Node<'_>) {
        let target = self.unwind_block(n, self.exception_destination(), None);
        self.blocks[self.at].terminator = Terminator::Jump { target };
    }
    fn handler_try(&mut self, n: Node<'_>, handlers: &[Node<'_>]) {
        let handler_categories: Vec<_> = handlers
            .iter()
            .map(|h| {
                let head: Vec<_> = kids(*h)
                    .into_iter()
                    .filter(|c| !matches!(c.kind(), "block" | "comment"))
                    .collect();
                if head.len() == 1 && head[0].kind() == "identifier" {
                    fn mentions(n: Node<'_>, source: &str, name: &str) -> bool {
                        n.kind() == "identifier" && text(source, n) == name
                            || kids(n).into_iter().any(|c| mentions(c, source, name))
                    }
                    if field(n, "body").is_some_and(|body| {
                        mentions(body, &self.s.text, text(&self.s.text, head[0]))
                    }) {
                        return None;
                    }
                    match self.symbol(head[0]).as_deref() {
                        Some("builtins.Exception") => Some(1),
                        Some("builtins.BaseException") => Some(3),
                        _ => None,
                    }
                } else {
                    None
                }
            })
            .collect();
        let entry_types = self.types.clone();
        let mut rebound_names = BTreeSet::new();
        rebound(n, &self.s.text, &mut rebound_names);
        let caught = self.block();
        let join = self.block();
        self.exception_targets.push((caught, self.cleanup.len()));
        if let Some(body) = field(n, "body") {
            self.statements(body);
        }
        self.exception_targets.pop();
        // else is outside this handler's protected region.
        if matches!(self.blocks[self.at].terminator, Terminator::Stop) {
            if let Some(otherwise) = kids(n).into_iter().find(|c| c.kind() == "else_clause")
                && let Some(body) = field(otherwise, "body")
            {
                self.statements(body);
            }
            if matches!(self.blocks[self.at].terminator, Terminator::Stop) {
                self.blocks[self.at].terminator = Terminator::Jump { target: join };
            }
        }
        // Preserve only pre-try types whose bindings cannot change anywhere in
        // the protected/handler/else region. Resource state is still joined by
        // the CFG solver; a stable File type does not mean an open resource.
        let body_names: Vec<_> = self.types.keys().cloned().collect();
        let mut handler_types = entry_types;
        for name in body_names {
            handler_types.entry(name).or_insert(ValueType::Unknown);
        }
        for name in rebound_names {
            handler_types.insert(name, ValueType::Unknown);
        }
        self.types = handler_types.clone();
        self.symbols.clear();
        let mut dispatch = caught;
        for (handler_index, handler) in handlers.iter().enumerate() {
            self.at = dispatch;
            self.types = handler_types.clone();
            self.symbols.clear();
            let header: Vec<_> = kids(*handler)
                .into_iter()
                .filter(|c| !matches!(c.kind(), "block" | "comment"))
                .collect();
            let body_block = self.block();
            let next = self.block();
            if header.is_empty() {
                self.blocks[self.at].terminator = Terminator::Jump { target: body_block };
            } else if let Some(categories) = handler_categories[handler_index] {
                let site = self.site(*handler);
                self.blocks[self.at].terminator = Terminator::ExceptionMatch {
                    categories,
                    matched: body_block,
                    unmatched: next,
                    site,
                };
            } else {
                // Runtime exception identities are not yet available. Preserve
                // ordered possible match and non-match, with explicit uncertainty.
                fn invalidate_names(
                    n: Node<'_>,
                    source: &str,
                    types: &mut BTreeMap<String, ValueType>,
                ) {
                    if n.kind() == "identifier" {
                        types.insert(text(source, n).into(), ValueType::Unknown);
                    }
                    for child in kids(n) {
                        invalidate_names(child, source, types);
                    }
                }
                for part in header {
                    self.expr(part);
                    invalidate_names(part, &self.s.text, &mut self.types);
                }
                self.unknown(
                    *handler,
                    "Exception type matching/binding is unresolved",
                    vec![],
                );
                self.blocks[self.at].terminator = Terminator::Branch {
                    then_target: body_block,
                    else_target: next,
                };
            }
            self.at = body_block;
            if let Some(body) = kids(*handler).into_iter().find(|c| c.kind() == "block") {
                self.statements(body);
            } else {
                self.unknown(*handler, "Exception handler body is missing", vec![]);
            }
            if matches!(self.blocks[self.at].terminator, Terminator::Stop) {
                self.blocks[self.at].terminator = Terminator::Jump { target: join };
            }
            dispatch = next;
        }
        self.at = dispatch;
        self.raise_edge(n); // unmatched exception propagates to the outer region
        // New or rebound names remain unknown at the join; unchanged entry
        // bindings retain only type identity, never a resource-state assertion.
        for name in self.types.keys() {
            handler_types
                .entry(name.clone())
                .or_insert(ValueType::Unknown);
        }
        self.types = handler_types;
        self.symbols.clear();
        self.at = join;
    }
    fn try_statement(&mut self, n: Node<'_>) {
        if kids(n).iter().any(|c| c.kind() == "finally_clause") {
            self.unknown(n, "Try/finally cleanup lowering is not modeled", vec![]);
            return;
        }
        let handlers: Vec<_> = kids(n)
            .into_iter()
            .filter(|c| c.kind() == "except_clause")
            .collect();
        if !handlers.is_empty() {
            self.handler_try(n, &handlers);
            return;
        }
        self.unknown(
            n,
            "Try exception dispatch and handler paths are not modeled; normal path only",
            vec![],
        );
        self.observations
            .push((self.id.clone(), None, ValueType::Unknown));
        if let Some(body) = field(n, "body") {
            self.statements(body);
        }
        if matches!(self.blocks[self.at].terminator, Terminator::Stop) {
            self.unknown(
                n,
                "Excluded exception-handler effects may reach continuation",
                vec![],
            );
            for ty in self.types.values_mut() {
                *ty = ValueType::Unknown;
            }
            self.symbols.clear();
        }
    }
    fn with_statement(&mut self, n: Node<'_>) {
        if text(&self.s.text, n).trim_start().starts_with("async ") {
            self.unknown(n, "Async context manager is not modeled", vec![]);
            return;
        }

        let old = self.cleanup.len();
        for clause in kids(n).into_iter().filter(|x| x.kind() == "with_clause") {
            for item in kids(clause) {
                let value = field(item, "value").unwrap_or(item);
                let (expr, alias) = if value.kind() == "as_pattern" {
                    (
                        kids(value).first().copied().unwrap_or(value),
                        field(value, "alias"),
                    )
                } else {
                    (value, field(item, "alias"))
                };
                let v = self.expr(expr);
                if matches!(
                    v.ty,
                    ValueType::File
                        | ValueType::BytesBuffer
                        | ValueType::TextWrapper
                        | ValueType::BufferView { .. }
                ) {
                    if matches!(v.ty, ValueType::BufferView { .. }) {
                        // Exact memoryview.__enter__ failure means an already released
                        // view. Keep the real handler edge without inventing a live-loan
                        // failure that would falsely block the handler's owner access.
                        self.emit(
                            item,
                            Kind::Read {
                                value: v.place.clone(),
                            },
                        );
                        let original = self.at;
                        let failed = self.block();
                        self.at = failed;
                        self.emit(
                            item,
                            Kind::AssumeBorrowEnded {
                                value: v.place.clone(),
                            },
                        );
                        let unwind = self.unwind_block(item, self.exception_destination(), Some(1));
                        self.blocks[self.at].terminator = Terminator::Jump { target: unwind };
                        let success = self.block();
                        self.blocks[original].terminator = Terminator::Branch {
                            then_target: success,
                            else_target: failed,
                        };
                        self.at = success;
                    } else {
                        self.emit_fallible_effect(
                            item,
                            Kind::Read {
                                value: v.place.clone(),
                            },
                        );
                    }
                    self.cleanup.push(match v.ty {
                        ValueType::BufferView { .. } => Kind::EndBorrow {
                            value: v.place.clone(),
                        },
                        ValueType::BytesBuffer => Kind::CloseIfUnborrowed {
                            value: v.place.clone(),
                        },
                        ValueType::TextWrapper => Kind::CloseIfOwned {
                            value: v.place.clone(),
                        },
                        _ => Kind::Close {
                            value: v.place.clone(),
                        },
                    });
                    if let Some(a) = alias {
                        let a = if a.kind() == "as_pattern_target" {
                            kids(a).first().copied().unwrap_or(a)
                        } else {
                            a
                        };
                        self.bind(a, v);
                    }
                } else {
                    self.unknown(
                        item,
                        "Context manager enter/exit effects are unresolved",
                        vec![v.place],
                    );
                }
            }
        }
        if let Some(body) = field(n, "body") {
            self.statements(body);
        }
        if matches!(self.blocks[self.at].terminator, Terminator::Stop) {
            self.cleanup_to(n, old);
        }
        self.cleanup.truncate(old);
    }
    fn loop_jump(&mut self, n: Node<'_>, is_continue: bool) {
        let Some(targets) = self.loops.last().copied() else {
            self.unknown(n, "Loop jump outside a modeled loop", vec![]);
            return;
        };
        // Exit only contexts created inside this loop. Outer with scopes stay open.
        self.cleanup_to(n, targets.cleanup_depth);
        self.blocks[self.at].terminator = Terminator::Jump {
            target: if is_continue {
                targets.header
            } else {
                targets.exit
            },
        };
    }
    fn sentinel_for(&mut self, n: Node<'_>) {
        // Bounded lowering of the builtin two-argument iterator. The lambda
        // itself has no defaults/parameters, so creation has no body effects.
        let shape = (|| {
            let right = field(n, "right")?;
            if right.kind() != "call"
                || self.symbol(field(right, "function")?).as_deref() != Some("builtins.iter")
                || n.child(0).is_some_and(|c| c.kind() == "async")
            {
                return None;
            }
            let args = kids(field(right, "arguments")?);
            if args.len() != 2
                || args[0].kind() != "lambda"
                || field(args[0], "parameters").is_some_and(|p| !kids(p).is_empty())
            {
                return None;
            }
            let sentinel = args[1];
            if !matches!(
                sentinel.kind(),
                "string" | "integer" | "float" | "true" | "false" | "none"
            ) || kids(sentinel).iter().any(|c| c.kind() == "interpolation")
            {
                return None;
            }
            let target = field(n, "left")?;
            if target.kind() != "identifier" {
                return None;
            }
            Some((field(args[0], "body")?, sentinel, target))
        })();
        let Some((callback, sentinel, target)) = shape else {
            self.unknown(
                n,
                "Unsupported for_statement: iterator protocol/callable binding is not modeled",
                vec![],
            );
            return;
        };
        self.expr(sentinel); // evaluated once, before the first next()
        self.lower_loop(n, callback, Some(target));
    }
    fn exact_file_read(&self, n: Node<'_>) -> bool {
        // Scalar annotations are not proof of exact runtime equality semantics.
        // Only a resolved standard file read with inert arguments qualifies.
        if n.kind() != "call" {
            return false;
        }
        let Some(callee) = field(n, "function") else {
            return false;
        };
        if callee.kind() != "attribute" {
            return false;
        }
        let Some(receiver) = field(callee, "object") else {
            return false;
        };
        let Some(method) = field(callee, "attribute") else {
            return false;
        };
        receiver.kind() == "identifier"
            && self.types.get(text(&self.s.text, receiver)) == Some(&ValueType::File)
            && self.symbol(callee).is_none()
            && matches!(text(&self.s.text, method), "read" | "readline")
            && field(n, "arguments").is_some_and(|a| {
                kids(a)
                    .iter()
                    .all(|v| exact_integer_expression(*v, &self.s.text))
            })
    }
    fn while_statement(&mut self, n: Node<'_>) {
        let Some(condition) = field(n, "condition") else {
            self.unknown(n, "While condition is missing", vec![]);
            return;
        };
        self.lower_loop(n, condition, None);
    }
    fn lower_loop(&mut self, n: Node<'_>, condition: Node<'_>, target: Option<Node<'_>>) {
        let body = field(n, "body");
        // A one-pass type environment is not a loop invariant. Remove facts for
        // any rebound name before lowering the header or body; don't reuse a
        // first-iteration File type after a later iteration stores a scalar.
        let mut names = BTreeSet::new();
        if let Some(body) = body {
            rebound(body, &self.s.text, &mut names);
        }
        if let Some(target) = target {
            names.insert(text(&self.s.text, target).into());
        }
        for name in &names {
            self.types.insert(name.clone(), ValueType::Unknown);
            self.symbols.remove(name);
        }
        let invariant_types = self.types.clone();
        let invariant_symbols = self.symbols.clone();
        let header = self.block();
        let body_block = self.block();
        let exhausted = self.block();
        let exit = self.block();
        self.blocks[self.at].terminator = Terminator::Jump { target: header };
        self.at = header;
        let equality_known = target.is_some() && self.exact_file_read(condition);
        let value = self.expr(condition);
        if target.is_some() && !equality_known {
            self.unknown(
                condition,
                "Iterator sentinel equality may have unresolved effects",
                vec![],
            );
        } else if target.is_none()
            && !matches!(
                value.ty,
                ValueType::Scalar | ValueType::FileData | ValueType::Text | ValueType::JsonData
            )
        {
            self.unknown(
                condition,
                "Loop condition truth conversion may have unresolved effects",
                vec![value.place.clone()],
            );
        }
        self.blocks[self.at].terminator = match (target.is_some(), condition.kind()) {
            (false, "true") => Terminator::Jump { target: body_block },
            (false, "false") => Terminator::Jump { target: exhausted },
            _ => Terminator::Branch {
                then_target: body_block,
                else_target: exhausted,
            },
        };
        self.loops.push(LoopTargets {
            header,
            exit,
            cleanup_depth: self.cleanup.len(),
        });
        self.at = body_block;
        if let Some(target) = target {
            self.bind(target, value);
        }
        if let Some(body) = body {
            self.statements(body);
        }
        if matches!(self.blocks[self.at].terminator, Terminator::Stop) {
            self.blocks[self.at].terminator = Terminator::Jump { target: header };
        }
        self.loops.pop();
        self.types = invariant_types.clone();
        self.symbols = invariant_symbols.clone();
        self.at = exhausted;
        if let Some(otherwise) = field(n, "alternative") {
            if let Some(body) = field(otherwise, "body") {
                self.statements(body);
            } else {
                self.unknown(otherwise, "Loop else shape is not modeled", vec![]);
            }
        }
        if matches!(self.blocks[self.at].terminator, Terminator::Stop) {
            self.blocks[self.at].terminator = Terminator::Jump { target: exit };
        }
        // Both break and exhaustion may reach this point; do not keep else-only facts.
        self.types = invariant_types
            .into_iter()
            .map(|(name, ty)| {
                let joined = if self.types.get(&name) == Some(&ty) {
                    ty
                } else {
                    ValueType::Unknown
                };
                (name, joined)
            })
            .collect();
        self.symbols = invariant_symbols
            .into_iter()
            .filter(|(name, target)| self.symbols.get(name) == Some(target))
            .collect();
        self.at = exit;
    }
    fn if_statement(&mut self, n: Node<'_>) {
        if let Some(c) = field(n, "condition") {
            let value = self.expr(c);
            if !matches!(
                value.ty,
                ValueType::Text | ValueType::JsonData | ValueType::FileData | ValueType::Scalar
            ) {
                self.unknown(
                    c,
                    "Conditional truth conversion may have unresolved effects",
                    vec![value.place],
                );
            }
        }
        let then_b = self.block();
        let else_b = self.block();
        let join = self.block();
        self.blocks[self.at].terminator = Terminator::Branch {
            then_target: then_b,
            else_target: else_b,
        };
        let types = self.types.clone();
        let symbols = self.symbols.clone();
        self.at = then_b;
        if let Some(b) = field(n, "consequence") {
            self.statements(b);
        }
        let then_types = self.types.clone();
        let then_symbols = self.symbols.clone();
        if matches!(self.blocks[self.at].terminator, Terminator::Stop) {
            self.blocks[self.at].terminator = Terminator::Jump { target: join };
        }
        self.at = else_b;
        self.types = types;
        self.symbols = symbols;
        if let Some(b) = field(n, "alternative") {
            if b.kind() == "else_clause" {
                if let Some(body) = field(b, "body") {
                    self.statements(body);
                }
            } else {
                self.unknown(b, "Elif branch lowering is not modeled", vec![]);
            }
        }
        if matches!(self.blocks[self.at].terminator, Terminator::Stop) {
            self.blocks[self.at].terminator = Terminator::Jump { target: join };
        }
        for (k, t) in &mut self.types {
            if then_types.get(k) != Some(t) {
                *t = ValueType::Unknown;
            }
        }
        self.symbols.retain(|k, v| then_symbols.get(k) == Some(v));
        self.at = join;
    }
}
