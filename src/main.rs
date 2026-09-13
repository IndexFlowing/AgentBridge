mod cli;

use std::process::ExitCode;

fn main() -> ExitCode {
    if let Err(err) = cli::run() {
        eprintln!("error: {err}");
        for cause in err.chain().skip(1) {
            eprintln!("  caused by: {cause}");
        }
        return ExitCode::FAILURE;
    }
    ExitCode::SUCCESS
}
