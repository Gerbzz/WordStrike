# WordStrike native source adaptation

## Source and scope

This workspace is based on `storytold/wordcraft` commit
[`04f6f53b8a4d8140c7bb62f949b0a6099a473da1`](https://github.com/storytold/wordcraft/commit/04f6f53b8a4d8140c7bb62f949b0a6099a473da1)
(upstream version 0.6.1). The import copied 428 tracked files, totaling 16,083,644 bytes,
from `apps`, `crates`, `contributors`, `xtask`, the necessary fonts/dictionaries,
and root build/license metadata. It excluded upstream application icons,
packaging, screenshots, marketing documents, restricted brand art, and build outputs.
The source snapshot can be checked against the pinned upstream commit; the
adaptation edits are in this repository's history.

The desktop binary is named `wordstrike`; its window title, product metadata,
visible UI names, website link, and embedded egui icon use the original
WordStrike crane. Internal crate/package names stay `wordcraft-*` so the
upstream dependency graph remains coherent. Windows executable icon packaging
is pending an approved `.ico` version of the crane. The window/taskbar icon
uses the original PNG now. OS default-app integration is disabled until
WordStrike installers and desktop entries exist; the donor's packaging is not
silently reused.

## Legacy document conversion

`file.importLegacy` is an explicit engine command that reads the earlier
`.wstrike` archive through `apps/wordstrike`. It creates a native document
in memory, clears the current native path and undo history, and requires
Save As. It leaves the source file unchanged. The converter maps page setup,
title, two heading levels, paragraph text with bold/italic runs, page breaks,
PNG/JPEG images, and alt text. It refuses lists, invalid heading levels,
unbound object markers, missing images, and unattached assets. The original
bounded ZIP/schema/image validator remains in the legacy reader. Page breaks,
font metrics, and pagination can render differently in the new model.

The converter and two self-authored regression tests came from a local
WordCraft adaptation at upstream commit
[`191d8fd48ec28b959dc269159a3b0f2c8b885669`](https://github.com/storytold/wordcraft/commit/191d8fd48ec28b959dc269159a3b0f2c8b885669),
where those tests passed 2/2 on Windows. Their APIs were reviewed against the
newer pinned source, but **they have not run on this revision**. The new local
dependency also needs a refreshed `Cargo.lock` before `--locked` builds.

## Verification and limits

`cargo metadata --offline --no-deps` and `cargo fmt --all -- --check` pass.
Full offline Cargo resolution stops at pinned `storytold/craft-libs` revision
`d27646a5f7c8b4fc85d9dd3213b8c88b58e0088d`, which is not cached on the
authorized machine. No build, release, installer, interactive acceptance, or
format-fidelity claim is made for this revision. The donor `xtask assets` audit
references upstream assets intentionally excluded here and needs adaptation.

The upstream MIT/Apache code license, `NOTICE`, contributors, and all included
font/dictionary license texts remain present. `ATTRIBUTION.md` is the donor's
original asset inventory; some rows describe files not copied. No donor app
icon or restricted brand file is distributed. The crane and PWDS logos are
separate original suite artwork, with provenance in the repository's
`assets/branding` directory. The first-party legacy WordStrike code has no
selected license; no new license grant is inferred from the donor license.
