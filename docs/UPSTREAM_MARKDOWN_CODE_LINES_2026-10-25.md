# Markdown inline-code line endings backport — 2026-10-25

## Origin and retained source

- Selected WordCraft foundation: `04f6f53b8a4d8140c7bb62f949b0a6099a473da1`.
- Narrow upstream fix: [storytold/wordcraft b0b7e6b5ea7589450b01d74cb2a8645312bac370](https://github.com/storytold/wordcraft/commit/b0b7e6b5ea7589450b01d74cb2a8645312bac370), contributor Lucenx9, upstream issue #3804.
- Source path: `crates/formats/src/markdown.rs`; retained path: `native/wordstrike/crates/formats/src/markdown.rs`.
- Strike base: `54899f3e77adad9a72ce3c0866e7f1c737bc12ca`. The retained source matched the selected foundation after line-ending normalization; original CRLF bytes are preserved.
- Production change: only the upstream line normalizing CRLF, CR and LF to spaces before Markdown code-span serialization. No foundation/dependency pins or lockfile changes.

## License and adaptation

Upstream is `MIT OR Apache-2.0`. The imported line and adapted regressions retain [MIT](../native/wordstrike/LICENSE-MIT) and [Apache-2.0](../native/wordstrike/LICENSE-APACHE) notices, copyright 2026 ArtCraft Team and WordCraft contributors. License texts at the donor commit match retained notices after line-ending normalization.

The upstream tests are adapted to the retained `ListInfo { ordered, level }` model, without importing its newer `start` field. Unrelated upstream pipe-in-code behavior is deliberately not asserted or imported. Native raw CR is installed after the flow builder so the tests genuinely exercise serializer input; the existing builder strips CR controls.

## Observed regression and checks

At exact test-only source `5d3682c326a73e3af90db1d8623c46116e5620d5`, both tests compiled and failed as intended ([run 40227234701](https://github.com/Gerbzz/WordStrike/actions/runs/40227234701), job 121271079603, 2026-10-25 19:52 UTC):

- Inline source `a\n# b` exported across two Markdown lines instead of one literal code span.
- Table-cell source containing CR round-tripped to `a# b` instead of `a # b`.

The regression uses real document export/reimport and checks literal text, list shape and code formatting for LF/CR/CRLF, heading/list/HTML/backtick-like content, blank lines, leading/trailing spaces, ordinary text, table cells and a fenced-code control.

The immutable-head hosted workflow runs:

```
cargo test --locked -p wordcraft-formats --lib code_span_line_endings
cargo test --locked -p wordcraft-formats
cargo test --locked -p wordcraft-doc --lib locale_number::tests::
```

The existing native Windows workflow is unchanged and retains document, desktop and legacy/safe-save checks. Earlier locale-overflow fixes, Strike branding, app identity and project conversion remain untouched. Passing checks must be verified on the final candidate; this red receipt is not delivery or native visual acceptance.
