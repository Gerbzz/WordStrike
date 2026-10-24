# Word 2003 XML sniff gateway

The full engine suite exposed a pre-existing source-boundary failure:
`file_access::tests::commands_do_not_touch_the_file_system_themselves` found
`std::fs::File::open` in `cmd/file.rs`'s Word 2003 XML detection helper. That
helper already checked the automation read root before opening the file; this
was not an established confinement bypass or incident.

A separate follow-on commit to the bounded table-margin backport moves the
native and wasm helper implementations unchanged into the existing `io.rs`
gateway, with crate-private visibility, and updates the save call site. It
retains the read-root check, 64 KiB prefix bound, false-on-error/wasm behavior,
and XML-kind predicate. The source guard is unchanged, not skipped or weakened.

Focused synthetic tests cover a valid XML document larger than 64 KiB, denied
read roots and write-only authority, Flat OPC/non-XML, and missing/directory
paths. The existing Word 2003 save-command preservation test remains in use.
The existing Windows workflow now runs these XML tests and the source guard.

The legacy `.wstrike` importer, archive/image/schema limits, dirty and Save As
guards, safe-save publication, dependencies, permissions, branding, and license
notices remain unchanged. No whole-file read replaces the prefix read. Engine
tests do not establish interactive desktop acceptance or release readiness.
