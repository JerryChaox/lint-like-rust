//! Snapshot-bound nominal provider evidence. Never grants dispatch or effect proof.
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Bundle {
    pub schema_version: u32,
    pub facts: Vec<Fact>,
    #[serde(default)]
    pub documents: Vec<Document>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Fact {
    pub path: String,
    pub normalized: Normalized,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Normalized {
    pub status: String,
    pub candidates: Vec<Candidate>,
    pub reasons: Vec<String>,
    pub binding: Binding,
    pub dispatch: String,
    pub span: Option<ByteSpan>,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Candidate {
    pub name: String,
    pub uri: String,
    pub range_utf16: Range,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Binding {
    pub snapshot: serde_json::Value,
    pub document_sha256: String,
    pub provider_sha256: String,
    pub stubs_sha256: String,
    pub configuration_sha256: String,
    pub uri: String,
    pub protocol: String,
    pub query_range: Range,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct ByteSpan {
    pub start: usize,
    pub end: usize,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}
#[derive(Debug, Deserialize, Serialize)]
pub struct Position {
    pub line: usize,
    pub character: usize,
}
pub fn sha256(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}
fn offset(source: &str, pos: &Position) -> Option<usize> {
    let mut base = 0;
    for (line, text) in source.split('\n').enumerate() {
        if line == pos.line {
            let text = text.strip_suffix('\r').unwrap_or(text);
            let mut units = 0;
            for (i, c) in text.char_indices() {
                if units == pos.character {
                    return Some(base + i);
                }
                units += c.len_utf16();
                if units > pos.character {
                    return None;
                }
            }
            return (units == pos.character).then_some(base + text.len());
        }
        base += text.len() + 1;
    }
    None
}
impl Bundle {
    pub fn validate(&self, sources: &[(String, String)]) -> Result<(), String> {
        if self.schema_version != 1 {
            return Err("Unsupported type-evidence schema".into());
        }
        let sources: BTreeMap<_, _> = sources
            .iter()
            .map(|(p, s)| (p.as_str(), s.as_str()))
            .collect();
        for fact in &self.facts {
            let source = sources
                .get(fact.path.as_str())
                .ok_or("Type evidence references an unscanned source")?;
            let n = &fact.normalized;
            let b = &n.binding;
            if n.dispatch != "nominal_candidates_only"
                || !matches!(n.status.as_str(), "unknown" | "candidates")
            {
                return Err("Type evidence must remain nominal".into());
            }
            if b.protocol != "0.4.1"
                || b.document_sha256 != sha256(source.as_bytes())
                || b.snapshot.is_null()
                || b.snapshot == serde_json::json!("")
                || b.uri.is_empty()
                || [&b.provider_sha256, &b.stubs_sha256, &b.configuration_sha256]
                    .iter()
                    .any(|s| s.is_empty())
            {
                return Err("Stale or incomplete type-evidence binding".into());
            }
            let start =
                offset(source, &b.query_range.start).ok_or("Invalid type-evidence range")?;
            let end = offset(source, &b.query_range.end).ok_or("Invalid type-evidence range")?;
            if start > end {
                return Err("Reversed type-evidence range".into());
            }
            if let Some(span) = &n.span {
                if span.start != start || span.end != end {
                    return Err("Type-evidence byte/UTF16 span mismatch".into());
                }
            } else if n.status == "candidates" {
                return Err("Candidate evidence lacks a span".into());
            }
            if n.candidates.iter().any(|c| {
                c.name.is_empty()
                    || c.uri.is_empty()
                    || (c.range_utf16.start.line, c.range_utf16.start.character)
                        > (c.range_utf16.end.line, c.range_utf16.end.character)
            }) {
                return Err("Invalid candidate declaration".into());
            }
            if n.status == "candidates" && (n.candidates.is_empty() || !n.reasons.is_empty()) {
                return Err("Incomplete candidate evidence".into());
            }
        }
        Ok(())
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct Document {
    pub uri: String,
    pub path: String,
    pub document_sha256: String,
}
#[derive(Debug, Serialize)]
pub struct DeclarationMatch {
    pub fact_index: usize,
    pub candidate_index: usize,
    pub symbol: Option<String>,
    pub reason: String,
    pub dispatch: &'static str,
}
impl Bundle {
    pub fn map_declarations(
        &self,
        sources: &[(String, String)],
    ) -> Result<Vec<DeclarationMatch>, String> {
        self.validate(sources)?;
        let index = crate::frontend_v2::declaration_index(sources, std::path::Path::new(""))?;
        let mut documents = BTreeMap::new();
        for doc in &self.documents {
            let source = sources
                .iter()
                .find(|(p, _)| p == &doc.path)
                .ok_or("Declaration document was not scanned")?;
            if doc.uri.is_empty()
                || sha256(source.1.as_bytes()) != doc.document_sha256
                || documents
                    .insert(doc.uri.as_str(), (doc.path.as_str(), source.1.as_str()))
                    .is_some()
            {
                return Err("Stale or ambiguous declaration document binding".into());
            }
        }
        let mut out = Vec::new();
        for (fi, fact) in self.facts.iter().enumerate() {
            for (ci, candidate) in fact.normalized.candidates.iter().enumerate() {
                let mut symbol = None;
                let reason = if let Some((path, source)) = documents.get(candidate.uri.as_str()) {
                    let span = offset(source, &candidate.range_utf16.start)
                        .zip(offset(source, &candidate.range_utf16.end));
                    let matches: Vec<_> = index
                        .iter()
                        .filter(|d| {
                            d.path == *path
                                && d.name == candidate.name
                                && span.is_some_and(|(start, end)| {
                                    (d.start == start && d.end == end)
                                        || (d.name_start == start && d.name_end == end)
                                })
                        })
                        .collect();
                    if matches.len() == 1
                        && index
                            .iter()
                            .filter(|d| d.symbol == matches[0].symbol)
                            .count()
                            == 1
                    {
                        symbol = Some(matches[0].symbol.clone());
                        "scanned_definition_location_match_only"
                    } else {
                        "declaration_not_unique_or_not_admitted"
                    }
                } else {
                    "declaration_document_not_bound"
                };
                out.push(DeclarationMatch {
                    fact_index: fi,
                    candidate_index: ci,
                    symbol,
                    reason: reason.into(),
                    dispatch: "nominal_candidates_only",
                });
            }
        }
        Ok(out)
    }
}
