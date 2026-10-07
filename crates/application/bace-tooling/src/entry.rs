use clap::Parser;
use std::process::ExitCode;

pub fn run() -> ExitCode {
    let args = match crate::commands::Arguments::try_parse() {
        Ok(value) => value,
        Err(error) => {
            let code = error.exit_code();
            let _ = error.print();
            return ExitCode::from(code as u8);
        }
    };
    let result = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| -> Box<dyn std::error::Error> { Box::new(e) })
        .and_then(|runtime| runtime.block_on(crate::commands::execute(args)));
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
