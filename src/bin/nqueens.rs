//! Map N-queens to a cover table using the encoding in Knuth's paper (page 20).
//!
//! Each table row represents placing one queen on one square. Table columns
//! represent the constraints that this placement uses. Index board rows by i
//! from top to bottom and board columns by j from left to right, starting at 0:
//!
//! - R_i is rank i (a horizontal board row).
//! - F_j is file j (a vertical board column).
//! - A_s is the / diagonal whose squares have the same sum s = i + j.
//! - B_d is the \ diagonal whose squares have the same d = N - 1 - i + j.
//!   The N - 1 offset makes these diagonal indices nonnegative. Each diagonal
//!   family has indices from 0 through 2N - 2.
//!
//! For N = 4, the diagonal labels on each square are:
//!
//! ```text
//!        A diagonals (/)              B diagonals (\)
//!        j=0  1  2  3                 j=0  1  2  3
//!     i                               i
//!     0   A0 A1 A2 A3                 0   B3 B4 B5 B6
//!     1   A1 A2 A3 A4                 1   B2 B3 B4 B5
//!     2   A2 A3 A4 A5                 2   B1 B2 B3 B4
//!     3   A3 A4 A5 A6                 3   B0 B1 B2 B3
//! ```
//!
//! Example: a queen at (i, j) produces the table row `R_i F_j A_(i+j)
//! B_(N-1-i+j)`. (1, 2) on the 4-by-4 board gives `R1 F2 A3 B4`. There are N*N
//! candidate rows, each with four entries (the queen's placement in rank, file,
//! and the two diagonals).
//!
//! Ranks and files are primary columns: each must be covered exactly once. This
//! requires N queens, with one in every board row and every board column.
//! Diagonals are secondary columns: each may be covered at most once. Empty
//! diagonals are allowed, but two queens on the same diagonal are not.
//!
//! Only the primary columns belong to ROOT's list, which search uses to choose
//! a constraint and decide when a solution is complete. Selecting a queen still
//! covers all four of its columns, including its secondary diagonal columns.
//! This blocks other placements sharing any of those constraints. Thus unused
//! diagonals never need to be covered, while diagonal conflicts are prevented.
//!
//! We keep the one-square corner diagonals, matching the paper's example; they
//! could be omitted because they cannot cause a conflict. For N = 4 this gives
//! 8 primary columns, 14 secondary columns, and 16 candidate rows.

use std::env;
use std::process::ExitCode;

use dlx::{ColumnKind, Table};

/// Build a cover table for an N-by-N board using the encoding above.
fn queens_table(n: usize) -> Result<Table, &'static str> {
    if n == 0 {
        return Err("N must be at least 1");
    }
    let too_large = "N is too large to represent as a table";
    let diagonal_count = n
        .checked_mul(2)
        .and_then(|v| v.checked_sub(1))
        .ok_or(too_large)?;
    let column_count = n
        .checked_mul(6)
        .and_then(|v| v.checked_sub(2))
        .ok_or(too_large)?;
    // Check the number of data nodes plus headers before computing indices.
    n.checked_mul(n)
        .and_then(|v| v.checked_mul(4))
        .and_then(|v| v.checked_add(column_count))
        .and_then(|v| v.checked_add(1))
        .ok_or(too_large)?;

    let mut table = Table::new();
    let ranks: Vec<_> = (0..n)
        .map(|i| table.add_column(&format!("R{i}"), ColumnKind::Primary))
        .collect();
    let files: Vec<_> = (0..n)
        .map(|j| table.add_column(&format!("F{j}"), ColumnKind::Primary))
        .collect();
    let diagonals_a: Vec<_> = (0..diagonal_count)
        .map(|d| table.add_column(&format!("A{d}"), ColumnKind::Secondary))
        .collect();
    let diagonals_b: Vec<_> = (0..diagonal_count)
        .map(|d| table.add_column(&format!("B{d}"), ColumnKind::Secondary))
        .collect();

    for i in 0..n {
        for j in 0..n {
            table.add_row(&[
                ranks[i],
                files[j],
                diagonals_a[i + j],
                diagonals_b[n - 1 - i + j],
            ]);
        }
    }
    Ok(table)
}

fn main() -> ExitCode {
    let args: Vec<_> = env::args().skip(1).collect();
    let [size] = args.as_slice() else {
        eprintln!("Usage: nqueens N");
        return ExitCode::FAILURE;
    };
    let n = match size.parse::<usize>() {
        Ok(n) => n,
        Err(_) => {
            eprintln!("nqueens: N must be a positive integer");
            return ExitCode::FAILURE;
        }
    };
    match queens_table(n) {
        Ok(table) => {
            let primary = 2 * n;
            let secondary = table.columns().len() - 1 - primary;
            let entries = table.nodes().len() - table.columns().len();
            println!(
                "Built {n}-queens table: {primary} primary columns, {secondary} secondary columns, {} candidate rows, {entries} nonzero entries.",
                n * n,
            );
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("nqueens: {error}");
            ExitCode::FAILURE
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use dlx::{ColumnId, ROOT};

    /// Return candidate rows in square order (rank first, then file), as column IDs.
    fn queen_rows(table: &Table, n: usize) -> Vec<Vec<ColumnId>> {
        let mut rows = Vec::new();
        for rank in 0..n {
            let head = table.columns()[rank + 1].head;
            let mut row = table.nodes()[head].down;
            for _ in 0..n {
                assert_ne!(row, head);
                let mut columns = Vec::new();
                let mut node = row;
                for _ in 0..4 {
                    columns.push(table.nodes()[node].column);
                    assert_eq!(table.nodes()[table.nodes()[node].left].right, node);
                    assert_eq!(table.nodes()[table.nodes()[node].right].left, node);
                    node = table.nodes()[node].right;
                }
                assert_eq!(node, row);
                rows.push(columns);
                row = table.nodes()[row].down;
            }
            assert_eq!(row, head);
        }
        rows
    }

    #[test]
    fn four_queens_matches_paper() {
        let table = queens_table(4).unwrap();
        let rows = queen_rows(&table, 4);
        let names: Vec<Vec<&str>> = rows
            .iter()
            .map(|row| {
                row.iter()
                    .map(|&c| table.columns()[c].name.as_str())
                    .collect()
            })
            .collect();
        assert_eq!(
            names,
            vec![
                vec!["R0", "F0", "A0", "B3"],
                vec!["R0", "F1", "A1", "B4"],
                vec!["R0", "F2", "A2", "B5"],
                vec!["R0", "F3", "A3", "B6"],
                vec!["R1", "F0", "A1", "B2"],
                vec!["R1", "F1", "A2", "B3"],
                vec!["R1", "F2", "A3", "B4"],
                vec!["R1", "F3", "A4", "B5"],
                vec!["R2", "F0", "A2", "B1"],
                vec!["R2", "F1", "A3", "B2"],
                vec!["R2", "F2", "A4", "B3"],
                vec!["R2", "F3", "A5", "B4"],
                vec!["R3", "F0", "A3", "B0"],
                vec!["R3", "F1", "A4", "B1"],
                vec!["R3", "F2", "A5", "B2"],
                vec!["R3", "F3", "A6", "B3"],
            ],
        );
    }

    #[test]
    fn sizes_and_column_links() {
        for n in [1, 2, 5, 8] {
            let table = queens_table(n).unwrap();
            let columns = table.columns();
            let nodes = table.nodes();
            assert_eq!(columns.len() - 1, 6 * n - 2);
            assert_eq!(nodes.len() - columns.len(), 4 * n * n);
            assert_eq!(queen_rows(&table, n).len(), n * n);

            let mut column = columns[ROOT].next;
            for expected in 1..=2 * n {
                assert_eq!(column, expected);
                assert_eq!(columns[column].len, n);
                assert_eq!(columns[columns[column].prev].next, column);
                column = columns[column].next;
            }
            assert_eq!(column, ROOT);
            assert_eq!(columns[ROOT].prev, 2 * n);

            for (c, column) in columns.iter().enumerate().skip(2 * n + 1) {
                assert_eq!(column.prev, c);
                assert_eq!(column.next, c);
                let d = (c - 2 * n - 1) % (2 * n - 1);
                assert_eq!(column.len, 1 + d.min(2 * n - 2 - d));
            }
            for (c, column) in columns.iter().enumerate() {
                let mut node = nodes[column.head].down;
                for _ in 0..column.len {
                    assert_ne!(node, column.head);
                    assert_eq!(nodes[node].column, c);
                    assert_eq!(nodes[nodes[node].up].down, node);
                    assert_eq!(nodes[nodes[node].down].up, node);
                    node = nodes[node].down;
                }
                assert_eq!(node, column.head);
                assert_eq!(nodes[nodes[column.head].up].down, column.head);
                assert_eq!(nodes[nodes[column.head].down].up, column.head);
            }
        }
    }

    #[test]
    fn shared_constraints_match_queen_attacks() {
        let n = 5;
        let table = queens_table(n).unwrap();
        let rows = queen_rows(&table, n);
        for first in 0..rows.len() {
            for second in first + 1..rows.len() {
                let (i, j) = (first / n, first % n);
                let (k, l) = (second / n, second % n);
                let attack = i == k || j == l || i.abs_diff(k) == j.abs_diff(l);
                let share_column = rows[first].iter().any(|c| rows[second].contains(c));
                assert_eq!(share_column, attack, "squares ({i}, {j}) and ({k}, {l})");
            }
        }
    }

    #[test]
    fn invalid_sizes() {
        assert_eq!(queens_table(0).unwrap_err(), "N must be at least 1");
        assert!(queens_table(usize::MAX).is_err());
    }
}
