# knuth-dlx

Knuth's dancing links (DLX) algorithm implementation

Run `cargo run -- < table.txt` to read a table from stdin and print its column
and nonzero-entry counts.

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

The `|` separator for secondary columns in `dance.w` is not supported.
