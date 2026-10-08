//! Separate scoped integration tokens. No host password or game account grants
//! access to these read-only feeds. Tokens are never included in Debug output.
use crate::{FeedChannel, HostError, ensure_private_directory};
use rand_core::{OsRng, RngCore};
use serde::{Deserialize, Serialize};
use std::{
    fs,
    io::{Read, Write},
    path::Path,
};
#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChatBotCredential {
    pub name: String,
    pub token: String,
    pub channels: Vec<FeedChannel>,
}
impl std::fmt::Debug for ChatBotCredential {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ChatBotCredential")
            .field("name", &self.name)
            .field("channels", &self.channels)
            .finish_non_exhaustive()
    }
}
impl ChatBotCredential {
    pub fn generate(name: String, channels: Vec<FeedChannel>) -> Result<Self, HostError> {
        let mut bytes = [0u8; 32];
        OsRng
            .try_fill_bytes(&mut bytes)
            .map_err(|_| HostError::Credentials)?;
        let credential = Self {
            name,
            channels,
            token: bytes.iter().map(|b| format!("{b:02x}")).collect(),
        };
        credential.validate()?;
        Ok(credential)
    }
    fn validate(&self) -> Result<(), HostError> {
        if self.name.is_empty()
            || self.name.len() > 64
            || self.name.chars().any(char::is_control)
            || self.token.len() != 64
            || !self.token.bytes().all(|c| c.is_ascii_hexdigit())
            || self.channels.is_empty()
            || self.channels.len() > 3
        {
            return Err(HostError::Credentials);
        }
        for (index, channel) in self.channels.iter().enumerate() {
            if self.channels[..index].contains(channel) {
                return Err(HostError::Credentials);
            }
        }
        Ok(())
    }
    pub(crate) fn accepts(&self, token: &str, channel: FeedChannel) -> bool {
        if token.len() != 64 {
            return false;
        }
        let difference = self
            .token
            .bytes()
            .zip(token.bytes())
            .fold(0u8, |acc, (a, b)| acc | (a ^ b));
        difference == 0 && self.channels.contains(&channel)
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialFile {
    version: u32,
    bots: Vec<ChatBotCredential>,
}
pub fn validate_chat_bots(bots: &[ChatBotCredential]) -> Result<(), HostError> {
    if bots.is_empty() || bots.len() > 32 {
        return Err(HostError::Credentials);
    }
    for (index, bot) in bots.iter().enumerate() {
        bot.validate()?;
        if bots[..index]
            .iter()
            .any(|b| b.name == bot.name || b.token == bot.token)
        {
            return Err(HostError::Credentials);
        }
    }
    Ok(())
}
pub fn provision_chat_bots(path: &Path, bots: &[ChatBotCredential]) -> Result<(), HostError> {
    validate_chat_bots(bots)?;
    let parent = path.parent().ok_or(HostError::Permissions)?;
    ensure_private_directory(parent)?;
    let text = toml::to_string(&CredentialFile {
        version: 1,
        bots: bots.to_vec(),
    })
    .map_err(|_| HostError::Credentials)?;
    let mut file = tempfile::NamedTempFile::new_in(parent)?;
    file.write_all(text.as_bytes())?;
    file.as_file().sync_all()?;
    file.persist_noclobber(path)
        .map_err(|e| HostError::Io(e.error))?;
    Ok(())
}
pub fn load_chat_bots(path: &Path) -> Result<Vec<ChatBotCredential>, HostError> {
    ensure_private_directory(path.parent().ok_or(HostError::Permissions)?)?;
    let metadata = fs::symlink_metadata(path)?;
    if !metadata.is_file() || metadata.len() > 16384 {
        return Err(HostError::Permissions);
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        if metadata.permissions().mode() & 0o077 != 0 {
            return Err(HostError::Permissions);
        }
    }
    let mut text = String::new();
    fs::File::open(path)?
        .take(16385)
        .read_to_string(&mut text)?;
    if text.len() > 16384 {
        return Err(HostError::Credentials);
    }
    let file: CredentialFile = toml::from_str(&text).map_err(|_| HostError::Credentials)?;
    if file.version != 1 {
        return Err(HostError::Credentials);
    }
    validate_chat_bots(&file.bots)?;
    Ok(file.bots)
}
