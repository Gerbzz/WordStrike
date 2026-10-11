# WordStrike

WordStrike is a Windows-first Rust document editor prototype. It edits an open `.wstrike` archive, supports structured text blocks, page setup, embedded PNG/JPEG images, undo/redo, and a bounded DOCX preview and import. The optional native editor exports PDF through the selected DesignCraft page crates.

This repository layout is a source package from the frozen 2026-10-11 Office checkpoint. The standalone layout passed 21 locked all-feature tests and built a Windows native executable. Independent review of the frozen outputs checked synthetic, searchable PDF text. No release or installer is supplied.

## Build and test

Install a Rust toolchain that supports edition 2024, then from the repository root run:

```powershell
cargo test --locked --manifest-path apps/wordstrike/Cargo.toml --all-features
cargo build --locked --manifest-path apps/wordstrike/Cargo.toml --bin wordstrike-native --features native-ui
```

The app has its own `Cargo.lock` and `[workspace]`. Its path dependencies resolve to `vendor/designcraft-page`, whose eight selected Rust crates, needed fonts, hyphenation data, ICC profile, upstream licenses, and notices are included. Synthetic DOCX and image fixtures live in `fixtures/wordstrike` at their tested relative paths. Cargo fetches the remaining dependencies from crates.io.

## Limits and source terms

Pagination uses conservative block-height estimates. DOCX import is bounded and refuses tracked changes and broken image references; imported inline bold and italic styling can be flattened. The editable `.wstrike` format is the source of truth, rather than a claim of Word document round-trip fidelity. Native pointer, keyboard, screen reader, high contrast, scaling, font fallback, and other-platform acceptance are pending.

The app's first-party license has not been selected; no first-party license grant is made here. The bundled upstream DesignCraft subset retains its original MIT/Apache license texts and third-party font/asset notices. See [SOURCE_SCOPE.md](SOURCE_SCOPE.md) for provenance and the tested boundary.
