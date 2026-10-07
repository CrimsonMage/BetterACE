//! Thin executable adapter.

mod checks;
mod coverage;
mod entry;
mod roots;
mod unsafe_policy;

fn main() -> std::process::ExitCode {
    entry::run()
}
