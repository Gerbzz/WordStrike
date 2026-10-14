<p align="center">
  <img src="assets/branding/WordStrike.png" alt="Original WordStrike logo" width="160" height="160">
</p>

<h1 align="center">WordStrike</h1>

<p align="center">
  <a href="https://prowebdevsolutions.com"><picture>
    <source media="(prefers-color-scheme: dark)" srcset="assets/branding/PWDS-logo-orange.svg">
    <img src="assets/branding/PWDS-logo.png" alt="PWDS company logo" width="120">
  </picture></a>
</p>

<p align="center"><strong>Give every page a purpose.</strong></p>

WordStrike is a Windows-first document editor. This repository contains a full native editing foundation for page layout, rich text, DOCX, PDF, proofing, and a ribbon-style desktop interface, alongside the original lightweight `.wstrike` editor and an explicit import path between them.

![WordStrike native editor showing a synthetic field-notes document with structured text, a blue chart, and a page break](assets/screenshots/wordstrike-editor.png)

This screenshot shows the **older lightweight editor**, not the newer native foundation. It opened a self-authored field-notes project for this capture. See [screenshot provenance](docs/SCREENSHOT_PROVENANCE.md).

<p align="center"><a href="#foundation">Foundation</a> · <a href="#build-and-test">Build and test</a> · <a href="assets/branding">Original artwork</a></p>

## Foundation

- `native/wordstrike` contains the complete source graph of the WordCraft document, layout, DOCX, PDF, proofing, command, and egui desktop crates, pinned to upstream commit [`04f6f53`](https://github.com/storytold/wordcraft/commit/04f6f53b8a4d8140c7bb62f949b0a6099a473da1).
- The desktop target is named `wordstrike`, uses the original crane artwork and WordStrike identity, and exposes the donor's ribbon, page canvas, formatting, review, and document commands. Internal Rust crate names remain unchanged for dependency compatibility.
- File > Open offers an import picker for the original `.wstrike` archive. `file.importLegacy` converts it into a new unsaved native document, prompting before replacing unsaved work. It maps titles, level 1-2 headings, styled paragraph runs, page setup, page breaks, and bounded, fully decoded PNG/JPEG images with alt text. It refuses lists, invalid image references, unbound object markers, and unsupported heading levels rather than silently dropping them.
- `apps/wordstrike` remains the original lightweight editor. It keeps its own archive format and can still be built independently.

## Current status

**Source-stage migration.** The newer native source is present. [Windows CI for source revision `a4f22da`](https://github.com/Gerbzz/WordStrike/actions/runs/38561143477) built the desktop editor and passed the targeted legacy import engine tests (5/5), safe-save tests (6/6), import UI tests (4/4), and Save Changes UI guard (1/1). The engine cases cover conversion, Save As and reopen, read-root confinement, invalid images, and the image dimension boundary. Cargo also resolves the locked graph offline locally. No installer or binary release is supplied. The editor has not been launched and visually accepted for this revision; interactive editing, accessibility, and broad file fidelity need verification.

## Files and export

The lightweight editor uses `.wstrike` as its editable source. Its DOCX import covers a bounded subset; tracked changes and broken image references are refused, and inline styling can flatten. The newer native engine has its own document model and DOCX path. The explicit import command leaves the original `.wstrike` file untouched and requires **Save As** to create a native document. Layout may change during conversion.

## Build and test

Install Rust 1.95 or newer (required by the pinned `craft-libs`). The native workspace needs crates.io packages and the pinned `storytold/craft-libs` Git dependency. From the repository root:

```powershell
cargo run --manifest-path native/wordstrike/Cargo.toml -p wordcraft --bin wordstrike
cargo test --offline --locked -j 1 --manifest-path native/wordstrike/Cargo.toml -p wordcraft-engine --test legacy_import
cargo test --offline --locked -j 1 --manifest-path native/wordstrike/Cargo.toml -p wordcraft-engine --test safe_save
cargo test --locked --manifest-path native/wordstrike/Cargo.toml -p wordcraft-ui-egui --lib legacy_import
cargo test --locked --manifest-path native/wordstrike/Cargo.toml -p wordcraft-ui-egui --lib save_changes_waits_for_the_save_as_answer
```

The targeted engine commands passed locally and in CI. The UI commands passed in CI. The desktop editor compiled in CI; the run command remains unverified. The lockfile includes the local legacy dependency and resolves offline on the authorized machine.

To build and test the older lightweight editor independently:

```powershell
cargo test --locked --manifest-path apps/wordstrike/Cargo.toml --all-features
cargo build --locked --manifest-path apps/wordstrike/Cargo.toml --bin wordstrike-native --features native-ui
```

The older editor has its own `Cargo.lock` and `[workspace]`. Its path dependencies resolve to `vendor/designcraft-page`, whose eight selected Rust crates, fonts, hyphenation data, ICC profile, licenses, and notices are included. Synthetic DOCX and image fixtures live in `fixtures/wordstrike`.

## Limits and source terms

The older editor's pagination uses conservative block-height estimates, and its DOCX import is bounded. The newer native foundation has a much broader editing source graph, but this migration branch has no current runtime acceptance evidence. Native pointer, keyboard, screen reader, high contrast, scaling, font fallback, document compatibility, and other-platform behavior remain to be checked. Neither editor is claimed to match Microsoft Word.

The original first-party WordStrike code has no selected license; this repository makes no new grant for it. The native WordCraft-derived workspace retains upstream MIT/Apache license texts, copyright notices, and asset attribution under `native/wordstrike`; the older DesignCraft subset retains its licenses and notices under `vendor/designcraft-page`. See [SOURCE_SCOPE.md](SOURCE_SCOPE.md) and [native adaptation notes](native/wordstrike/ADAPTATION.md).

## PWDS brand artwork

The company mark in this masthead is the existing [official PWDS logo](https://github.com/Gerbzz/PWDS-Website/blob/main/public/prowebdevsolutions-logo.png), with the [official orange display asset](https://prowebdevsolutions.com/images/prowebdevsolutions-logo-orange.svg) for dark backgrounds. The app logo above it remains the original Strike suite artwork.
