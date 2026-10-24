# Partial table-cell margin updates

This bounded backport applies the production change from
[`storytold/wordcraft` PR #3509](https://github.com/storytold/wordcraft/pull/3509),
merged as `0af1a197ad34c76ad83f631468ed9ff502bf9a7e`, to the existing native
WordStrike 0.6.1 workspace. The original WordStrike base is
`3379ee3244b20075eab95d7a97e7b97f4d861ad5`.

`table.cellMargins` now resolves the current array from direct table properties,
then the resolved table style (including its based-on chain), then the Word
defaults `[0.0, 5.4, 0.0, 5.4]` points. Supplied sides replace their corresponding
values instead of resetting other sides to defaults. The existing 0–144 point
clamp remains on the resulting array, including inherited or direct values on
omitted sides. Table-context refusal, paragraph cache invalidation, selection
result, and the session's undo/save behavior are unchanged.

The production edit is the upstream 10-line addition / 5-line removal in
`native/wordstrike/crates/engine/src/cmd/tools.rs`; this is not a donor snapshot
update. No dependencies, lockfiles, document schemas, legacy importer/dirty
guards, branding, or license/notice files are changed.

The real-engine regression target expands the upstream sequential-edit scenario
with inherited-style fallback, direct-property precedence, zero and empty
updates, undo/redo, clamp/refusal/cache behavior, and native DOCX Save As/reopen:

```sh
cd native/wordstrike
cargo test --locked -p wordcraft-engine --test table_cell_margins
cargo test --locked -p wordcraft-engine
```

The existing Windows workflow runs the new target alongside the retained legacy
import, safe-save, and UI guard checks, with no additional permissions or service.
These synthetic engine checks do not establish interactive native desktop
acceptance, broad DOCX fidelity, or release readiness.
