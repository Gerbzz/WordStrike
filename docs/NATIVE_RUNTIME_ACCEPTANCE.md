# Native editor runtime evidence

## Native branding capture, 21 October 2026

[Windows Actions run 39669849487](https://github.com/Gerbzz/WordStrike/actions/runs/39669849487) built the selected `wordstrike` desktop executable once, then captured it with an isolated profile and a temporary copy of the public [`fixtures/wordstrike/with-image.docx`](../fixtures/wordstrike/with-image.docx) sample. The source head was `1c471e9b8b15ee2f68123c31ba4a557d8a00fbbf`; the tested checkout was PR merge `18c18699efff09e95f6e2997826f20ab7411ec18`. Both share tree `f21557652936f7ee89584c7fe5e56946d8239532`, which reached main through [capture workflow PR #5](https://github.com/Gerbzz/WordStrike/pull/5), merge `1cdd867bdc07ded9c0ca2f5aae3f0af73b11ad68`.

At 20:39:59 UTC the app's read-only control interface confirmed the expected sample text and one document page. The original sample and its temporary copy remained byte-identical. Capture checked that the native HWND belonged to the spawned process and was foreground, with its Windows caption and small icon present. The executable SHA-256 was `5147C25BF64DF524757D870EFA62BF81FA1BBD477E02FD6691E1725AC5020294`.

| Original image | Size | SHA-256 |
| --- | --- | --- |
| [Client frame used in the README](../assets/screenshots/wordstrike-native-geometric-client.png) | 960 x 633 | `8b92ca78eca39df9ce51c5eee3bbfe74b1da5d00eabefa67354fc149ea4e7b18` |
| [Full Windows window with caption icon](../assets/screenshots/wordstrike-native-geometric-window.png) | 976 x 672 | `b9a8d8d1d916071d1f4ea60502a8af8abfa420b7863b472b90d0ef2f21535548` |

The client frame comes directly from the app's `ui.screenshot` response and excludes Windows chrome. The full-window frame contains the original screen pixels at the verified HWND bounds, including narrow runner-desktop margins and hosted Windows evaluation watermark fragments. Neither image was edited, annotated, resized or given a substituted logo. Visual review confirmed the blue geometric caption icon, the legible final caption `WordStrike sample - WordStrike`, ribbon and public sample text. The original proof's `title` field sampled the initial caption `WordStrike` before the later capture; its bytes remain unchanged. The [original scoped proof](wordstrike-native-geometric-proof.json) records the source, executable, fixture and image hashes without a control key or private profile path.

All four checks at this head passed: native build/capture and existing native tests, plus legacy persistence on Windows, Linux and macOS. The screenshots establish this hosted synthetic-document appearance only. They do not establish hands-on Room acceptance, taskbar appearance, seven-payload PE verification, wider display configurations, full DOCX fidelity, accessibility or release readiness. No installer or release was produced.

## Earlier import/save/reopen check, 15 October 2026

The [draft WordStrike foundation PR](https://github.com/Gerbzz/WordStrike/pull/1) was built by [Windows Actions run 38668293554](https://github.com/Gerbzz/WordStrike/actions/runs/38668293554). That run compiled the `wordstrike` desktop target, passed its focused legacy import and safe-save engine tests, passed its import UI and Save Changes guard tests, and retained a one-day executable artifact for local acceptance. The source head was `03ea913de5cada3b391acfc251f762158e53df28`; the tested PR merge build was `637db096b2b1d4a70082434fac01b6d988ea9325`. The 69,568,000-byte debug executable reported `wordstrike 0.6.1 (dev)` and SHA-256 `D84630DCCF0618FEB9D6FCEEF4DDEC6D20DA4AE9673F2CF498E5F35FABD12393`.

On 15 October 2026 UTC, that exact executable ran on Windows with app settings directed to an isolated workspace folder. A self-authored `.wstrike` fixture (SHA-256 `D2C671F5D9B2BC1E6ECB502A6B1C9A4FD434441C7C45246A50DEB963BF7C16ED`) contained fictional field notes and a small original blue chart. The app's local control interface performed these checks:

1. `file.importLegacy` opened seven blocks and one image as an unsaved document requiring Save As. Inspection found the expected title and text.
2. `file.saveAs` wrote a 7,721-byte DOCX (SHA-256 `F6E7119A67183C7ACA42D823CDB97A965FEA0F88591EE49AE789D71D5B7F06EA`). The original `.wstrike` bytes remained unchanged; the document became clean.
3. The editor quit cleanly, reopened that DOCX, reported seven blocks with the expected text and a clean state, then quit cleanly again.
4. WordStrike's own `ui.screenshot` captured the rendered 1440 × 900 window after reopen. The [archived native screenshot](../assets/screenshots/wordstrike-native-current.png) has SHA-256 `E83AA124D968BCE1ECB592ACB725894241F4A866499C723AE496CFFD194A06AD`. Visual inspection found a rendered ribbon, page, title, heading, text, and blue chart. No personal document or client asset is shown.

This is a small synthetic Windows import/save/reopen and rendering check. It does not establish interactive editing, full DOCX fidelity, accessibility, other display configurations, or release readiness. The artifact is a short-lived CI debug build, not an installer or release.
