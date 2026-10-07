//! Thin executable adapter; implementation belongs to bace_runtime.

fn main() -> std::process::ExitCode {
    bace_runtime::entry::run()
}
