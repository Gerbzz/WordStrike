//! Windows only: embed version info (VERSIONINFO) into `wordstrike.exe`, so it
//! shows in Explorer, the taskbar, the Start menu and Alt-Tab.
//!
//! On every other target this does nothing. A missing resource compiler is a warning, so a
//! cross-compile from macOS or Linux still links, unless `WORDCRAFT_REQUIRE_WINRES=1` turns it
//! into an error (for release builds).

fn main() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=../../assets/app-icon/wordstrike-geometric-256.png");
    println!("cargo:rerun-if-changed=../../assets/app-icon/wordstrike-geometric.ico");
    println!("cargo:rerun-if-env-changed=WORDCRAFT_REQUIRE_WINRES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let mut res = winresource::WindowsResource::new();
    res.set_icon("../../assets/app-icon/wordstrike-geometric.ico");
    res.set("ProductName", "WordStrike")
        .set("FileDescription", "WordStrike word processor")
        .set("CompanyName", "PWDS")
        .set("LegalCopyright", "Copyright (c) the WordCraft authors. MIT OR Apache-2.0; WordStrike adaptation by PWDS.")
        .set("OriginalFilename", "wordstrike.exe")
        .set("InternalName", "wordstrike");
    if let Err(e) = res.compile() {
        if std::env::var_os("WORDCRAFT_REQUIRE_WINRES").is_some() {
            eprintln!("embedding Windows resources failed: {e}");
            std::process::exit(1);
        }
        println!("cargo:warning=wordstrike.exe built without version resources: {e}");
    }
}
