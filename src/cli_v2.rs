//! Experimental v2 entry points, kept separate from the v1 check command.
use clap::{Args, ValueEnum};
use lint_like_rust::{diagnostics_v2 as d, frontend_v2, report_v2, solver_v2};
use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::{Path, PathBuf},
};

#[derive(Clone, Copy, ValueEnum)]
pub enum Format {
    Text,
    Json,
}
#[derive(Args)]
pub struct AnalyzeArgs {
    /// Python project root, or one file (its parent is the import root).
    pub root: PathBuf,
    /// Qualified entry function(s); defaults to the frontend's root inventory.
    #[arg(long)]
    pub entry: Vec<String>,
    #[arg(long, value_enum, default_value = "text")]
    pub format: Format,
    #[arg(long)]
    pub output: Option<PathBuf>,
    /// Optional normalized nominal type evidence; does not authorize exact dispatch.
    #[arg(long)]
    pub type_evidence: Option<PathBuf>,
    /// Explicit caller assertions for entry parameters; verification is conditional on them.
    #[arg(long)]
    pub entry_contract: Option<PathBuf>,
}
#[derive(Args)]
pub struct LintArgs {
    /// Python project root, or one Python file.
    pub root: PathBuf,
    /// Qualified entry function(s); defaults to every discovered function and method.
    #[arg(long)]
    pub entry: Vec<String>,
    #[arg(long, value_enum, default_value = "text")]
    pub format: Format,
}
#[derive(Args)]
pub struct CompareArgs {
    pub before: PathBuf,
    pub after: PathBuf,
    #[arg(long)]
    pub output: Option<PathBuf>,
}
fn emit(output: &Option<PathBuf>, text: &str) -> Result<(), String> {
    if let Some(path) = output {
        fs::write(path, format!("{text}\n")).map_err(|e| format!("{}: {e}", path.display()))
    } else {
        println!("{text}");
        Ok(())
    }
}
fn collect(path: &Path) -> Result<Vec<(String, String)>, String> {
    let path = fs::canonicalize(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let root = if path.is_file() {
        path.parent().ok_or("file has no parent")?
    } else {
        &path
    };
    let mut files = BTreeSet::new();
    if path.is_file() {
        files.insert(path.clone());
    } else {
        let mut walk = ignore::WalkBuilder::new(&path);
        walk.filter_entry(|e| {
            !matches!(
                e.file_name().to_str(),
                Some(".git" | ".venv" | "venv" | "node_modules" | "target" | "__pycache__")
            )
        });
        for entry in walk.build() {
            let entry = entry.map_err(|e| e.to_string())?;
            if entry.file_type().is_some_and(|t| t.is_file())
                && entry.path().extension().is_some_and(|e| e == "py")
            {
                files.insert(entry.into_path());
            }
        }
    }
    if files.is_empty() {
        return Err("No Python sources discovered".into());
    }
    files
        .into_iter()
        .map(|p| {
            if p.extension().is_none_or(|e| e != "py") {
                return Err("Input must be a Python source or directory".into());
            }
            let name = p
                .strip_prefix(root)
                .map_err(|e| e.to_string())?
                .to_string_lossy()
                .replace('\\', "/");
            let source = fs::read_to_string(&p).map_err(|e| format!("{}: {e}", p.display()))?;
            Ok((name, source))
        })
        .collect()
}
fn analyze_inner(args: &AnalyzeArgs) -> Result<i32, String> {
    let sources = collect(&args.root)?;
    let type_evidence = args
        .type_evidence
        .as_ref()
        .map(|path| {
            let bytes = fs::read(path).map_err(|e| e.to_string())?;
            let bundle: lint_like_rust::type_evidence_v2::Bundle =
                serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            bundle.validate(&sources)?;
            Ok::<_, String>(bundle)
        })
        .transpose()?;
    let declaration_matches = type_evidence
        .as_ref()
        .map(|b| b.map_declarations(&sources))
        .transpose()?;
    let entry_contract = args
        .entry_contract
        .as_ref()
        .map(|path| {
            let bytes = fs::read(path).map_err(|e| e.to_string())?;
            serde_json::from_slice::<lint_like_rust::entry_contracts_v2::Bundle>(&bytes)
                .map_err(|e| e.to_string())
        })
        .transpose()?;
    let mut program = if let Some(bundle) = &entry_contract {
        bundle.lower(&sources, &args.entry)?
    } else {
        let facts = frontend_v2::SemanticFacts {
            unbound_entries: args.entry.iter().cloned().collect(),
            ..Default::default()
        };
        frontend_v2::lower_project_with_facts(&sources, Path::new(""), &facts)?
    };
    if !args.entry.is_empty() {
        for name in &args.entry {
            if !program.functions.iter().any(|f| &f.id == name) {
                return Err(format!(
                    "Unknown entry {name}; available: {}",
                    program
                        .functions
                        .iter()
                        .map(|f| f.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
        program.roots = args.entry.clone();
    }
    let analysis = solver_v2::analyze(&program);
    let context = d::AnalysisContext {
        semantics_revision: "v2-resource-cfg-23".into(),
        configuration_fingerprint: format!("{};entries={:?}", entry_contract.as_ref().map_or_else(||"default-no-user-contracts".into(),|b|b.fingerprint()),program.roots),
        environment_fingerprint:
            "python-closed-subset-23;standard-file-model-1;bytesio-management-ownership-policy-1;buffer-view-borrow-policy-1;plain-json-data-provenance-1;no-external-type-provider".into(),
    };
    let mut report = report_v2::build_report(&analysis, context);
    if let Some(bundle) = &entry_contract {
        let assumptions = bundle.assumptions();
        for obligation in &mut report.obligations {
            obligation.assumptions.extend(assumptions.clone());
        }
    }
    let verified = report
        .obligations
        .iter()
        .filter(|o| o.status == d::ProofStatus::Verified)
        .count();
    let violated = report
        .obligations
        .iter()
        .filter(|o| o.status == d::ProofStatus::Violated)
        .count();
    let unknown = report
        .obligations
        .iter()
        .filter(|o| o.status == d::ProofStatus::Unverified)
        .count();
    let status = if violated > 0 {
        1
    } else if unknown > 0 || !report.gaps.is_empty() || report.obligations.is_empty() {
        3
    } else {
        0
    };
    let output = match args.format {
        Format::Json => {
            let mut value = serde_json::to_value(&report).map_err(|e| e.to_string())?;
            if let Some(bundle) = &entry_contract {
                value["caller_supplied_entry_contract"] =
                    serde_json::to_value(bundle).map_err(|e| e.to_string())?;
                value["verification_basis"] = "conditional_on_explicit_caller_assumptions".into();
            }
            if let Some(bundle) = &type_evidence {
                value["nominal_declaration_matches"] =
                    serde_json::to_value(&declaration_matches).map_err(|e| e.to_string())?;
                value["nominal_type_evidence"] =
                    serde_json::to_value(bundle).map_err(|e| e.to_string())?;
            }
            serde_json::to_string_pretty(&value).map_err(|e| e.to_string())?
        }
        Format::Text => {
            let mut lines = vec![
                "V2 experimental resource analysis; verification is limited to listed obligations."
                    .into(),
            ];
            if let Some(bundle) = &entry_contract {
                lines.extend(bundle.assumptions());
            }
            if let Some(bundle) = &type_evidence {
                lines.push(format!("{} snapshot-validated nominal type facts attached; no dispatch/effect proof granted", bundle.facts.len()));
            }
            for o in &report.obligations {
                if o.status != d::ProofStatus::Verified {
                    lines.push(format!(
                        "{:?}: {} [{}]",
                        o.status,
                        o.key.issue_id(),
                        o.key.rule
                    ));
                    if let Some(e) = &o.evidence {
                        lines.push(format!(
                            "  {}:{}:{}: {}",
                            e.primary.path, e.primary.line, e.primary.column, e.message
                        ));
                        for t in &e.trace {
                            lines.push(format!(
                                "  {}:{}:{}: {}",
                                t.location.path, t.location.line, t.location.column, t.explanation
                            ));
                        }
                    }
                }
            }
            lines.extend(
                report
                    .gaps
                    .iter()
                    .map(|g| format!("UNVERIFIED: {}", g.reason)),
            );
            lines.push(format!("{} files; {verified} verified obligations; {violated} violations; {unknown} unverified obligations; {} gaps; exit {status}",sources.len(),report.gaps.len()));
            lines.join("\n")
        }
    };
    emit(&args.output, &output)?;
    Ok(status)
}

#[derive(serde::Serialize)]
struct LintLocation {
    path: String,
    line: usize,
    column: usize,
}

#[derive(serde::Serialize)]
struct LintEvidenceStep {
    path: String,
    line: usize,
}

#[derive(serde::Serialize)]
struct LintFinding {
    rule: String,
    location: LintLocation,
    reason: String,
    evidence_chain: Vec<LintEvidenceStep>,
    suggested_fix: String,
}

#[derive(serde::Serialize)]
struct LintReport {
    schema_version: u32,
    files: usize,
    findings: Vec<LintFinding>,
}

fn suggested_fix(rule: &str) -> &'static str {
    if rule == "LIFE001" {
        "Move the access before the modeled close, or acquire a fresh valid resource while preserving cleanup."
    } else if rule.starts_with("BOR") {
        "Release the conflicting view before this operation and preserve readonly and ownership restrictions."
    } else if rule.starts_with("OWN") {
        "Use the modeled transfer recipient, or move the access before the ownership transfer."
    } else {
        "Handle the modeled result or operation according to the rule contract."
    }
}

fn lint_inner(args: &LintArgs) -> Result<i32, String> {
    let sources = collect(&args.root)?;
    let facts = frontend_v2::SemanticFacts {
        unbound_entries: args.entry.iter().cloned().collect(),
        benign_unknowns: true,
        ..Default::default()
    };
    let mut program = frontend_v2::lower_project_with_facts(&sources, Path::new(""), &facts)?;
    if args.entry.is_empty() {
        program.roots = program
            .functions
            .iter()
            .filter(|function| !function.id.ends_with("::<module>"))
            .map(|function| function.id.clone())
            .collect();
    } else {
        for name in &args.entry {
            if !program
                .functions
                .iter()
                .any(|function| &function.id == name)
            {
                return Err(format!(
                    "Unknown entry {name}; available: {}",
                    program
                        .functions
                        .iter()
                        .map(|function| function.id.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ));
            }
        }
        program.roots = args.entry.clone();
    }

    let analysis = solver_v2::lint(&program);
    let mut unique = BTreeMap::new();
    for finding in analysis.findings {
        // Possible findings are derived from joined fact sets. The v2 solver
        // explicitly does not claim those sets form an ordered feasible path,
        // so they cannot satisfy lint's direct-evidence requirement.
        if finding.certainty != solver_v2::Certainty::Definite {
            continue;
        }
        if !matches!(
            finding.rule.as_str(),
            "LIFE001" | "OWN001" | "OWN002" | "ERR001"
        ) && !finding.rule.starts_with("BOR")
        {
            continue;
        }
        let mut evidence_chain = Vec::new();
        for step in finding.trace {
            let evidence = LintEvidenceStep {
                path: step.site.path,
                line: step.site.span.line,
            };
            if !evidence_chain.iter().any(|old: &LintEvidenceStep| {
                old.path == evidence.path && old.line == evidence.line
            }) {
                evidence_chain.push(evidence);
            }
        }
        if !evidence_chain
            .iter()
            .any(|old| old.path == finding.site.path && old.line == finding.site.span.line)
        {
            evidence_chain.push(LintEvidenceStep {
                path: finding.site.path.clone(),
                line: finding.site.span.line,
            });
        }
        let result = LintFinding {
            suggested_fix: suggested_fix(&finding.rule).into(),
            rule: finding.rule.clone(),
            location: LintLocation {
                path: finding.site.path.clone(),
                line: finding.site.span.line,
                column: finding.site.span.column,
            },
            reason: finding.message.clone(),
            evidence_chain,
        };
        unique
            .entry((
                finding.rule,
                finding.site.path,
                finding.site.span.line,
                finding.site.span.column,
                finding.message,
            ))
            .or_insert(result);
    }
    let report = LintReport {
        schema_version: 1,
        files: sources.len(),
        findings: unique.into_values().collect(),
    };
    match args.format {
        Format::Json => println!(
            "{}",
            serde_json::to_string_pretty(&report).map_err(|error| error.to_string())?
        ),
        Format::Text => {
            for finding in &report.findings {
                println!(
                    "{}:{}:{}: {}: {}",
                    finding.location.path,
                    finding.location.line,
                    finding.location.column,
                    finding.rule,
                    finding.reason
                );
                let chain = finding
                    .evidence_chain
                    .iter()
                    .map(|step| format!("{}:{}", step.path, step.line))
                    .collect::<Vec<_>>()
                    .join(" -> ");
                println!("  evidence: {chain}");
                println!("  fix: {}", finding.suggested_fix);
            }
            println!(
                "{} files; {} findings; exit {}",
                report.files,
                report.findings.len(),
                usize::from(!report.findings.is_empty())
            );
        }
    }
    Ok(i32::from(!report.findings.is_empty()))
}

pub fn lint(args: LintArgs) -> i32 {
    match lint_inner(&args) {
        Ok(status) => status,
        Err(error) => {
            eprintln!("ERROR: {error}");
            2
        }
    }
}

pub fn analyze(args: AnalyzeArgs) -> i32 {
    match analyze_inner(&args) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ERROR: {e}");
            2
        }
    }
}
pub fn compare(args: CompareArgs) -> i32 {
    let result = (|| -> Result<i32, String> {
        let read = |path: &Path| -> Result<d::Report, String> {
            let s = fs::read_to_string(path).map_err(|e| e.to_string())?;
            serde_json::from_str(&s).map_err(|e| e.to_string())
        };
        let result = d::compare_reports(&read(&args.before)?, &read(&args.after)?);
        emit(
            &args.output,
            &serde_json::to_string_pretty(&result).map_err(|e| e.to_string())?,
        )?;
        Ok(
            if !result.comparable
                || result
                    .issues
                    .iter()
                    .any(|i| i.kind == d::ChangeKind::BecameUnverified)
            {
                3
            } else if result
                .issues
                .iter()
                .any(|i| matches!(i.kind, d::ChangeKind::StillPresent | d::ChangeKind::New))
            {
                1
            } else {
                0
            },
        )
    })();
    match result {
        Ok(s) => s,
        Err(e) => {
            eprintln!("ERROR: {e}");
            2
        }
    }
}
