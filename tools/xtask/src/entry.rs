use std::process::ExitCode;

pub fn run() -> ExitCode {
    let args: Vec<_> = std::env::args().skip(1).collect();
    if args.as_slice() != ["check"] {
        eprintln!("usage: cargo xtask check");
        return ExitCode::FAILURE;
    }
    match crate::checks::check_workspace()
        .and_then(|report| crate::coverage::check().map(|()| report))
    {
        Ok(report) => {
            println!("{report}");
            ExitCode::SUCCESS
        }
        Err(errors) => {
            for error in errors {
                eprintln!("{error}");
            }
            ExitCode::FAILURE
        }
    }
}
