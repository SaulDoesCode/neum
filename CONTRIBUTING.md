# contributing

## layout

| file | what it is |
|---|---|
| `src/gematria.rs` | the sums (ordinal, reversed, bytes), tokens, factoring, the byte sum story |
| `src/store.rs` | the memory: tables, ids, indexes, lists, notes, search, backup, repair, the open memory |
| `src/settings.rs` | settings kept between runs |
| `src/main.rs` | the window |
| `tests/` | the library end to end, on temporary files |

## before a pull request

```console
cargo test
cargo clippy --all-targets
```

Both run in CI. On Linux, building needs `libxkbcommon-dev`, `libwayland-dev` and `pkg-config`.

## changing the memory's tables

Bump `SCHEMA` in `src/store.rs`. `Memory::open` runs `repair_in` on anything older, which rebuilds
every index from the memories themselves; a new index only has to be part of `index`, `unindex`
and `repair_in`. Add a test that opens a memory written the old way (see
`a_schema_1_memory_gets_its_byte_bucket` in `tests/store.rs`).

## the fast paths

The AVX2 sums and tokenizer must agree with their scalar versions byte for byte; the tests in
`src/gematria.rs` check that over many lengths, including the block edges.
