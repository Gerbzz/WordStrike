use legacy_wordstrike::{Block, Document as LegacyDocument, Editor, PageSetup, TextRun, save_document};
use serde_json::json;
use sha2::{Digest, Sha256};
use wordcraft_doc::{Block as NativeBlock, StoryRef};
use wordcraft_engine::{Session, file_access, io};

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
    assert!(session.dirty, "a converted document must prompt before closing");
    assert_eq!(session.doc.body.len(), 3);
    assert_eq!(session.doc.last_section.page_w, 612.0);
    assert_eq!(session.doc.last_section.margin_left, 60.0);
    assert_eq!(session.doc.plain_text(StoryRef::Body).contains("Revenue rose"), true);
    assert!(matches!(&*session.doc.body[0], NativeBlock::Para(p) if p.props.style.as_deref() == Some("Title")));
    assert_eq!(session.doc.media.len(), 1);
    assert_eq!(session.doc.media.values().next().unwrap().as_slice(), png.as_slice());

    session.run("file.saveAs", &json!({"path": target})).unwrap();
    assert!(!session.dirty, "a successful native Save As clears the prompt");
    let reopened = io::open_path(&target).unwrap();
    assert!(reopened.plain_text(StoryRef::Body).contains("Revenue rose"));
    assert_eq!(reopened.media.values().next().unwrap().as_slice(), png.as_slice());
    assert_eq!(std::fs::read(&source).unwrap(), original);
    let _ = std::fs::remove_file(source);
    let _ = std::fs::remove_file(target);
}

/// A root-confined automation may import only an archive inside its read root.
/// Images in a .wstrike document are ZIP members, never external file paths.
#[test]
fn legacy_import_honors_read_root_and_preserves_active_document_on_denial() {
    let base = scratch("read-roots");
    let allowed = base.join("allowed");
    let outside = base.join("outside");
    std::fs::create_dir_all(&allowed).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    let make = |path: &std::path::Path| {
        let mut doc = LegacyDocument::default();
        doc.blocks.push(Block::Title { text: "Imported note".into() });
        let mut editor = Editor::from_document(doc).unwrap();
        save_document(&mut editor, path).unwrap();
    };
    let accepted = allowed.join("accepted.wstrike");
    let denied = outside.join("denied.wstrike");
    make(&accepted);
    make(&denied);
    let mut session = Session::new(wordcraft_doc::Document::from_text("Keep this work"));
    let original_id = session.document_id();
    let roots = file_access::AutomationRoots::new(Some(&allowed), None).unwrap().unwrap();
    let error = file_access::confine(Some(&roots), || session.run("file.importLegacy", &json!({"path": denied}))).unwrap_err();
    assert!(error.to_string().contains("outside the read root"), "{error}");
    assert_eq!(session.document_id(), original_id);
    assert_eq!(session.doc.plain_text(StoryRef::Body), "Keep this work");
    let write_only = file_access::AutomationRoots::new(None, Some(&allowed)).unwrap().unwrap();
    let error = file_access::confine(Some(&write_only), || session.run("file.importLegacy", &json!({"path": accepted}))).unwrap_err();
    assert!(error.to_string().contains("read authority is absent"), "{error}");
    assert_eq!(session.document_id(), original_id);
    file_access::confine(Some(&roots), || session.run("file.importLegacy", &json!({"path": accepted}))).unwrap();
    assert_eq!(session.doc.plain_text(StoryRef::Body), "Imported note");
    assert!(session.dirty);
    let _ = std::fs::remove_dir_all(base);
}

#[test]
fn legacy_import_rejects_unsupported_and_truncated_image_payloads() {
    let gif = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff\x21\xf9\x04\x01\x00\x00\x01\x00\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02\x44\x01\x00\x3b";
    let truncated_png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15\xc4\x89";
    for (name, bytes) in [("gif", gif.as_slice()), ("truncated", truncated_png.as_slice())] {
        let source = scratch(name).with_extension("wstrike");
        let hash = format!("{:x}", Sha256::digest(bytes));
        let mut doc = LegacyDocument::default();
        doc.assets.insert(hash.clone(), bytes.to_vec());
        doc.blocks.push(Block::Image { hash, alt: "Preview".into() });
        let mut editor = Editor::from_document(doc).unwrap();
        save_document(&mut editor, &source).unwrap();
        let original = std::fs::read(&source).unwrap();
        let mut session = Session::new(wordcraft_doc::Document::from_text("Keep this work"));
        assert!(session.run("file.importLegacy", &json!({"path": source})).is_err(), "{name}");
        assert_eq!(session.doc.plain_text(StoryRef::Body), "Keep this work", "{name}");
        assert_eq!(std::fs::read(&source).unwrap(), original, "{name}");
        let _ = std::fs::remove_file(source);
    }
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
