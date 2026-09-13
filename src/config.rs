use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Config {
    pub exclude: Vec<String>,
    pub select: Vec<String>,
    pub ignore: Vec<String>,
    pub strict: bool,
    pub contracts: BTreeMap<String, Contract>,
}

/// Parameter positions are zero-based, excluding method receiver. Contracts are trusted inputs.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub struct Contract {
    pub must_use: bool,
    pub consumes: Vec<usize>,
    pub readonly: Vec<usize>,
    pub mutable: Vec<usize>,
    pub closes: Vec<usize>,
    pub returns_alias: Option<usize>,
    pub returns_borrow: Option<usize>,
    pub returns_mut_borrow: Option<usize>,
    pub returns_resource: bool,
    pub pure: bool,
}
