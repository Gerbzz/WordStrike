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
visible UI names, and website link use the WordStrike identity. Its embedded
egui and web icons reference the geometric page mark, and Windows resources
reference the matching ICO. Historical crane artwork and provenance remain
unchanged. Internal crate/package names stay `wordcraft-*` so the upstream
dependency graph remains coherent. Native resource packaging and runtime icon
display remain unverified for the new assets. OS default-app integration is disabled until
WordStrike installers and desktop entries exist; the donor's packaging is not
silently reused.

## Legacy document conversion

`file.importLegacy` is an explicit engine command that reads the earlier
`.wstrike` archive through `apps/wordstrike`. It creates a native document
in memory, clears the current native path and undo history, and requires
Save As. The desktop File > Open page has a `.wstrike` import picker, and
replacing an unsaved document first prompts Save, Don't Save, or Cancel.
The importer checks the automation read root before opening the archive;
embedded images are ZIP entries, not external paths. It leaves the source
file unchanged. The converter maps page setup,
title, two heading levels, paragraph text with bold/italic runs, page breaks,
PNG/JPEG images, and alt text. Images must pass the legacy 10 MiB, 8192-pixel
per-side, 20-million-pixel limits and decode completely. It refuses lists, invalid heading levels,
unbound object markers, missing images, and unattached assets. The original
bounded ZIP/schema/image validator remains in the legacy reader. Page breaks,
font metrics, and pagination can render differently in the new model.

The converter and two self-authored regression tests came from a local
WordCraft adaptation at upstream commit
[`191d8fd48ec28b959dc269159a3b0f2c8b885669`](https://github.com/storytold/wordcraft/commit/191d8fd48ec28b959dc269159a3b0f2c8b885669),
where those tests passed 2/2 on Windows. The current revision's expanded
engine import suite passed 5/5 on Windows, including read-root and image
boundary cases. Windows CI also passed the import UI tests (4/4) and the
Save Changes UI guard (1/1).

## Verification and limits

`cargo metadata --offline --locked`, `cargo fmt --all -- --check`, and
targeted `wordcraft-engine` integration tests pass: `legacy_import` reports
5 passed, 0 failed; `safe_save` reports 6 passed, 0 failed. Both test runs
used `--offline --locked -j 1` on Windows.
The pinned `storytold/craft-libs` revision
`d27646a5f7c8b4fc85d9dd3213b8c88b58e0088d` was fetched and inspected.
A monitored single-job build completed the targeted engine tests within the
local disk budget. A later local desktop build stopped at its 2 GiB target-growth
limit. [Windows CI run 38556741197](https://github.com/Gerbzz/WordStrike/actions/runs/38556741197)
built the desktop editor and passed the same engine tests plus 4 import UI and
1 Save Changes UI test. CI disabled debug info and incremental compilation to
fit the standard hosted runner. No release, installer, interactive acceptance, or
format-fidelity claim is made for this revision. The donor `xtask assets` audit
references upstream assets intentionally excluded here and needs adaptation.

The upstream MIT/Apache code license, `NOTICE`, contributors, and all included
font/dictionary license texts remain present. `ATTRIBUTION.md` is the donor's
original asset inventory; some rows describe files not copied. No donor app
icon or restricted brand file is distributed. The crane and PWDS logos are
separate original suite artwork, with provenance in the repository's
`assets/branding` directory. The first-party legacy WordStrike code has no
selected license; no new license grant is inferred from the donor license.
