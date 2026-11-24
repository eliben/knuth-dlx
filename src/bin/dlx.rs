//! Read a table in Knuth's input format from stdin and enumerate its solutions.

use std::io;
use std::process::ExitCode;

use dlx::{Table, search};

fn main() -> ExitCode {
    match Table::from_reader(io::stdin().lock()) {
        Ok(mut table) => {
            println!(
                "Read {} columns and {} nonzero entries.",
                table.columns().len() - 1,
                table.nodes().len() - table.columns().len(),
            );
            let mut count = 0u64;
            search(&mut table, |solution| {
                count += 1;
                println!("Solution {count}:");
                for names in solution.rows() {
                    println!("  {}", names.join(" "));
                }
            });
            println!("Solutions: {count}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("dlx: {error}");
            ExitCode::FAILURE
        }
    }
}
