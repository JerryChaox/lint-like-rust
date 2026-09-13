mod cli_v2;
use clap::{Parser, Subcommand, ValueEnum};
use lint_like_rust::{
    config::Config,
    engine,
    ir::{CoverageGap, Diagnostic},
    python,
};
use serde::Serialize;
use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
};

const RULES: &[(&str, &str)] = &[
    (
        "OWN001",
        "Use after an explicit or modeled ownership transfer",
    ),
    ("OWN002", "Repeated ownership transfer"),
    (
        "BOR001",
        "New borrow conflicts with a live borrow (borrowing policy)",
    ),
    (
        "BOR002",
        "Access conflicts with a live borrow (borrowing policy)",
    ),
    (
        "BOR003",
        "Move or close conflicts with a live borrow (borrowing policy)",
    ),
    ("LIFE001", "Use after resource close or scope invalidation"),
    ("LIFE002", "Resource or borrow escapes its valid lifetime"),
    ("ERR001", "Discarded result of a modeled must-use call"),
];
#[derive(Parser)]
#[command(
    name = "llr",
    version,
    about = "Rust-inspired ownership and borrowing analysis for Python; never executes scanned code"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    Check(Check),
    /// Experimental project-level v2 analysis (exit 3 means unverified).
    Analyze(cli_v2::AnalyzeArgs),
    /// Compare v2 proof reports; missing diagnostics are not proof of repair.
    Compare(cli_v2::CompareArgs),
    Rules,
}
#[derive(clap::Args)]
struct Check {
    #[arg(required = true)]
    paths: Vec<PathBuf>,
    #[arg(long, value_enum, default_value = "text")]
    format: Format,
    #[arg(long)]
    strict: bool,
    #[arg(long)]
    config: Option<PathBuf>,
    #[arg(long, value_delimiter = ',')]
    select: Vec<String>,
    #[arg(long, value_delimiter = ',')]
    ignore: Vec<String>,
    #[arg(long, value_delimiter = ',')]
    exclude: Vec<String>,
    #[arg(long)]
    output: Option<PathBuf>,
}
#[derive(Clone, Copy, ValueEnum)]
enum Format {
    Text,
    Json,
}
#[derive(Serialize)]
struct FileConfig {
    path: String,
    config_source: Option<String>,
    trusted_contracts: bool,
    effective: Config,
}
#[derive(Serialize)]
struct Suppressed {
    diagnostic: Diagnostic,
    reason: String,
}
#[derive(Serialize)]
struct Report {
    schema_version: u32,
    version: &'static str,
    files: usize,
    diagnostics: Vec<Diagnostic>,
    coverage: Vec<CoverageGap>,
    suppressed: Vec<Suppressed>,
    configurations: Vec<FileConfig>,
    errors: Vec<String>,
}
fn main() {
    let status = match Cli::parse().command {
        Command::Rules => {
            for (code, description) in RULES {
                println!("{code}  {description}");
            }
            0
        }
        Command::Check(args) => run(args),
        Command::Analyze(args) => cli_v2::analyze(args),
        Command::Compare(args) => cli_v2::compare(args),
    };
    std::process::exit(status);
}
fn validate(config: &Config) -> Result<(), String> {
    for code in config.select.iter().chain(&config.ignore) {
        if !RULES.iter().any(|(known, _)| *known == code) {
            return Err(format!("unknown rule: {code}"));
        }
    }
    for pattern in &config.exclude {
        ignore::gitignore::GitignoreBuilder::new(".")
            .add_line(None, pattern)
            .map_err(|e| format!("invalid exclusion {pattern:?}: {e}"))?;
    }
    Ok(())
}
fn read_config(path: &Path) -> Result<Config, String> {
    let source = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let value: toml::Value =
        toml::from_str(&source).map_err(|e| format!("{}: {e}", path.display()))?;
    let value =
        if path.file_name().is_some_and(|s| s == "pyproject.toml") || value.get("tool").is_some() {
            value
                .get("tool")
                .and_then(|v| v.get("llr"))
                .cloned()
                .unwrap_or(toml::Value::Table(Default::default()))
        } else {
            value
        };
    let config: Config = value
        .try_into()
        .map_err(|e| format!("{}: {e}", path.display()))?;
    validate(&config)?;
    Ok(config)
}
fn config_for(path: &Path, args: &Check) -> Result<(Config, Option<PathBuf>), String> {
    let found = if let Some(explicit) = &args.config {
        Some(explicit.clone())
    } else {
        let mut found = None;
        for directory in path.parent().unwrap_or(Path::new(".")).ancestors() {
            let candidate = directory.join("pyproject.toml");
            if candidate.is_file() {
                let source = fs::read_to_string(&candidate)
                    .map_err(|e| format!("{}: {e}", candidate.display()))?;
                let value: toml::Value =
                    toml::from_str(&source).map_err(|e| format!("{}: {e}", candidate.display()))?;
                if value.get("tool").and_then(|tool| tool.get("llr")).is_some() {
                    found = Some(candidate);
                    break;
                }
            }
        }
        found
    };
    let mut config = found
        .as_deref()
        .map(read_config)
        .transpose()?
        .unwrap_or_default();
    config.strict |= args.strict;
    if !args.select.is_empty() {
        config.select = args.select.clone();
    }
    config.ignore.extend(args.ignore.clone());
    config.exclude.extend(args.exclude.clone());
    validate(&config)?;
    Ok((config, found))
}
fn excluded(path: &Path, config: &Config, root: &Path) -> bool {
    let mut builder = ignore::gitignore::GitignoreBuilder::new(root);
    for pattern in &config.exclude {
        let _ = builder.add_line(None, pattern);
    }
    builder
        .build()
        .is_ok_and(|matcher| matcher.matched_path_or_any_parents(path, false).is_ignore())
}
fn inline_ignored(source: &str, diagnostic: &Diagnostic) -> bool {
    // Parse comments so a directive inside a string cannot suppress a finding.
    let mut parser = tree_sitter::Parser::new();
    if parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .is_err()
    {
        return false;
    }
    let Some(tree) = parser.parse(source, None) else {
        return false;
    };
    let row = diagnostic.span.line.saturating_sub(1);
    let Some(line) = source.lines().nth(row) else {
        return false;
    };
    let Some(node) = tree.root_node().named_descendant_for_point_range(
        tree_sitter::Point::new(row, line.len().saturating_sub(1)),
        tree_sitter::Point::new(row, line.len().saturating_sub(1)),
    ) else {
        return false;
    };
    if node.kind() != "comment" {
        return false;
    }
    let Ok(comment) = node.utf8_text(source.as_bytes()) else {
        return false;
    };
    let Some(codes) = comment
        .trim()
        .strip_prefix("# llr: ignore[")
        .and_then(|s| s.strip_suffix(']'))
    else {
        return false;
    };
    codes.split(',').any(|code| code.trim() == diagnostic.rule)
}

fn run(args: Check) -> i32 {
    let mut report = Report {
        schema_version: 1,
        version: env!("CARGO_PKG_VERSION"),
        files: 0,
        diagnostics: vec![],
        coverage: vec![],
        suppressed: vec![],
        configurations: vec![],
        errors: vec![],
    };
    let mut paths = BTreeSet::new();
    // Validate even when all input files will be excluded.
    if let Some(path) = &args.config
        && let Err(e) = read_config(path)
    {
        report.errors.push(e);
    }
    for path in &args.paths {
        if !path.exists() {
            report
                .errors
                .push(format!("{}: input does not exist", path.display()));
            continue;
        }
        if path.is_file() {
            if path.extension().is_some_and(|ext| ext == "py") {
                match fs::canonicalize(path) {
                    Ok(p) => {
                        paths.insert(p);
                    }
                    Err(e) => report.errors.push(format!("{}: {e}", path.display())),
                }
            } else {
                report
                    .errors
                    .push(format!("{}: expected a Python .py file", path.display()));
            }
            continue;
        }
        let mut walk = ignore::WalkBuilder::new(path);
        walk.hidden(false).filter_entry(|entry| {
            !entry.file_type().is_some_and(|t| t.is_dir())
                || !matches!(
                    entry.file_name().to_str(),
                    Some(".git" | ".venv" | "venv" | "node_modules" | "target" | "__pycache__")
                )
        });
        for entry in walk.build() {
            match entry {
                Err(e) => report.errors.push(e.to_string()),
                Ok(entry)
                    if entry.file_type().is_some_and(|t| t.is_file())
                        && entry.path().extension().is_some_and(|e| e == "py") =>
                {
                    match fs::canonicalize(entry.path()) {
                        Ok(p) => {
                            paths.insert(p);
                        }
                        Err(e) => report
                            .errors
                            .push(format!("{}: {e}", entry.path().display())),
                    }
                }
                _ => {}
            }
        }
    }
    let mut strict_failed = false;
    for path in paths {
        let (config, origin) = match config_for(&path, &args) {
            Ok(c) => c,
            Err(e) => {
                report.errors.push(e);
                continue;
            }
        };
        let scan_root = args
            .paths
            .iter()
            .filter(|p| p.is_dir())
            .filter_map(|p| fs::canonicalize(p).ok())
            .find(|p| path.starts_with(p));
        let root = origin
            .as_deref()
            .and_then(Path::parent)
            .or(scan_root.as_deref())
            .unwrap_or_else(|| path.parent().unwrap());
        if excluded(&path, &config, root) {
            continue;
        }
        let source = match fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                report.errors.push(format!("{}: {e}", path.display()));
                continue;
            }
        };
        let name = path.to_string_lossy().into_owned();
        report.files += 1;
        let units = match python::lower_source(&name, &source, &config) {
            Ok(u) => u,
            Err(e) => {
                report.errors.push(format!("{name}: {e}"));
                continue;
            }
        };
        for unit in units {
            let analysis = engine::analyze(&unit);
            strict_failed |= config.strict && !analysis.coverage.is_empty();
            report.coverage.extend(analysis.coverage);
            for diagnostic in analysis.diagnostics {
                let reason =
                    if !config.select.is_empty() && !config.select.contains(&diagnostic.rule) {
                        Some("not selected")
                    } else if config.ignore.contains(&diagnostic.rule) {
                        Some("configuration ignore")
                    } else if inline_ignored(&source, &diagnostic) {
                        Some("inline ignore")
                    } else {
                        None
                    };
                if let Some(reason) = reason {
                    report.suppressed.push(Suppressed {
                        diagnostic,
                        reason: reason.into(),
                    });
                } else {
                    report.diagnostics.push(diagnostic);
                }
            }
        }
        report.configurations.push(FileConfig {
            path: name,
            config_source: origin.map(|p| p.to_string_lossy().into_owned()),
            trusted_contracts: !config.contracts.is_empty(),
            effective: config,
        });
    }
    if report.files == 0 {
        report
            .errors
            .push("no Python files analyzed (check paths, exclusions and gitignore)".into());
    }
    report.diagnostics.sort_by(|a, b| {
        (&a.path, a.span.line, a.span.column, &a.rule).cmp(&(
            &b.path,
            b.span.line,
            b.span.column,
            &b.rule,
        ))
    });
    report.diagnostics.dedup();
    report.coverage.sort_by(|a, b| {
        (&a.path, a.span.line, a.span.column, &a.reason).cmp(&(
            &b.path,
            b.span.line,
            b.span.column,
            &b.reason,
        ))
    });
    report.coverage.dedup();
    let status = if !report.errors.is_empty() {
        2
    } else if !report.diagnostics.is_empty() || strict_failed {
        1
    } else {
        0
    };
    let output = match args.format {
        Format::Json => serde_json::to_string_pretty(&report).expect("report is serializable"),
        Format::Text => {
            let mut lines = Vec::new();
            for d in &report.diagnostics {
                lines.push(format!(
                    "{}:{}:{}: {} {:?}: {}",
                    d.path, d.span.line, d.span.column, d.rule, d.confidence, d.message
                ));
                for note in &d.notes {
                    lines.push(format!(
                        "  {}:{}:{}: note: {}",
                        d.path, note.span.line, note.span.column, note.message
                    ));
                }
            }
            lines.extend(report.coverage.iter().map(|g| {
                format!(
                    "{}:{}:{}: UNKNOWN: {}",
                    g.path, g.span.line, g.span.column, g.reason
                )
            }));
            lines.extend(report.errors.iter().map(|e| format!("ERROR: {e}")));
            lines.push(format!(
                "{} files; {} findings; {} coverage gaps; {} suppressed; {} errors",
                report.files,
                report.diagnostics.len(),
                report.coverage.len(),
                report.suppressed.len(),
                report.errors.len()
            ));
            lines.join("\n")
        }
    };
    if let Some(path) = args.output {
        if let Err(e) = fs::write(&path, format!("{output}\n")) {
            eprintln!("{}: {e}", path.display());
            return 2;
        }
    } else {
        println!("{output}");
    }
    status
}
