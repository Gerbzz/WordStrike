# Source scope and provenance

- Source checkpoint: the Office task-4 source, frozen in `docs/word-slide-build-hashes-20261011.tsv` there. This original local manifest is verification provenance; the packaged build is self-contained.
- Copied without byte changes: `apps/wordstrike/Cargo.lock`, `apps/wordstrike/src`, `apps/wordstrike/tests`, `fixtures/wordstrike`, and `vendor/designcraft-page`.
- The only edit to the original app source metadata removes the unsupported first-party `MIT OR Apache-2.0` declaration from `apps/wordstrike/Cargo.toml`. Path dependencies, package features and locked graph are otherwise unchanged.
- Selected donor: `storytold/designcraft` commit `a9124a0517594ce9872f73637e7b841a4967bcf9`, verified source archive SHA-256 `fc6f5287328145aabc75ab56e32a7e574dc8d2cd07184f4e6ceeec4d7f53f384`. The selected donor workspace contains only `geom`, `color`, `doc`, `fonts`, `compose`, `images`, `render`, and `pdf` crates. The whole selected subset is retained to preserve its tested Cargo graph.
- The required `assets/hyphenation/en-us.dic` and `en-us.pat` came from that pinned archive. The donor's `LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE`, `ASSETS.md`, font OFL texts, fonts, and ICC profile remain with the source. Original donor notices also describe upstream files excluded from this selected subset. No upstream brand assets or marks are included.
- Self-authored synthetic fixtures only. No user documents, binaries, build outputs, captures, application state, or credentials are packaged.
- Both the Office checkpoint and this standalone layout passed `cargo test --locked --manifest-path apps/wordstrike/Cargo.toml --all-features` (21 tests) and built `wordstrike-native`. Independent review reran existing native UI, import and donor tests and inspected searchable PDF output from small fixtures. Direct native input acceptance remains pending.

This is a source package, not a redistributable binary release. Review transitive dependencies and notices before binary distribution.
