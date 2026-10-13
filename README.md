# WordStrike

**by PWDS**

**Give every page a purpose.**

WordStrike is a Windows-first workspace for building documents from text blocks and images. Edit a `.wstrike` project, adjust pages, undo changes, and export PDF from the optional native editor.

### Highlights

- Compose structured text with embedded PNG or JPEG images.
- Keep an editable `.wstrike` document.
- Preview or import a bounded DOCX subset and export PDF.

## Current status

Source-only editor prototype. The standalone package passed 21 locked tests and built a Windows native executable, but no installer or release is supplied. DOCX support is bounded and native interaction, accessibility, and broad format fidelity remain unverified.

## Build a document

Add structured text and images to a `.wstrike` project, revise page setup, and use undo or redo while editing. The native path can export PDF; a bounded DOCX import and preview helps bring in source material, with the editable project remaining the source of truth.

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
