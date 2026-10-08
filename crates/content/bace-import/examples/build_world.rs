#[path = "build_world/entry.rs"]
mod entry;
fn main() -> Result<(), Box<dyn std::error::Error>> {
    entry::run()
}
