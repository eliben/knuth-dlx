# knuth-dlx

Knuth's dancing links (DLX) algorithm implementation

Run `cargo run -- < table.txt` to read a table from stdin, print its size, and
enumerate all solutions. Each solution lists rows that cover every primary
column exactly once and every secondary column at most once. The final line
reports the total number of solutions.

The shared table representation and search code live in `src/lib.rs`. Binaries
live in `src/bin/`; `src/bin/dlx.rs` implements the stdin interface and is the
default binary. It can also be run explicitly with
`cargo run --bin dlx -- < table.txt`.

The input describes a table of zeros and ones using the sparse format from
`knuth-resources/dance.w`. The first line lists the column names. Each
subsequent line describes one row by listing only the columns containing a 1;
all other columns in that row contain 0.

For example:

```text
A B C D E F G
C E F
A D G
B C F
A D
B G
D E G
```

This declares seven columns and six rows. The first row, `C E F`, represents
`0 0 1 0 1 1 0` in the declared column order.

An optional, whitespace-delimited `|` on the first line separates primary
columns from secondary columns. Without it, all columns are primary. At least
one primary column is required; the secondary section may be empty.

For example:

```text
A B | X
A X
A
B X
B
```

Here `A` and `B` must each be covered exactly once. `X` may remain uncovered,
but cannot be covered twice. The three solutions are rows `(A X, B)`,
`(A, B X)`, and `(A, B)`; `(A X, B X)` is invalid because it repeats `X`.
As in `dance.w`, search only selects rows containing a primary column.
