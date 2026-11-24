# knuth-dlx

Knuth's dancing links (DLX) algorithm implementation

Run `cargo run -- < matrix.txt` to read a matrix from stdin and print its column
and nonzero-entry counts. The format follows `knuth-resources/dance.w`:

```text
A B C D E F G
C E F
A D G
B C F
A D
B G
D E G
```
