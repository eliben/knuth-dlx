# knuth-dlx

Knuth's dancing links (DLX) algorithm implementation

The shared table representation and search code live in `src/lib.rs`. The two
binaries in `src/bin/` can be run with Cargo.

## dlx (default binary)

`cargo run --bin dlx -- < table.txt` reads a table from stdin, prints its size,
then lists every solution and the total count. Each solution covers every
primary column exactly once and every secondary column at most once. This is
the default binary, so `cargo run -- < table.txt` also works.

The `dlx` input describes a table of zeros and ones using the sparse format from
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

## nqueens

`cargo run --bin nqueens -- 8` solves the eight-queens problem. Replace `8`
with any positive integer N for an N-by-N board. It prints the total number of
solutions and the first solution as a board (`Q` for queens, `.` for empty
squares). Rotations and reflections count separately. If no solution exists,
it prints `Solutions: 0` without a board.
