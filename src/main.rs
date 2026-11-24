use std::io;
use std::process::ExitCode;

use dlx::Table;

fn main() -> ExitCode {
    match Table::from_reader(io::stdin().lock()) {
        Ok(table) => {
            println!(
                "Read {} columns and {} nonzero entries.",
                table.columns().len() - 1,
                table.nodes().len() - table.columns().len(),
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("dlx: {error}");
            ExitCode::FAILURE
        }
    }
}
