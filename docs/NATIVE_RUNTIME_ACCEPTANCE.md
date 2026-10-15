# Native editor runtime check

The [draft WordStrike foundation PR](https://github.com/Gerbzz/WordStrike/pull/1) was built by [Windows Actions run 38668293554](https://github.com/Gerbzz/WordStrike/actions/runs/38668293554). That run compiled the `wordstrike` desktop target, passed its focused legacy import and safe-save engine tests, passed its import UI and Save Changes guard tests, and retained a one-day executable artifact for local acceptance. The source head was `03ea913de5cada3b391acfc251f762158e53df28`; the tested PR merge build was `637db096b2b1d4a70082434fac01b6d988ea9325`. The 69,568,000-byte debug executable reported `wordstrike 0.6.1 (dev)` and SHA-256 `D84630DCCF0618FEB9D6FCEEF4DDEC6D20DA4AE9673F2CF498E5F35FABD12393`.

On 15 October 2026 UTC, that exact executable ran on Windows with app settings directed to an isolated workspace folder. A self-authored `.wstrike` fixture (SHA-256 `D2C671F5D9B2BC1E6ECB502A6B1C9A4FD434441C7C45246A50DEB963BF7C16ED`) contained fictional field notes and a small original blue chart. The app's local control interface performed these checks:

1. `file.importLegacy` opened seven blocks and one image as an unsaved document requiring Save As. Inspection found the expected title and text.
2. `file.saveAs` wrote a 7,721-byte DOCX (SHA-256 `F6E7119A67183C7ACA42D823CDB97A965FEA0F88591EE49AE789D71D5B7F06EA`). The original `.wstrike` bytes remained unchanged; the document became clean.
3. The editor quit cleanly, reopened that DOCX, reported seven blocks with the expected text and a clean state, then quit cleanly again.
4. WordStrike's own `ui.screenshot` captured the rendered 1440 × 900 window after reopen. The [current native screenshot](../assets/screenshots/wordstrike-native-current.png) has SHA-256 `E83AA124D968BCE1ECB592ACB725894241F4A866499C723AE496CFFD194A06AD`. Visual inspection found a rendered ribbon, page, title, heading, text, and blue chart. No personal document or client asset is shown.

This is a small synthetic Windows import/save/reopen and rendering check. It does not establish interactive editing, full DOCX fidelity, accessibility, other display configurations, or release readiness. The artifact is a short-lived CI debug build, not an installer or release.
