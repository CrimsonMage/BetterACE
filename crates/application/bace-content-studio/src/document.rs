use sha2::{Digest, Sha256};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};

const HISTORY_BYTES: usize = 32 * 1024 * 1024;

pub(crate) struct Document {
    pub value: toml::Value,
    pub path: Option<PathBuf>,
    pub disk_hash: Option<[u8; 32]>,
    saved: String,
    history: VecDeque<String>,
    redo: Vec<String>,
    uncheckpointed: bool,
}

impl Document {
    pub fn new() -> Result<Self, String> {
        Self::from_text(
            "schema_version=1\nweenie_id=1\nclass_name='new_weenie'\nweenie_type=1\n",
            None,
            None,
        )
    }

    pub fn from_text(
        text: &str,
        path: Option<PathBuf>,
        disk_hash: Option<[u8; 32]>,
    ) -> Result<Self, String> {
        let template = bace_content_tools::parse(text).map_err(|e| e.to_string())?;
        let canonical = bace_content_tools::export(&template).map_err(|e| e.to_string())?;
        let value = toml::from_str(&canonical).map_err(|e: toml::de::Error| e.to_string())?;
        Ok(Self {
            value,
            saved: if path.is_some() {
                canonical.clone()
            } else {
                String::new()
            },
            path,
            disk_hash,
            history: VecDeque::from([canonical]),
            redo: Vec::new(),
            uncheckpointed: false,
        })
    }

    pub fn text(&self) -> Result<String, String> {
        toml::to_string_pretty(&self.value).map_err(|e| e.to_string())
    }

    pub fn validated(&self) -> Result<String, String> {
        let template = bace_content_tools::parse(&self.text()?).map_err(|e| e.to_string())?;
        bace_content_tools::export(&template).map_err(|e| e.to_string())
    }

    pub fn dirty(&self) -> bool {
        self.uncheckpointed || self.history.back().is_none_or(|text| text != &self.saved)
    }

    pub fn checkpoint(&mut self) -> Result<(), String> {
        self.uncheckpointed = true;
        let text = self.text()?;
        if text.len() > 16 * 1024 * 1024 {
            return Err("Editor document exceeds 16 MiB.".into());
        }
        if self.history.back() != Some(&text) {
            self.history.push_back(text);
            self.redo.clear();
            while self.history.len() > 32
                || self.history.iter().map(String::len).sum::<usize>() > HISTORY_BYTES
            {
                self.history.pop_front();
            }
        }
        self.uncheckpointed = false;
        Ok(())
    }

    pub fn undo(&mut self) {
        if self.uncheckpointed {
            if let Some(text) = self.history.back()
                && let Ok(value) = toml::from_str(text)
            {
                self.value = value;
            }
            self.uncheckpointed = false;
            return;
        }
        if self.history.len() > 1
            && let Some(last) = self.history.pop_back()
        {
            if let Some(previous) = self.history.back()
                && let Ok(value) = toml::from_str(previous)
            {
                self.value = value;
            }
            self.redo.push(last);
        }
    }

    pub fn redo(&mut self) {
        if let Some(text) = self.redo.pop() {
            if let Ok(value) = toml::from_str(&text) {
                self.value = value;
            }
            self.history.push_back(text);
        }
    }

    pub fn saved(&mut self, path: PathBuf, text: String) {
        self.uncheckpointed = false;
        self.redo.clear();
        self.disk_hash = Some(Sha256::digest(text.as_bytes()).into());
        self.path = Some(path);
        self.saved = text;
        // Save uses canonical text. Reset current checkpoint to that exact
        // representation while preserving the preceding undo checkpoints.
        if let Ok(value) = toml::from_str(&self.saved) {
            self.value = value;
        }
        if self.history.back() != Some(&self.saved) {
            self.history.push_back(self.saved.clone());
        }
        while self.history.len() > 32
            || self.history.iter().map(String::len).sum::<usize>() > HISTORY_BYTES
        {
            self.history.pop_front();
        }
    }

    pub fn clone_as_new(&mut self) {
        self.path = None;
        self.disk_hash = None;
        self.saved.clear();
    }
}

pub(crate) fn read_bounded(path: &Path) -> Result<String, String> {
    use std::io::Read;
    if !path.is_file() {
        return Err("Select a regular content file.".into());
    }
    let mut text = String::new();
    std::fs::File::open(path)
        .map_err(|e| e.to_string())?
        .take(16 * 1024 * 1024 + 1)
        .read_to_string(&mut text)
        .map_err(|e| e.to_string())?;
    if text.len() > 16 * 1024 * 1024 {
        return Err("Source exceeds 16 MiB.".into());
    }
    Ok(text)
}

pub(crate) fn open(path: &Path) -> Result<Document, String> {
    let text = read_bounded(path)?;
    if path
        .extension()
        .is_some_and(|s| s.eq_ignore_ascii_case("json"))
    {
        let template = bace_import::import_weenie_json(&text).map_err(|e| e.to_string())?;
        Document::from_text(
            &bace_content_tools::export(&template).map_err(|e| e.to_string())?,
            None,
            None,
        )
    } else if path
        .extension()
        .is_some_and(|s| s.eq_ignore_ascii_case("toml"))
    {
        Document::from_text(
            &text,
            Some(path.to_path_buf()),
            Some(Sha256::digest(text.as_bytes()).into()),
        )
    } else {
        Err("Open native TOML or legacy JSON. Convert SQL through the Import tab first.".into())
    }
}

pub(crate) fn write(path: &Path, text: &str, expected: Option<[u8; 32]>) -> Result<(), String> {
    use std::io::Write;
    if let Some(expected) = expected {
        let disk = read_bounded(path)?;
        let current: [u8; 32] = Sha256::digest(disk.as_bytes()).into();
        if current != expected {
            return Err(
                "This file changed on disk. Use Save As or reopen it; your edits are still here."
                    .into(),
            );
        }
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or(Path::new("."));
    let mut file = tempfile::NamedTempFile::new_in(parent).map_err(|e| e.to_string())?;
    file.write_all(text.as_bytes())
        .and_then(|()| file.as_file().sync_all())
        .map_err(|e| e.to_string())?;
    if expected.is_some() {
        file.persist(path).map_err(|e| e.to_string())?;
    } else {
        file.persist_noclobber(path)
            .map_err(|e| format!("Choose a new filename; existing files are preserved: {e}"))?;
    }
    Ok(())
}
