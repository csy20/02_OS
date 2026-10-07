use agent_core::{
    contain::read_regular_text_within,
    types::{EvidenceItem, Language},
    AgentError, Result,
};
use agent_parser::CodeExtractor;
use sha2::{Digest, Sha256};
use std::{collections::HashMap, path::Path};

const SYMBOL_BASELINE: &str = "symbols-v1:";
const FILE_BASELINE: &str = "file-v1:";

/// A live, bounded read independent of the mutable SQLite catalog.
pub(crate) struct EvidenceSnapshot {
    code: bool,
    file_fingerprint: String,
    symbols: HashMap<String, Vec<String>>,
}

impl EvidenceSnapshot {
    pub(crate) fn load(root: &Path, relative: &str, max_bytes: u64) -> Result<Self> {
        let path = Path::new(relative);
        let content = read_regular_text_within(root, path, max_bytes)?;
        Self::from_content(relative, &content)
    }

    pub(crate) fn from_content(relative: &str, content: &str) -> Result<Self> {
        let path = Path::new(relative);
        let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
        let language = Language::from_extension(extension);
        let mut symbols: HashMap<String, Vec<String>> = HashMap::new();
        if language.extracts_symbols() {
            for symbol in CodeExtractor::extract(relative, content, language)?.symbols {
                symbols
                    .entry(symbol.name)
                    .or_default()
                    .push(symbol.fingerprint.clone());
                symbols
                    .entry(symbol.qualified_name)
                    .or_default()
                    .push(symbol.fingerprint);
            }
        }
        Ok(Self {
            code: language.extracts_symbols(),
            file_fingerprint: digest(content.as_bytes()),
            symbols,
        })
    }

    pub(crate) fn symbols_exist(&self, requested: &[String]) -> bool {
        !self.code || requested.iter().all(|name| self.symbols.contains_key(name))
    }

    /// Versioned prefixes distinguish an aggregate baseline from legacy single-symbol hashes.
    /// Sort fingerprints rather than declaration offsets so unrelated preceding edits stay fresh.
    pub(crate) fn baseline(&self, requested: &[String]) -> Result<String> {
        if !self.code || requested.is_empty() {
            return Ok(format!("{FILE_BASELINE}{}", self.file_fingerprint));
        }
        let mut selected = Vec::new();
        let mut requested: Vec<&String> = requested.iter().collect();
        requested.sort_unstable();
        requested.dedup();
        for name in requested {
            let fingerprints = self.symbols.get(name).ok_or_else(|| {
                AgentError::General(format!("evidence symbol '{name}' is missing from its file"))
            })?;
            for fingerprint in fingerprints {
                selected.push((name.as_str(), fingerprint.as_str()));
            }
        }
        selected.sort_unstable();
        let mut hasher = Sha256::new();
        for (name, fingerprint) in selected {
            hasher.update(name.as_bytes());
            hasher.update([0]);
            hasher.update(fingerprint.as_bytes());
            hasher.update([0]);
        }
        Ok(format!(
            "{SYMBOL_BASELINE}{}",
            hex::encode(hasher.finalize())
        ))
    }

    pub(crate) fn matches(&self, item: &EvidenceItem) -> bool {
        let Some(recorded) = &item.fingerprint else {
            return true;
        };
        if recorded.starts_with(SYMBOL_BASELINE) || recorded.starts_with(FILE_BASELINE) {
            return self.baseline(&item.symbols).as_ref().ok() == Some(recorded);
        }
        // Preserve the old single-symbol fingerprint format.
        !self.code
            || item.symbols.iter().all(|name| {
                self.symbols
                    .get(name)
                    .is_some_and(|found| found.iter().any(|value| value == recorded))
            })
    }
}

fn digest(bytes: &[u8]) -> String {
    hex::encode(Sha256::digest(bytes))
}
