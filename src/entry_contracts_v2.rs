//! Explicit caller assertions for standalone entry analysis, never inferred facts.
//! Admission binds assertions to exact source snapshots. Comparison fingerprints
//! bind their semantics (not source hashes), allowing a freshly admitted repair.
use crate::{
    frontend_v2::{self, FunctionFacts, SemanticFacts, ValueType},
    v2_ir::{Kind, Program},
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{collections::BTreeSet, path::Path};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub schema_version: u32,
    pub entries: Vec<Entry>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entry {
    pub path: String,
    pub source_sha256: String,
    pub symbol: String,
    pub parameters: Vec<Parameter>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Parameter {
    pub name: String,
    pub kind: ParameterKind,
}
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParameterKind {
    /// Exact standard platform Path; subclass dispatch is outside this assertion.
    ExactStdlibPath,
}
impl Bundle {
    pub fn lower(&self, sources: &[(String, String)], roots: &[String]) -> Result<Program, String> {
        if self.schema_version != 1 || self.entries.is_empty() {
            return Err("Entry contract requires schema_version 1 and nonempty entries".into());
        }
        let import_root = Path::new("");
        let declarations = frontend_v2::declaration_index(sources, import_root)?;
        let baseline = frontend_v2::lower_project_with_root(sources, import_root)?;
        let roots: BTreeSet<_> = if roots.is_empty() {
            baseline.roots.iter().collect()
        } else {
            roots.iter().collect()
        };
        let mut facts = SemanticFacts::default();
        let mut seen = BTreeSet::new();
        for entry in &self.entries {
            if !seen.insert(&entry.symbol) || entry.parameters.is_empty() {
                return Err("Duplicate entry contract or empty parameter assertions".into());
            }
            if !roots.contains(&entry.symbol) {
                return Err(format!(
                    "Contracted entry {} must be selected as a root",
                    entry.symbol
                ));
            }
            let source = sources
                .iter()
                .find(|(p, _)| p == &entry.path)
                .ok_or_else(|| format!("Entry contract source not found: {}", entry.path))?;
            if format!("{:x}", Sha256::digest(source.1.as_bytes())) != entry.source_sha256 {
                return Err(format!(
                    "Entry contract source snapshot mismatch: {}",
                    entry.path
                ));
            }
            let matches: Vec<_> = declarations
                .iter()
                .filter(|d| d.symbol == entry.symbol)
                .collect();
            if matches.len() != 1 || matches[0].path != entry.path {
                return Err(format!(
                    "Entry contract needs one admitted declaration with matching path: {}",
                    entry.symbol
                ));
            }
            let function = baseline
                .functions
                .iter()
                .find(|f| f.id == entry.symbol)
                .ok_or("Contracted function was not lowered")?;
            let mut function_facts = FunctionFacts {
                parameters: vec![ValueType::Unknown; function.params.len()],
                ..Default::default()
            };
            let mut names = BTreeSet::new();
            for parameter in &entry.parameters {
                if !names.insert(&parameter.name) {
                    return Err("Duplicate entry parameter assertion".into());
                }
                let index = function
                    .params
                    .iter()
                    .position(|p| p.root == parameter.name)
                    .ok_or_else(|| {
                        format!(
                            "Unknown entry parameter {}::{}",
                            entry.symbol, parameter.name
                        )
                    })?;
                function_facts.parameters[index] = match parameter.kind {
                    ParameterKind::ExactStdlibPath => ValueType::Path,
                };
            }
            facts.functions.insert(entry.symbol.clone(), function_facts);
        }
        facts.unbound_entries = roots
            .iter()
            .filter(|id| !seen.contains(**id))
            .map(|id| (**id).clone())
            .collect();
        let program = frontend_v2::lower_project_with_facts(sources, import_root, &facts)?;
        // Facts currently specialize a whole function, not individual contexts.
        // Reject internal callers rather than leaking an entry-only assertion.
        for function in &program.functions {
            for operation in function.blocks.iter().flat_map(|b| &b.operations) {
                if let Kind::Call { callee, .. } | Kind::Invoke { callee, .. } = &operation.kind
                    && seen.contains(callee)
                {
                    return Err(format!(
                        "Entry-only contract cannot specialize internally called function {callee}; call-context specialization is not implemented"
                    ));
                }
            }
        }
        Ok(program)
    }
    pub fn assumptions(&self) -> Vec<String> {
        let mut assumptions = Vec::new();
        for entry in &self.entries {
            for parameter in &entry.parameters {
                assumptions.push(format!("Caller-supplied assumption (not inferred or runtime-checked): {} parameter {} is an exact standard pathlib.Path instance; custom subclasses are excluded",entry.symbol,parameter.name));
            }
        }
        assumptions.sort();
        assumptions
    }
    pub fn fingerprint(&self) -> String {
        let mut semantic: Vec<_> = self
            .entries
            .iter()
            .flat_map(|e| {
                e.parameters
                    .iter()
                    .map(|p| (e.path.clone(), e.symbol.clone(), p.name.clone(), p.kind))
            })
            .collect();
        semantic.sort_by(|a, b| (&a.0, &a.1, &a.2).cmp(&(&b.0, &b.1, &b.2)));
        let bytes = serde_json::to_vec(&(self.schema_version, semantic))
            .expect("entry contract serialization");
        format!("entry-contract-v1:{:x}", Sha256::digest(bytes))
    }
}
