use std::io::Write;
pub fn run() -> Result<(), String> {
    let args: Vec<_> = std::env::args_os().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: compile_character_start SOURCE.toml NEW_OUTPUT.bace".into());
    }
    let input = std::path::Path::new(&args[0]);
    let output = std::path::Path::new(&args[1]);
    if output.extension().is_none_or(|v| v != "bace") {
        return Err("runtime content requires .bace extension".into());
    }
    if std::fs::metadata(input).map_err(|e| e.to_string())?.len() > 1024 * 1024 {
        return Err("character-start authoring limit".into());
    }
    let source = std::fs::read_to_string(input).map_err(|e| e.to_string())?;
    let profile = bace_content_tools::parse_character_start(&source)?;
    let bytes = bace_content_tools::compile_character_start(&profile)?;
    let mut file = std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(output)
        .map_err(|e| e.to_string())?;
    file.write_all(&bytes)
        .and_then(|()| file.sync_all())
        .map_err(|e| e.to_string())
}
