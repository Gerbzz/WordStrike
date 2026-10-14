# WordStrike editor screenshot provenance

`assets/screenshots/wordstrike-editor.png` is a genuine 1380 × 840 image returned by WordStrike's own egui/eframe Glow renderer on 14 October 2026 UTC. The editor opened a local, self-authored `.wstrike` project containing a field-notes title, headings, a short paragraph, a blue bar chart, and a page break. No user document, client file, online service, or external font was used.

The first-party native editor source gained a one-shot `--capture FILE.png` option. It waits for two painted frames, requests `egui::ViewportCommand::Screenshot`, saves the returned `egui::Event::Screenshot` as PNG, and closes the capture window. The normal `wordstrike-native FILE.wstrike` path remains available. The screenshot was inspected visually: the toolbar, structure pane, editable blocks, chart, page break, and page-setup pane all render. The app exited with status 0 and left no WordStrike process running.

| Item | SHA-256 |
| --- | --- |
| Synthetic `.wstrike` fixture used locally (not published) | `D2C671F5D9B2BC1E6ECB502A6B1C9A4FD434441C7C45246A50DEB963BF7C16ED` |
| Local capture executable (not published) | `F339E3334BB470E04FF29B0456C3BF64639D2EC9C6FF5F760B4C1FA16B9F0D0B` |
| Published screenshot PNG | `47D1F1A4E39BB53A104DD5A1E5AD29A59292042A045B78070F9DC78C3BCADAEF` |

The executable was built from the source in this repository with `cargo build --locked --offline -j 1 --features native-ui --bin wordstrike-native`. Cargo reported a successful dev build in 40.49 seconds using cached dependencies. The per-second monitor reached its 750 MiB target-growth stop threshold as Cargo finished; the binary's changed hash, Cargo completion line, successful capture run, and screenshot were independently checked. No dependency download, clean, installer, or release was made. Existing upstream source and asset credits remain in `SOURCE_SCOPE.md` and the bundled vendor notices.

This is evidence of the visible editor for one small project. It does not verify interactive editing, saving, PDF export, DOCX fidelity, accessibility, other display scales, or a release build. The earlier 21 locked tests were not rerun after the capture-only source change.
