use legacy_wordstrike::{Block, Document as LegacyDocument, Editor, PageSetup, TextRun, save_document};
use serde_json::json;
use wordcraft_doc::{Block as NativeBlock, StoryRef};
use wordcraft_engine::{Session, io};

fn scratch(name: &str) -> std::path::PathBuf {
    let id = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("wordstrike-legacy-{name}-{}-{id}", std::process::id()))
}

#[test]
fn rich_blocks_page_setup_and_image_import_without_changing_source() {
    let picture = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../../../fixtures/wordstrike/blue-chart.png");
    let png = std::fs::read(picture).unwrap();
    let source = scratch("source").with_extension("wstrike");
    let target = scratch("converted").with_extension("docx");
    let mut old = LegacyDocument { page_setup: PageSetup { width_pt: 612.0, height_pt: 792.0, margin_pt: 60.0 }, ..Default::default() };
    old.blocks.push(Block::Title { text: "Review".into() });
    old.blocks.push(Block::Paragraph {
        runs: vec![TextRun { text: "Revenue ".into(), bold: false, italic: false }, TextRun { text: "rose".into(), bold: true, italic: false }],
    });
    let mut editor = Editor::from_document(old).unwrap();
    editor.insert_image(2, &png, "Three blue columns").unwrap();
    save_document(&mut editor, &source).unwrap();
    let original = std::fs::read(&source).unwrap();

    let mut session = Session::new(wordcraft_doc::Document::from_text("Keep this until import succeeds."));
    let report = session.run("file.importLegacy", &json!({"path": source})).unwrap();
    assert_eq!(report["saveAsRequired"], true);
    assert!(session.path.is_none());
    assert_eq!(session.doc.body.len(), 3);
    assert_eq!(session.doc.last_section.page_w, 612.0);
    assert_eq!(session.doc.last_section.margin_left, 60.0);
    assert_eq!(session.doc.plain_text(StoryRef::Body).contains("Revenue rose"), true);
    assert!(matches!(&*session.doc.body[0], NativeBlock::Para(p) if p.props.style.as_deref() == Some("Title")));
    assert_eq!(session.doc.media.len(), 1);
    assert_eq!(session.doc.media.values().next().unwrap().as_slice(), png.as_slice());

    session.run("file.saveAs", &json!({"path": target})).unwrap();
    let reopened = io::open_path(&target).unwrap();
    assert!(reopened.plain_text(StoryRef::Body).contains("Revenue rose"));
    assert_eq!(reopened.media.values().next().unwrap().as_slice(), png.as_slice());
    assert_eq!(std::fs::read(&source).unwrap(), original);
    let _ = std::fs::remove_file(source);
    let _ = std::fs::remove_file(target);
}

#[test]
fn unsupported_list_and_malformed_source_keep_current_document() {
    let source = scratch("list").with_extension("wstrike");
    let mut old = LegacyDocument::default();
    old.blocks.push(Block::ListItem { ordered: true, runs: vec![TextRun { text: "One".into(), bold: false, italic: false }] });
    let mut editor = Editor::from_document(old).unwrap();
    save_document(&mut editor, &source).unwrap();
    let original = std::fs::read(&source).unwrap();
    let mut session = Session::new(wordcraft_doc::Document::from_text("Keep this"));
    assert!(session.run("file.importLegacy", &json!({"path": source})).is_err());
    assert_eq!(session.doc.plain_text(StoryRef::Body), "Keep this");
    assert_eq!(std::fs::read(&source).unwrap(), original);

    let malformed = scratch("malformed").with_extension("wstrike");
    std::fs::write(&malformed, b"not a document").unwrap();
    assert!(session.run("file.importLegacy", &json!({"path": malformed})).is_err());
    assert_eq!(session.doc.plain_text(StoryRef::Body), "Keep this");
    let _ = std::fs::remove_file(source);
    let _ = std::fs::remove_file(malformed);
}
