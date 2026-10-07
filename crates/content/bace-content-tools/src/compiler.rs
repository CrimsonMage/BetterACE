use bace_content::{ContentError, ContentLimits, WeenieTemplate};
use bace_storage_codec::{CodecError, CodecLimits};
use thiserror::Error;

const WEENIE_KIND: u16 = 1;
const WEENIE_SCHEMA: u16 = 1;
const MAX_SOURCE_BYTES: usize = 16 * 1024 * 1024;

#[derive(Debug, Error)]
pub enum ToolError {
    #[error("TOML source exceeds 16 MiB limit")]
    SourceLimit,
    #[error("invalid authoring TOML: {0}")]
    Parse(#[from] toml::de::Error),
    #[error("cannot export authoring TOML: {0}")]
    Export(#[from] toml::ser::Error),
    #[error(transparent)]
    Content(#[from] ContentError),
    #[error(transparent)]
    Codec(#[from] CodecError),
}

pub fn parse(source: &str) -> Result<WeenieTemplate, ToolError> {
    if source.len() > MAX_SOURCE_BYTES {
        return Err(ToolError::SourceLimit);
    }
    let mut template: WeenieTemplate = toml::from_str(source)?;
    template.validate(ContentLimits::default())?;
    template.canonicalize();
    Ok(template)
}

pub fn compile(source: &str) -> Result<Vec<u8>, ToolError> {
    compile_template(&parse(source)?)
}

pub fn compile_template(template: &WeenieTemplate) -> Result<Vec<u8>, ToolError> {
    template.validate(ContentLimits::default())?;
    let mut canonical = template.clone();
    canonical.canonicalize();
    Ok(bace_storage_codec::encode(
        WEENIE_KIND,
        WEENIE_SCHEMA,
        &canonical,
        CodecLimits::default(),
    )?)
}

pub fn decode(bytes: &[u8]) -> Result<WeenieTemplate, ToolError> {
    let mut template: WeenieTemplate =
        bace_storage_codec::decode(bytes, WEENIE_KIND, WEENIE_SCHEMA, CodecLimits::default())?;
    template.validate(ContentLimits::default())?;
    template.canonicalize();
    Ok(template)
}

pub fn export(template: &WeenieTemplate) -> Result<String, ToolError> {
    template.validate(ContentLimits::default())?;
    let mut canonical = template.clone();
    canonical.canonicalize();
    Ok(toml::to_string_pretty(&canonical)?)
}

pub fn export_binary(bytes: &[u8]) -> Result<String, ToolError> {
    export(&decode(bytes)?)
}
