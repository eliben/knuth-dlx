//! Exact cover search using dancing links.

use std::collections::HashMap;
use std::io::{self, BufRead};

pub type NodeId = usize;
pub type ColumnId = usize;

/// Set to true and recompile to trace search decisions and backtracking on
/// stderr.
const VERBOSE: bool = false;

// Log a message to stderr with the recursion depth and two spaces of
// indentation per level, only when VERBOSE is true. Using a macro keeps the
// argument expressions inside that check: when logging is disabled, expressions
// such as solution.rows() are not evaluated, avoiding their string
// allocations.
macro_rules! search_log {
    ($depth:expr, $($args:tt)*) => {
        if VERBOSE {
            eprintln!(
                "{:indent$}[{}] {}",
                "",
                $depth,
                format_args!($($args)*),
                indent = 2 * $depth,
            );
        }
    };
}

/// A sparse table stored as circular, doubly linked row and column lists.
///
/// Column IDs index `columns`; node IDs index `nodes`. These are separate index
/// spaces, connected by `Column::head` (column to header node) and
/// `Node::column` (node to column).
///
/// Primary columns must be covered exactly once. Secondary columns enforce
/// at-most-once constraints and are not linked into ROOT's list; their `prev`
/// and `next` links point to themselves.
///
/// `columns[ROOT]` is a sentinel, not an input column. Its `next` and `prev`
/// identify the first and last active primary columns. To visit them, start at
/// `columns[ROOT].next` and follow each column's `next` until reaching `ROOT`
/// again. Follow `prev` to traverse in reverse. An empty column list has both
/// root links pointing to `ROOT`. The root's name is empty and its header node
/// is unused.
///
/// Each real column has a dedicated header node at `head = columns[c].head`.
/// This is a sentinel, not a nonzero table entry. To visit the column's
/// entries, start at `nodes[head].down` and follow `down` until reaching `head`
/// again; use `up` for the reverse direction. An empty column's header links to
/// itself. `columns[c].len` counts entries only, excluding the header. Header
/// nodes' horizontal links are unused and point to themselves.
///
/// Rows have no headers. Starting at any data node, visit that node and follow
/// `right` until returning to the starting node, or follow `left` in reverse.
/// For each node `n`, `columns[nodes[n].column].name` gives its column name. A
/// one-entry row has both horizontal links pointing to its only node.
///
/// Vector positions serve as stable identities: entries must not be removed or
/// reordered, since other entries store their indices. Covering a column
/// unlinks entries while preserving their own links for later restoration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Table {
    nodes: Vec<Node>,
    columns: Vec<Column>,
}

/// Whether a column must be covered or only prevents conflicting row choices.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ColumnKind {
    /// Must be covered exactly once.
    Primary,
    /// May be covered at most once.
    Secondary,
}

/// The sentinel at the head of the circular list of primary columns.
pub const ROOT: ColumnId = 0;

/// A nonzero table entry, or a column's header node.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Node {
    pub left: NodeId,
    pub right: NodeId,
    pub up: NodeId,
    pub down: NodeId,
    pub column: ColumnId,
}

/// A column record.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Column {
    /// The column's header/sentinel in the node vector.
    pub head: NodeId,
    /// Number of data nodes in the column, excluding its header.
    pub len: usize,
    pub name: String,

    /// For primary columns: neighbors in the column list.
    /// For secondary columns: link to themselves.
    pub prev: ColumnId,
    pub next: ColumnId,
}

impl Table {
    /// Create an empty table containing only the root sentinel.
    /// Add columns and rows before starting a search.
    pub fn new() -> Self {
        // Create the root sentinel and its unused, self-linked header node.
        let head = 0;
        Self {
            nodes: vec![Node {
                left: head,
                right: head,
                up: head,
                down: head,
                column: ROOT,
            }],
            columns: vec![Column {
                head,
                len: 0,
                name: String::new(),
                prev: ROOT,
                next: ROOT,
            }],
        }
    }

    /// Read the input format used by `dance.w`.
    ///
    /// The first line lists primary column names, optionally followed by `|`
    /// and secondary column names. At least one primary column is required; the
    /// secondary section may be empty. Without `|`, all columns are primary.
    ///
    /// Each subsequent line lists the columns containing a 1 in one row. Names
    /// are whitespace-delimited.
    ///
    /// Malformed input returns an [`io::ErrorKind::InvalidData`] error with a
    /// line number. Reader errors are propagated unchanged.
    pub fn from_reader(reader: impl BufRead) -> io::Result<Self> {
        let mut lines = reader.lines();
        let header = lines
            .next()
            .ok_or_else(|| input_error(1, "missing column names"))??;

        let mut table = Self::new();
        let mut column_ids = HashMap::new();
        let mut kind = ColumnKind::Primary;
        for name in header.split_whitespace() {
            if name == "|" {
                if kind == ColumnKind::Secondary {
                    return Err(input_error(1, "repeated '|' separator"));
                }
                if column_ids.is_empty() {
                    return Err(input_error(1, "no primary columns"));
                }
                kind = ColumnKind::Secondary;
                continue;
            }
            if column_ids.contains_key(name) {
                return Err(input_error(1, format!("duplicate column name {name:?}")));
            }
            let column = table.add_column(name, kind);
            column_ids.insert(name, column);
        }
        if column_ids.is_empty() {
            return Err(input_error(1, "no columns"));
        }

        let mut row = Vec::new();
        for (index, line) in lines.enumerate() {
            let line = line?;
            let line_number = index + 2;
            row.clear();
            for name in line.split_whitespace() {
                let &column = column_ids.get(name).ok_or_else(|| {
                    input_error(line_number, format!("unknown column name {name:?}"))
                })?;
                if row.contains(&column) {
                    return Err(input_error(
                        line_number,
                        format!("repeated column {name:?} in row"),
                    ));
                }
                row.push(column);
            }
            if row.is_empty() {
                return Err(input_error(line_number, "empty row"));
            }
            table.add_row(&row);
        }
        Ok(table)
    }

    /// All nodes, including column headers and the root's unused header.
    pub fn nodes(&self) -> &[Node] {
        &self.nodes
    }

    /// All column records, with the root sentinel at [`ROOT`].
    pub fn columns(&self) -> &[Column] {
        &self.columns
    }

    /// Cover a column, unlinking it from the active list if it is primary.
    ///
    /// For each row in that column, unlink its other nodes from their vertical
    /// lists and decrement their columns' lengths. Row links and the covered
    /// column's vertical list stay intact.
    ///
    /// The column must not already be covered and must not be ROOT. Removed
    /// entries retain their own links so that [`Table::uncover`] can restore
    /// them.
    pub fn cover(&mut self, column: ColumnId) {
        assert_ne!(column, ROOT, "cannot cover the root sentinel");

        // Unlink the column from the active list, preserving its own links.
        // For a secondary column, its self-links make this a no-op.
        let prev = self.columns[column].prev;
        let next = self.columns[column].next;
        self.columns[prev].next = next;
        self.columns[next].prev = prev;

        // Visit the column's rows from top to bottom.
        let head = self.columns[column].head;
        let mut row = self.nodes[head].down;
        while row != head {
            // Walk the other nodes in this row, skipping the covered column.
            let mut node = self.nodes[row].right;
            while node != row {
                // Unlink this node vertically, retaining its links for uncover.
                let up = self.nodes[node].up;
                let down = self.nodes[node].down;
                self.nodes[up].down = down;
                self.nodes[down].up = up;
                // Its column now has one fewer available entry.
                self.columns[self.nodes[node].column].len -= 1;
                node = self.nodes[node].right;
            }
            row = self.nodes[row].down;
        }
    }

    /// Undo [`Table::cover`], restoring rows from bottom to top and updating
    /// column lengths, then restoring a primary column to the active list.
    ///
    /// Rows must be restored in reverse removal order. Within a row, either
    /// direction works; we traverse left to mirror cover's traversal.
    ///
    /// The column must be the most recently covered column not yet uncovered.
    /// Multiple covers must be undone in reverse order.
    pub fn uncover(&mut self, column: ColumnId) {
        assert_ne!(column, ROOT, "cannot uncover the root sentinel");

        // Visit the column's rows from bottom to top, reversing cover's row
        // order.
        let head = self.columns[column].head;
        let mut row = self.nodes[head].up;
        while row != head {
            // Walk the other nodes leftward to mirror cover's traversal.
            let mut node = self.nodes[row].left;
            while node != row {
                // Restore this node between its remembered vertical neighbors.
                let up = self.nodes[node].up;
                let down = self.nodes[node].down;
                self.nodes[up].down = node;
                self.nodes[down].up = node;
                // Its column now has one more available entry.
                self.columns[self.nodes[node].column].len += 1;
                node = self.nodes[node].left;
            }
            row = self.nodes[row].up;
        }

        // Restore the column between its remembered neighbors in the active
        // list. A secondary column's self-links make this a no-op.
        let prev = self.columns[column].prev;
        let next = self.columns[column].next;
        self.columns[prev].next = column;
        self.columns[next].prev = column;
    }

    /// Create an empty column and its header during table construction.
    /// Primary columns are appended to ROOT's list; secondary columns have
    /// self-links and are excluded from that list.
    /// Return the new column's ID.
    /// Panics if the name is empty or already used by another column.
    pub fn add_column(&mut self, name: &str, kind: ColumnKind) -> ColumnId {
        assert!(!name.is_empty(), "column name must not be empty");
        assert!(
            self.columns.iter().all(|column| column.name != name),
            "duplicate column name: {name}",
        );
        let column = self.columns.len();
        let head = self.nodes.len();
        let (prev, next) = match kind {
            ColumnKind::Primary => (self.columns[ROOT].prev, ROOT),
            ColumnKind::Secondary => (column, column),
        };
        self.nodes.push(Node {
            left: head,
            right: head,
            up: head,
            down: head,
            column,
        });
        self.columns.push(Column {
            head,
            len: 0,
            name: name.to_owned(),
            prev,
            next,
        });
        if kind == ColumnKind::Primary {
            self.columns[prev].next = column;
            self.columns[ROOT].prev = column;
        }
        column
    }

    /// Create a row with one node per supplied column, linked in the supplied
    /// order. The last node links back to the first; rows have no header.
    /// Append each node to its column's vertical list and increment the
    /// column's length. Use this during table construction, before any covers.
    /// Panics if the slice is empty or contains duplicate, unknown, or ROOT
    /// IDs.
    pub fn add_row(&mut self, columns: &[ColumnId]) {
        assert!(!columns.is_empty(), "row must not be empty");
        for (index, &column) in columns.iter().enumerate() {
            assert!(
                column != ROOT && column < self.columns.len(),
                "invalid column ID"
            );
            assert!(
                !columns[..index].contains(&column),
                "repeated column in row"
            );
        }
        let first = self.nodes.len();
        let last = first + columns.len() - 1;
        for &column in columns {
            let node = self.nodes.len();
            let head = self.columns[column].head;
            let up = self.nodes[head].up;
            self.nodes.push(Node {
                left: if node == first { last } else { node - 1 },
                right: if node == last { first } else { node + 1 },
                up,
                down: head,
                column,
            });
            // Update the previous last node and the header to include the new
            // node.
            self.nodes[up].down = node;
            self.nodes[head].up = node;
            self.columns[column].len += 1;
        }
    }
}

impl Default for Table {
    fn default() -> Self {
        Self::new()
    }
}

/// Return the active primary column with the fewest available entries. Ties
/// select the first column encountered in `next` order from ROOT.
///
/// Return `None` only when no active columns remain. An active column with
/// no entries is returned as `Some(column)`; it cannot be covered by a row.
pub fn choose_smallest_column(table: &Table) -> Option<ColumnId> {
    let columns = table.columns();
    let mut best = columns[ROOT].next;
    if best == ROOT {
        return None;
    }

    let mut column = columns[best].next;
    while column != ROOT {
        if columns[column].len < columns[best].len {
            best = column;
        }
        column = columns[column].next;
    }
    Some(best)
}

/// A borrowed view of one solution reported by [`search`].
///
/// The caller owns the table, and search owns the temporary list of selected
/// row-node indices. `Solution` borrows both only for the duration of the
/// callback. [`Solution::rows`] returns owned copies that can be kept
/// after the callback returns.
#[derive(Debug)]
pub struct Solution<'a> {
    table: &'a Table,
    rows: &'a [NodeId],
}

impl Solution<'_> {
    /// The table containing this solution's row nodes.
    pub fn table(&self) -> &Table {
        self.table
    }

    /// One data-node index per selected row, in search order.
    /// These are node indices, not input row numbers.
    pub fn row_nodes(&self) -> &[NodeId] {
        self.rows
    }

    /// Return the selected rows, each represented by its column names.
    /// Rows are in search order. Each inner vector follows the row's `right`
    /// links starting at its selected node, without sorting.
    pub fn rows(&self) -> Vec<Vec<String>> {
        let mut rows = Vec::new();
        for &row in self.rows {
            let mut names = Vec::new();
            let mut node = row;
            loop {
                names.push(
                    self.table.columns()[self.table.nodes()[node].column]
                        .name
                        .clone(),
                );
                node = self.table.nodes()[node].right;
                if node == row {
                    break;
                }
            }
            rows.push(names);
        }
        rows
    }
}

/// Search for rows covering the active primary columns exactly once, using
/// secondary columns to prevent conflicts, with dancing links.
/// As in `dance.w`, rows containing only secondary columns are never selected.
///
/// `on_solution` is called once per solution. The [`Solution`] value is passed
/// by value, but contains references to the caller's table and the row-node
/// indices owned by search. It cannot be kept after the callback returns:
/// search resumes modifying both the table and its list of selected rows.
/// Use [`Solution::rows`] to keep an owned copy of the names grouped
/// by row.
///
/// During the callback, all primary columns are covered and row links remain
/// intact.
/// Identical input rows are treated as distinct choices.
///
/// The table is restored to its initial state when search returns normally.
/// Copied row-node indices remain valid in that restored table.
pub fn search(table: &mut Table, mut on_solution: impl FnMut(Solution<'_>)) {
    search_log!(0, "search started");
    let mut selected_rows = Vec::new();
    search_recursive(table, &mut selected_rows, &mut on_solution);
    search_log!(0, "search finished");
}

fn search_recursive(
    table: &mut Table,
    selected_rows: &mut Vec<NodeId>,
    on_solution: &mut impl FnMut(Solution<'_>),
) {
    let depth = selected_rows.len();

    // Branch on the column with the fewest candidates; if none remain, the
    // solution is complete.
    let Some(column) = choose_smallest_column(table) else {
        let solution = Solution {
            table,
            rows: selected_rows,
        };
        search_log!(depth, "solution: {:?}", solution.rows());
        on_solution(solution);
        return;
    };
    // We cannot complete this partial solution if no row can cover this column.
    if table.columns()[column].len == 0 {
        search_log!(
            depth,
            "dead end: column {} has no available rows",
            table.columns()[column].name,
        );
        return;
    }

    search_log!(
        depth,
        "choose and cover column {} (rows: {})",
        table.columns()[column].name,
        table.columns()[column].len,
    );
    // Every solution must choose exactly one of this column's active rows.
    // Cover the column: remove it from the active list and hide its rows from
    // other columns. Its own list stays intact so we can try each candidate
    // below.
    table.cover(column);
    let head = table.columns()[column].head;
    let mut row = table.nodes()[head].down;
    while row != head {
        // Try this row as the one that covers the chosen column.
        search_log!(
            depth,
            "try row [{}] (node {row})",
            Solution {
                table,
                rows: &[row]
            }
            .rows()[0]
                .join(" "),
        );
        selected_rows.push(row);

        // This row also satisfies its other columns. Cover them to remove
        // them from the remaining problem and block rows that overlap them.
        // Walk right through the selected row's entries.
        let mut node = table.nodes()[row].right;
        while node != row {
            search_log!(
                depth,
                "cover column {}",
                table.columns()[table.nodes()[node].column].name
            );
            table.cover(table.nodes()[node].column);
            node = table.nodes()[node].right;
        }
        // Find all ways to cover the remaining columns with additional rows.
        search_recursive(table, selected_rows, on_solution);

        // Backtrack: undo this row's covers in reverse order, then remove it
        // from the partial solution. The next candidate starts from the same
        // state.
        search_log!(depth, "backtrack from row at node {row}");
        let mut node = table.nodes()[row].left;
        while node != row {
            search_log!(
                depth,
                "uncover column {}",
                table.columns()[table.nodes()[node].column].name
            );
            table.uncover(table.nodes()[node].column);
            node = table.nodes()[node].left;
        }
        selected_rows.pop();
        row = table.nodes()[row].down;
    }
    // All candidate rows have been tried; restore the caller's table state.
    search_log!(
        depth,
        "uncover column {}; finished trying its rows",
        table.columns()[column].name,
    );
    table.uncover(column);
}

fn input_error(line: usize, message: impl std::fmt::Display) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("line {line}: {message}"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAPER_TABLE: &str = "A B C D E F G\nC E F\nA D G\nB C F\nA D\nB G\nD E G\n";

    fn column_id(table: &Table, name: &str) -> ColumnId {
        table.columns.iter().position(|c| c.name == name).unwrap()
    }

    /// Check the remaining active primary column list in both directions.
    /// Covered primary columns and secondary columns are not visited.
    /// `expected_columns` is the complete sequence of active column names in
    /// `next` order from ROOT, excluding ROOT itself. An empty slice expects
    /// the list to contain only ROOT.
    fn assert_active_columns(table: &Table, expected_columns: &[&str]) {
        let columns = table.columns();
        let mut column = columns[ROOT].next;
        for &name in expected_columns {
            assert_ne!(column, ROOT);
            assert_eq!(columns[column].name, name);
            assert_eq!(columns[columns[column].prev].next, column);
            column = columns[column].next;
        }
        assert_eq!(column, ROOT);
        column = columns[ROOT].prev;
        for &name in expected_columns.iter().rev() {
            assert_ne!(column, ROOT);
            assert_eq!(columns[column].name, name);
            assert_eq!(columns[columns[column].next].prev, column);
            column = columns[column].prev;
        }
        assert_eq!(column, ROOT);
    }

    /// Check the rows whose nodes remain linked in the column identified by
    /// `name`, whether that column is active or covered.
    ///
    /// Rows whose nodes have been unlinked from this column are
    /// excluded, even though those nodes still exist in the node vector.
    /// `expected_rows` lists all rows found by following 'down' from the header
    /// of the given column.
    /// Each inner slice gives the full row's column names in `right` order,
    /// starting with `name`, including entries detached from other columns.
    /// For example, row A D G is written as ["D", "G", "A"] when checking D.
    /// An empty outer slice expects the named column to have no entries.
    /// Detached nodes need not have reciprocal vertical links while covered.
    fn assert_column_rows(table: &Table, name: &str, expected_rows: &[&[&str]]) {
        let column = column_id(table, name);
        let head = table.columns[column].head;
        let nodes = table.nodes();
        assert_eq!(table.columns[column].len, expected_rows.len());
        assert_eq!(nodes[nodes[head].up].down, head);
        assert_eq!(nodes[nodes[head].down].up, head);

        let mut row = nodes[head].down;
        for &expected in expected_rows {
            assert_ne!(row, head);
            assert_eq!(nodes[row].column, column);
            assert_eq!(nodes[nodes[row].up].down, row);
            assert_eq!(nodes[nodes[row].down].up, row);
            let mut node = row;
            for &name in expected {
                assert_eq!(table.columns[nodes[node].column].name, name);
                assert_eq!(nodes[nodes[node].left].right, node);
                assert_eq!(nodes[nodes[node].right].left, node);
                node = nodes[node].right;
            }
            assert_eq!(node, row);
            row = nodes[row].down;
        }
        assert_eq!(row, head);
    }

    /// Verify all row and column links, headers, and lengths in a fully linked
    /// table containing only primary columns.
    ///
    /// `expected_columns` lists all table column names in `next` order from
    /// ROOT, excluding ROOT itself.
    /// `expected_rows` lists all table rows in input order. Each inner slice
    /// gives one row's column names in `right` order. For each column, the
    /// helper selects the rows containing it and rotates each row to start with
    /// that column's name before comparing against the column's `down`
    /// traversal.
    ///
    /// Every data node must be linked in its column, so use this after table
    /// construction or after all covers have been undone.
    fn assert_links(table: &Table, expected_columns: &[&str], expected_rows: &[&[&str]]) {
        let nodes = table.nodes();
        let columns = table.columns();
        assert_eq!(columns.len(), expected_columns.len() + 1);
        assert_eq!(columns[ROOT].name, "");
        assert_eq!(columns[ROOT].len, 0);
        assert_active_columns(table, expected_columns);

        let mut vertical_nodes = vec![false; nodes.len()];
        for (c, column) in columns.iter().enumerate() {
            let head = column.head;
            assert_eq!(nodes[head].column, c);
            assert_eq!(nodes[head].left, head);
            assert_eq!(nodes[head].right, head);
            assert_eq!(nodes[nodes[head].up].down, head);
            assert_eq!(nodes[nodes[head].down].up, head);
            let mut node = nodes[head].down;
            let mut actual_rows = Vec::new();
            while node != head {
                assert!(!vertical_nodes[node], "node appears twice in column lists");
                vertical_nodes[node] = true;
                assert_eq!(nodes[node].column, c);
                assert_eq!(nodes[nodes[node].up].down, node);
                assert_eq!(nodes[nodes[node].down].up, node);

                // Reconstruct the row starting with this node's column.
                let mut row = vec![column.name.as_str()];
                let mut next = nodes[node].right;
                while next != node {
                    assert!(row.len() < columns.len());
                    assert_eq!(nodes[nodes[next].left].right, next);
                    assert_eq!(nodes[nodes[next].right].left, next);
                    row.push(columns[nodes[next].column].name.as_str());
                    next = nodes[next].right;
                }
                assert_eq!(nodes[nodes[node].left].right, node);
                assert_eq!(nodes[nodes[node].right].left, node);
                actual_rows.push(row);
                node = nodes[node].down;
            }
            assert_eq!(actual_rows.len(), column.len);

            let expected: Vec<Vec<&str>> = expected_rows
                .iter()
                .filter_map(|row| {
                    let start = row.iter().position(|&name| name == column.name)?;
                    Some(row[start..].iter().chain(&row[..start]).copied().collect())
                })
                .collect();
            assert_eq!(actual_rows, expected);
        }
        for column in columns {
            assert!(!vertical_nodes[column.head]);
            vertical_nodes[column.head] = true;
        }
        assert!(vertical_nodes.into_iter().all(|seen| seen));
    }

    #[test]
    fn paper_table() {
        let table = Table::from_reader(PAPER_TABLE.as_bytes()).unwrap();
        assert_links(
            &table,
            &["A", "B", "C", "D", "E", "F", "G"],
            &[
                &["C", "E", "F"],
                &["A", "D", "G"],
                &["B", "C", "F"],
                &["A", "D"],
                &["B", "G"],
                &["D", "E", "G"],
            ],
        );
    }

    #[test]
    fn primary_and_secondary_columns_from_input() {
        for input in [
            "A B | X Y\nA X\nY B X\n",
            " A\tB \t|\t X Y\r\nA\tX\r\nY B X",
        ] {
            let table = Table::from_reader(input.as_bytes()).unwrap();
            assert_eq!(
                table
                    .columns
                    .iter()
                    .map(|c| c.name.as_str())
                    .collect::<Vec<_>>(),
                vec!["", "A", "B", "X", "Y"],
            );
            assert_active_columns(&table, &["A", "B"]);
            for name in ["X", "Y"] {
                let c = column_id(&table, name);
                assert_eq!(table.columns[c].prev, c);
                assert_eq!(table.columns[c].next, c);
            }
            assert_column_rows(&table, "A", &[&["A", "X"]]);
            assert_column_rows(&table, "B", &[&["B", "X", "Y"]]);
            assert_column_rows(&table, "X", &[&["X", "A"], &["X", "Y", "B"]]);
            assert_column_rows(&table, "Y", &[&["Y", "B", "X"]]);
        }
    }

    #[test]
    fn empty_secondary_section() {
        let table = Table::from_reader(&b"A B |\nA\nB\n"[..]).unwrap();
        assert_links(&table, &["A", "B"], &[&["A"], &["B"]]);
    }

    #[test]
    fn secondary_columns_only_block_conflicting_rows() {
        let mut table = Table::new();
        let a = table.add_column("A", ColumnKind::Primary);
        let b = table.add_column("B", ColumnKind::Primary);
        let x = table.add_column("X", ColumnKind::Secondary);
        let y = table.add_column("Y", ColumnKind::Secondary);
        table.add_row(&[a, x]);
        table.add_row(&[b, x]);
        let original = table.clone();

        assert_active_columns(&table, &["A", "B"]);
        for c in [x, y] {
            assert_eq!(table.columns[c].prev, c);
            assert_eq!(table.columns[c].next, c);
        }
        // Empty secondary column Y must not be chosen for branching.
        assert_eq!(choose_smallest_column(&table), Some(a));
        assert_column_rows(&table, "X", &[&["X", "A"], &["X", "B"]]);

        table.cover(x);
        assert_active_columns(&table, &["A", "B"]);
        assert_column_rows(&table, "A", &[]);
        assert_column_rows(&table, "B", &[]);
        table.uncover(x);
        assert_eq!(table, original);
    }

    #[test]
    fn choose_shortest_column() {
        let table = Table::from_reader(&b"A B C\nA B\nA\nC\nC\n"[..]).unwrap();
        assert_eq!(choose_smallest_column(&table), Some(column_id(&table, "B")));
    }

    #[test]
    fn choose_first_active_column_on_ties() {
        let mut table = Table::from_reader(&b"Z A M\nZ\nA\nM\n"[..]).unwrap();
        let z = column_id(&table, "Z");
        assert_eq!(choose_smallest_column(&table), Some(z));

        table.cover(z);
        assert_eq!(choose_smallest_column(&table), Some(column_id(&table, "A")));
    }

    #[test]
    fn choose_empty_column() {
        let mut table = Table::from_reader(&b"A B C\nA\nC\n"[..]).unwrap();
        let b = column_id(&table, "B");
        assert_eq!(choose_smallest_column(&table), Some(b));

        // The covered empty column must no longer participate in selection.
        table.cover(b);
        assert_eq!(choose_smallest_column(&table), Some(column_id(&table, "A")));
        table.uncover(b);
        assert_eq!(choose_smallest_column(&table), Some(b));
    }

    #[test]
    fn choose_none_when_no_active_columns_remain() {
        for input in ["A", "A\nA\n"] {
            let mut table = Table::from_reader(input.as_bytes()).unwrap();
            let a = column_id(&table, "A");
            assert_eq!(choose_smallest_column(&table), Some(a));

            table.cover(a);
            assert_eq!(choose_smallest_column(&table), None);
            table.uncover(a);
            assert_eq!(choose_smallest_column(&table), Some(a));
        }
    }

    #[test]
    fn choose_smallest_column_after_cover_and_uncover() {
        let mut table = Table::from_reader(PAPER_TABLE.as_bytes()).unwrap();
        let a = column_id(&table, "A");
        let d = column_id(&table, "D");
        let e = column_id(&table, "E");
        assert_eq!(choose_smallest_column(&table), Some(a));

        table.cover(a);
        assert_eq!(choose_smallest_column(&table), Some(d));
        table.cover(d);
        assert_eq!(choose_smallest_column(&table), Some(e));
        table.uncover(d);
        assert_eq!(choose_smallest_column(&table), Some(d));
        table.uncover(a);
        assert_eq!(choose_smallest_column(&table), Some(a));
    }

    #[test]
    fn cover_a_matches_paper() {
        let mut table = Table::from_reader(PAPER_TABLE.as_bytes()).unwrap();
        let original = table.clone();
        let a = column_id(&table, "A");

        table.cover(a);

        assert_active_columns(&table, &["B", "C", "D", "E", "F", "G"]);
        assert_column_rows(&table, "A", &[&["A", "D", "G"], &["A", "D"]]);
        assert_column_rows(&table, "B", &[&["B", "C", "F"], &["B", "G"]]);
        assert_column_rows(&table, "C", &[&["C", "E", "F"], &["C", "F", "B"]]);
        assert_column_rows(&table, "D", &[&["D", "E", "G"]]);
        assert_column_rows(&table, "E", &[&["E", "F", "C"], &["E", "G", "D"]]);
        assert_column_rows(&table, "F", &[&["F", "C", "E"], &["F", "B", "C"]]);
        assert_column_rows(&table, "G", &[&["G", "B"], &["G", "D", "E"]]);

        assert_eq!(table.columns[a], original.columns[a]);
        assert_eq!(table.nodes.len(), original.nodes.len());
        for (after, before) in table.nodes.iter().zip(&original.nodes) {
            assert_eq!(
                (after.left, after.right, after.column),
                (before.left, before.right, before.column),
            );
        }
    }

    #[test]
    fn cover_uncover_restores_every_field() {
        let mut table = Table::from_reader(PAPER_TABLE.as_bytes()).unwrap();
        let original = table.clone();
        for column in 1..table.columns.len() {
            table.cover(column);
            table.uncover(column);
            assert_eq!(table, original, "restoration failed for column {column}");
        }
    }

    #[test]
    fn nested_covers_restore_in_reverse_order() {
        let mut table = Table::from_reader(PAPER_TABLE.as_bytes()).unwrap();
        let original = table.clone();
        let a = column_id(&table, "A");
        let d = column_id(&table, "D");

        table.cover(a);
        let after_a = table.clone();
        table.cover(d);
        assert_active_columns(&table, &["B", "C", "E", "F", "G"]);
        assert_column_rows(&table, "D", &[&["D", "E", "G"]]);
        assert_column_rows(&table, "E", &[&["E", "F", "C"]]);
        assert_column_rows(&table, "G", &[&["G", "B"]]);

        table.uncover(d);
        assert_eq!(table, after_a);
        table.uncover(a);
        assert_eq!(table, original);
    }

    /// Collect solutions as row-node indices and check the search's success state.
    fn collect_solutions(table: &mut Table) -> Vec<Vec<NodeId>> {
        let mut solutions = Vec::new();
        search(table, |solution| {
            assert_active_columns(solution.table(), &[]);
            solutions.push(solution.row_nodes().to_vec());
        });
        solutions
    }

    /// Convert solutions to column names, sorting names, rows, and solutions
    /// so comparisons do not depend on the chosen row nodes or search order.
    fn solution_names(table: &Table, solutions: &[Vec<NodeId>]) -> Vec<Vec<Vec<String>>> {
        let mut named_solutions = Vec::new();
        for solution in solutions {
            let mut rows = Solution {
                table,
                rows: solution,
            }
            .rows();
            for names in &mut rows {
                names.sort_unstable();
            }
            rows.sort();
            named_solutions.push(rows);
        }
        named_solutions.sort();
        named_solutions
    }

    #[test]
    fn search_paper_table() {
        let mut table = Table::from_reader(PAPER_TABLE.as_bytes()).unwrap();
        let original = table.clone();
        let solutions = collect_solutions(&mut table);
        assert_eq!(
            solution_names(&table, &solutions),
            vec![vec![vec!["A", "D"], vec!["B", "G"], vec!["C", "E", "F"]]],
        );
        assert_eq!(table, original);

        // The restored table can be searched again with identical results.
        assert_eq!(collect_solutions(&mut table), solutions);
        assert_eq!(table, original);
    }

    #[test]
    fn solution_names_can_outlive_search_and_table() {
        let mut saved = Vec::new();
        {
            let mut table = Table::from_reader(PAPER_TABLE.as_bytes()).unwrap();
            search(&mut table, |solution| saved.push(solution.rows()));
        }
        // Names preserve selected row order and each row's starting node.
        assert_eq!(
            saved,
            vec![vec![vec!["A", "D"], vec!["E", "F", "C"], vec!["B", "G"]]],
        );
    }

    #[test]
    fn search_multiple_solutions() {
        let mut table = Table::from_reader(&b"A B C\nA B\nC\nA\nB C\nB\n"[..]).unwrap();
        let original = table.clone();
        let solutions = collect_solutions(&mut table);
        assert_eq!(
            solution_names(&table, &solutions),
            vec![
                vec![vec!["A"], vec!["B"], vec!["C"]],
                vec![vec!["A"], vec!["B", "C"]],
                vec![vec!["A", "B"], vec!["C"]],
            ],
        );
        assert_eq!(table, original);
    }

    #[test]
    fn search_secondary_columns_are_optional_but_cannot_repeat() {
        // X may be used by either row or neither, but not both. Empty Y must
        // not prevent any of these solutions.
        let mut table = Table::from_reader(&b"A B | X Y\nA X\nA\nB X\nB\n"[..]).unwrap();
        let original = table.clone();
        let solutions = collect_solutions(&mut table);
        assert_eq!(
            solution_names(&table, &solutions),
            vec![
                vec![vec!["A"], vec!["B"]],
                vec![vec!["A"], vec!["B", "X"]],
                vec![vec!["A", "X"], vec!["B"]],
            ],
        );
        assert_eq!(table, original);
    }

    #[test]
    fn search_secondary_conflict_makes_table_unsatisfiable() {
        let mut table = Table::from_reader(&b"A B | X\nA X\nB X\n"[..]).unwrap();
        let original = table.clone();
        assert!(collect_solutions(&mut table).is_empty());
        assert_eq!(table, original);
    }

    #[test]
    fn search_does_not_select_secondary_only_rows() {
        let mut table = Table::from_reader(&b"A | X\nA\nX\n"[..]).unwrap();
        let original = table.clone();
        let solutions = collect_solutions(&mut table);
        assert_eq!(solution_names(&table, &solutions), vec![vec![vec!["A"]]]);
        assert_eq!(table, original);
    }

    #[test]
    fn search_unsatisfiable_tables() {
        for input in ["A", "A B\nA\n", "A B C\nA B\nA C\nB C\n"] {
            let mut table = Table::from_reader(input.as_bytes()).unwrap();
            let original = table.clone();
            assert!(collect_solutions(&mut table).is_empty());
            assert_eq!(table, original);
        }
    }

    #[test]
    fn search_singleton_rows() {
        for (input, expected) in [
            ("A\nA\n", vec![vec!["A"]]),
            ("A B C\nA\nB\nC\n", vec![vec!["A"], vec!["B"], vec!["C"]]),
        ] {
            let mut table = Table::from_reader(input.as_bytes()).unwrap();
            let original = table.clone();
            let solutions = collect_solutions(&mut table);
            assert_eq!(solution_names(&table, &solutions), vec![expected]);
            assert_eq!(table, original);
        }
    }

    #[test]
    fn search_wraps_around_from_middle_of_row() {
        // B is shortest, so the candidate starts in the middle of row A B C.
        let mut table = Table::from_reader(&b"A B C\nA B C\nA\nC\n"[..]).unwrap();
        let original = table.clone();
        let solutions = collect_solutions(&mut table);
        assert_eq!(
            solution_names(&table, &solutions),
            vec![vec![vec!["A", "B", "C"]]]
        );
        assert_eq!(table, original);
    }

    #[test]
    fn search_duplicate_rows_are_distinct_choices() {
        let mut table = Table::from_reader(&b"A B\nA\nA\nB\n"[..]).unwrap();
        let original = table.clone();
        let solutions = collect_solutions(&mut table);
        assert_eq!(
            solution_names(&table, &solutions),
            vec![vec![vec!["A"], vec!["B"]]; 2]
        );
        assert_ne!(solutions[0], solutions[1]);
        assert_eq!(table, original);
    }

    #[test]
    fn search_empty_table_has_one_empty_solution() {
        let mut table = Table::new();
        let original = table.clone();
        assert_eq!(collect_solutions(&mut table), vec![Vec::<NodeId>::new()]);
        assert_eq!(table, original);
    }

    #[test]
    fn cover_empty_column() {
        let mut table = Table::from_reader(&b"A B C\nA C\n"[..]).unwrap();
        let original = table.clone();
        let b = column_id(&table, "B");

        table.cover(b);
        assert_active_columns(&table, &["A", "C"]);
        assert_column_rows(&table, "B", &[]);
        assert_eq!(table.nodes, original.nodes);
        table.uncover(b);
        assert_eq!(table, original);
    }

    #[test]
    fn cover_column_with_singleton_rows() {
        let mut table = Table::from_reader(&b"A B\nA\nA\nB\n"[..]).unwrap();
        let original = table.clone();
        let a = column_id(&table, "A");

        table.cover(a);
        assert_active_columns(&table, &["B"]);
        assert_column_rows(&table, "A", &[&["A"], &["A"]]);
        assert_column_rows(&table, "B", &[&["B"]]);
        assert_eq!(table.nodes, original.nodes);
        table.uncover(a);
        assert_eq!(table, original);
    }

    #[test]
    fn cover_only_column() {
        for input in ["A", "A\nA\nA\n"] {
            let mut table = Table::from_reader(input.as_bytes()).unwrap();
            let original = table.clone();
            let a = column_id(&table, "A");

            table.cover(a);
            assert_active_columns(&table, &[]);
            assert_eq!(table.nodes, original.nodes);
            table.uncover(a);
            assert_eq!(table, original);
        }
    }

    #[test]
    fn singleton_and_repeated_rows() {
        let table = Table::from_reader(&b"A B X Y\nA X\nB\nY\nA X\n"[..]).unwrap();
        assert_links(
            &table,
            &["A", "B", "X", "Y"],
            &[&["A", "X"], &["B"], &["Y"], &["A", "X"]],
        );
    }

    #[test]
    fn empty_columns_and_no_rows() {
        for input in ["A", "A\n"] {
            let table = Table::from_reader(input.as_bytes()).unwrap();
            assert_links(&table, &["A"], &[]);
        }
        let table = Table::from_reader(&b"A B X\nA\n"[..]).unwrap();
        assert_links(&table, &["A", "B", "X"], &[&["A"]]);
    }

    #[test]
    fn whitespace_long_names_and_optional_final_newline() {
        let table =
            Table::from_reader(&b" first_column\tB X\r\n\tB first_column  \r\nX"[..]).unwrap();
        assert_links(
            &table,
            &["first_column", "B", "X"],
            &[&["B", "first_column"], &["X"]],
        );
    }

    #[test]
    fn invalid_input() {
        for (input, message) in [
            ("", "line 1: missing column names"),
            ("\n", "line 1: no columns"),
            ("|\n", "line 1: no primary columns"),
            ("| X\n", "line 1: no primary columns"),
            ("A | |\n", "line 1: repeated '|' separator"),
            ("A | X | Y\n", "line 1: repeated '|' separator"),
            ("A A\n", "line 1: duplicate column name \"A\""),
            ("A | A\n", "line 1: duplicate column name \"A\""),
            ("A | X X\n", "line 1: duplicate column name \"X\""),
            ("A\nB\n", "line 2: unknown column name \"B\""),
            ("A | X\nA Y\n", "line 2: unknown column name \"Y\""),
            ("A | X\nA | X\n", "line 2: unknown column name \"|\""),
            ("A\nA A\n", "line 2: repeated column \"A\" in row"),
            ("A | X\nA X X\n", "line 2: repeated column \"X\" in row"),
            ("A\n\n", "line 2: empty row"),
            ("A\nA\n \t\n", "line 3: empty row"),
        ] {
            let error = Table::from_reader(input.as_bytes()).unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::InvalidData);
            assert_eq!(error.to_string(), message);
        }
    }

    #[test]
    fn reader_errors_are_propagated() {
        struct BrokenReader;
        impl io::Read for BrokenReader {
            fn read(&mut self, _: &mut [u8]) -> io::Result<usize> {
                Err(io::Error::other("read failed"))
            }
        }
        let input = io::Read::chain(io::Cursor::new(b"A\nA\n"), BrokenReader);
        let error = Table::from_reader(io::BufReader::new(input)).unwrap_err();
        assert_eq!(error.kind(), io::ErrorKind::Other);
        assert_eq!(error.to_string(), "read failed");
    }
}
