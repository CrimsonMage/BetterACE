#[path = "compile_character_start/entry.rs"]
mod entry;
fn main() -> Result<(), String> {
    entry::run()
}
