//! Construction of the sparse matrix representation used by dancing links.

use std::collections::HashMap;
use std::io::{self, BufRead};

/// An index into [`Table::nodes`].
pub type NodeId = usize;

/// An index into [`Table::columns`].
pub type ColumnId = usize;

/// The sentinel at the head of the circular list of columns.
pub const ROOT: ColumnId = 0;

/// A nonzero matrix entry, or a column's header node.
#[derive(Debug)]
pub struct Node {
    pub left: NodeId,
    pub right: NodeId,
    pub up: NodeId,
    pub down: NodeId,
    pub column: ColumnId,
}

/// A column record.
#[derive(Debug)]
pub struct Column {
    /// The column's header/sentinel in the node vector.
    pub head: NodeId,
    /// Number of data nodes in the column, excluding its header.
    pub len: usize,
    pub name: String,
    /// Neighbors in the circular column list.
    pub prev: ColumnId,
    pub next: ColumnId,
}

/// A sparse matrix stored as circular, doubly linked row and column lists.
///
/// Node and column indices refer to separate vectors. Each column, including
/// the root sentinel, has a header node. Rows have no headers. Header nodes'
/// horizontal links are unused and point to themselves.
#[derive(Debug)]
pub struct Table {
    nodes: Vec<Node>,
    columns: Vec<Column>,
}

impl Table {
    /// Read the input format used by `dance.w`.
    ///
    /// The first line lists column names. At least one column is required.
    /// The `|` separator for secondary columns is not supported.
    /// Each subsequent line lists the columns containing a 1 in one row.
    /// Names are whitespace-delimited; unlike `dance.w`, their length is not
    /// limited to seven characters, and the final newline is optional.
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
        for name in header.split_whitespace() {
            if name == "|" {
                return Err(input_error(1, "unexpected '|' in column names"));
            }
            if column_ids.contains_key(name) {
                return Err(input_error(1, format!("duplicate column name {name:?}")));
            }
            let column = table.add_column(name);
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

    fn new() -> Self {
        let mut table = Self {
            nodes: Vec::new(),
            columns: Vec::new(),
        };
        // The root has an empty name and initially links to itself.
        table.add_column("");
        table
    }

    fn add_column(&mut self, name: &str) -> ColumnId {
        let column = self.columns.len();
        let head = self.nodes.len();
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
            prev: column,
            next: column,
        });
        let prev = self.columns[ROOT].prev;
        self.columns[column].prev = prev;
        self.columns[column].next = ROOT;
        self.columns[prev].next = column;
        self.columns[ROOT].prev = column;
        column
    }

    fn add_row(&mut self, columns: &[ColumnId]) {
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
            // Append to this column's vertical list, immediately before its header.
            self.nodes[up].down = node;
            self.nodes[head].up = node;
            self.columns[column].len += 1;
        }
    }
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

    /// Verify both directions of each list, including header and length invariants.
    fn assert_links(table: &Table, expected_rows: &[&[&str]], expected_columns: &[&str]) {
        let nodes = table.nodes();
        let columns = table.columns();
        assert_eq!(columns.len(), expected_columns.len() + 1);
        assert_eq!(columns[ROOT].name, "");
        assert_eq!(columns[ROOT].len, 0);

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
    fn paper_matrix() {
        let table =
            Table::from_reader(&b"A B C D E F G\nC E F\nA D G\nB C F\nA D\nB G\nD E G\n"[..])
                .unwrap();
        assert_links(
            &table,
            &[
                &["C", "E", "F"],
                &["A", "D", "G"],
                &["B", "C", "F"],
                &["A", "D"],
                &["B", "G"],
                &["D", "E", "G"],
            ],
            &["A", "B", "C", "D", "E", "F", "G"],
        );
    }

    #[test]
    fn singleton_and_repeated_rows() {
        let table = Table::from_reader(&b"A B X Y\nA X\nB\nY\nA X\n"[..]).unwrap();
        assert_links(
            &table,
            &[&["A", "X"], &["B"], &["Y"], &["A", "X"]],
            &["A", "B", "X", "Y"],
        );
    }

    #[test]
    fn empty_columns_and_no_rows() {
        for input in ["A", "A\n"] {
            let table = Table::from_reader(input.as_bytes()).unwrap();
            assert_links(&table, &[], &["A"]);
        }
        let table = Table::from_reader(&b"A B X\nA\n"[..]).unwrap();
        assert_links(&table, &[&["A"]], &["A", "B", "X"]);
    }

    #[test]
    fn whitespace_long_names_and_optional_final_newline() {
        let table =
            Table::from_reader(&b" first_column\tB X\r\n\tB first_column  \r\nX"[..]).unwrap();
        assert_links(
            &table,
            &[&["B", "first_column"], &["X"]],
            &["first_column", "B", "X"],
        );
    }

    #[test]
    fn invalid_input() {
        for (input, message) in [
            ("", "line 1: missing column names"),
            ("\n", "line 1: no columns"),
            ("| X\n", "line 1: unexpected '|' in column names"),
            ("A | X\n", "line 1: unexpected '|' in column names"),
            ("A A\n", "line 1: duplicate column name \"A\""),
            ("A\nB\n", "line 2: unknown column name \"B\""),
            ("A\nA A\n", "line 2: repeated column \"A\" in row"),
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
