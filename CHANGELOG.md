# changelog

## 0.2.0

### added

- **b**, the byte sum bucket: a third sum beside ordinal and reversed. The middle letter now cycles
  `o → r → b`. Every memory is indexed by its byte sum, so the bucket can be searched like the
  other two.
- the byte sum story of the input, shown in **b** mode. Input written as bytes (`104,105`) is read
  as those bytes.
- notes on the gematria channel, working as lists do on the search channel: Enter notes what was
  typed on the alt name, Enter on an empty bar shows the alt name's notes, Delete unlinks one.
- light mode, toggled from the escape menu.
- the escape menu: the open memory's details, saves (open, new, close, export, import, recent),
  settings and keys.
- settings kept between runs (`neum/settings.json` in the config folder, or `$NEUM_CONFIG`).
- `neum [MEMORY]`, `--help` and `--version`.
- a status line under the bar instead of messages printed to the terminal.
- tests: the sums and their AVX2 fast paths against scalar, byte sums, factoring, the memory end
  to end (ids, indexes, lists, notes, search, backup and restore, repair, migration, saves) and
  settings. CI on GitHub Actions.
- `examples/inspect.rs`.

### changed

- the code is split into a library (`gematria`, `store`, `settings`) and the window (`main.rs`).
- the open memory can be swapped at run time; a memory that can't be opened no longer panics.
- a number typed on the gematria channel looks up that sum (ctrl+Enter sums it as text).
- a note now goes *on* the alt name (before, the alt name went on what was typed).
- entries show in id order, and memories that aren't UTF-8 show as their bytes.
- forgetting a list lets its members go; a memory can't be a note on itself.
- dependencies have real version requirements instead of `*`.

### schema

- memories gain a version in `STATE` (v2) and the `MEMORY_BYTE_SUM` index. A v1 memory is brought
  up to date the first time it opens.

## 0.1.0

- ordinal and reversed gematria over a redb memory: saturate, search by words and sums, forget.
- lists on the search channel.
