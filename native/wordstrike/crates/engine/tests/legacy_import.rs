use legacy_wordstrike::{Block, Document as LegacyDocument, Editor, PageSetup, TextRun, save_document};
use serde_json::json;
use sha2::{Digest, Sha256};
use wordcraft_doc::numbering::Counters;
use wordcraft_doc::section::NumFormat;
use wordcraft_doc::{Block as NativeBlock, StoryRef};
use wordcraft_engine::{Session, file_access, io};

fn scratch(name: &str) -> std::path::PathBuf {
    let id = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    std::env::temp_dir().join(format!("wordstrike-legacy-{name}-{}-{id}", std::process::id()))
}

fn asset_hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes).iter().map(|byte| format!("{byte:02x}")).collect()
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
    let alias = allowed.join("outside-link.wstrike");
    #[cfg(windows)]
    let linked = std::os::windows::fs::symlink_file(&denied, &alias).is_ok();
    #[cfg(unix)]
    let linked = std::os::unix::fs::symlink(&denied, &alias).is_ok();
    #[cfg(not(any(windows, unix)))]
    let linked = false;
    if linked {
        let error = file_access::confine(Some(&roots), || session.run("file.importLegacy", &json!({"path": alias}))).unwrap_err();
        assert!(error.to_string().contains("outside the read root"), "{error}");
        assert_eq!(session.document_id(), original_id);
    }
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
fn legacy_import_enforces_image_dimension_boundary_before_decode() {
    for (width, accepted) in [(8192, true), (8193, false)] {
        let source = scratch(&format!("image-width-{width}")).with_extension("wstrike");
        let pixels = image::RgbaImage::from_pixel(width, 1, image::Rgba([0, 90, 190, 255]));
        let mut png = Vec::new();
        image::DynamicImage::ImageRgba8(pixels).write_to(&mut std::io::Cursor::new(&mut png), image::ImageFormat::Png).unwrap();
        let hash = asset_hash(&png);
        let mut doc = LegacyDocument::default();
        doc.assets.insert(hash.clone(), png);
        doc.blocks.push(Block::Image { hash, alt: "Wide chart".into() });
        let mut editor = Editor::from_document(doc).unwrap();
        save_document(&mut editor, &source).unwrap();
        let mut session = Session::new(wordcraft_doc::Document::from_text("Keep this work"));
        let result = session.run("file.importLegacy", &json!({"path": source}));
        if accepted {
            result.unwrap();
            assert_eq!(session.doc.media.len(), 1);
            assert!(session.dirty);
        } else {
            let error = result.unwrap_err();
            assert!(error.to_string().contains("image dimensions exceed"), "{error}");
            assert_eq!(session.doc.plain_text(StoryRef::Body), "Keep this work");
        }
        let _ = std::fs::remove_file(source);
    }
}

#[test]
fn legacy_import_rejects_unsupported_and_truncated_image_payloads() {
    let gif = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff\x21\xf9\x04\x01\x00\x00\x01\x00\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02\x44\x01\x00\x3b";
    let truncated_png = b"\x89PNG\r\n\x1a\n\x00\x00\x00\x0dIHDR\x00\x00\x00\x01\x00\x00\x00\x01\x08\x06\x00\x00\x00\x1f\x15\xc4\x89";
    for (name, bytes) in [("gif", gif.as_slice()), ("truncated", truncated_png.as_slice())] {
        let source = scratch(name).with_extension("wstrike");
        let hash = asset_hash(bytes);
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

fn assert_mixed_lists(doc: &wordcraft_doc::Document) {
    let paragraphs: Vec<_> = doc
        .body
        .iter()
        .map(|block| match &**block {
            NativeBlock::Para(p) => p,
            _ => panic!("expected a paragraph"),
        })
        .collect();
    assert_eq!(paragraphs.len(), 8);
    assert_eq!(
        paragraphs
            .iter()
            .map(|p| p.text.as_str())
            .collect::<Vec<_>>(),
        [
            "Before", "One bold", "Two", "Between", "Again", "Red", "Blue\tCaf\u{E9} \u{1F31F}", "After"
        ]
    );
    for index in [0, 3, 7] {
        assert!(
            paragraphs[index].props.numbering.is_none(),
            "paragraph {index} became a list item"
        );
    }
    let num = |index: usize| {
        paragraphs[index]
            .props
            .numbering
            .expect("list item has numbering")
            .num
    };
    assert_eq!(
        num(1),
        num(2),
        "adjacent ordered items must share a counter"
    );
    assert_ne!(num(2), num(4), "a paragraph must restart an ordered list");
    assert_eq!(num(5), num(6), "adjacent bullets must share a list");
    assert_ne!(num(4), num(5), "changing list kind must start a new list");
    for index in [1, 2, 4, 5, 6] {
        assert_eq!(paragraphs[index].props.numbering.unwrap().level, 0);
    }
    assert_eq!(
        doc.numbering.level(num(1), 0).unwrap().format,
        NumFormat::Decimal
    );
    assert_eq!(
        doc.numbering.level(num(5), 0).unwrap().format,
        NumFormat::Bullet
    );
    let mut counters = Counters::default();
    for (index, expected) in [(1, "1."), (2, "2."), (4, "1.")] {
        assert_eq!(
            counters
                .next_label(&doc.numbering, num(index), 0)
                .unwrap()
                .0,
            expected
        );
    }
    assert_eq!(paragraphs[1].runs.len(), 2);
    assert_eq!(paragraphs[1].runs[0].len, 4);
    assert_ne!(paragraphs[1].runs[0].props.bold, Some(true));
    assert_eq!(paragraphs[1].runs[1].len, 4);
    assert_eq!(paragraphs[1].runs[1].props.bold, Some(true));
    assert_eq!(paragraphs[2].runs[0].props.italic, Some(true));
    assert_eq!(paragraphs[5].runs[0].props.bold, Some(true));
}

#[test]
fn mixed_legacy_lists_preserve_order_styles_and_paragraphs_through_docx() {
    let source = scratch("mixed-lists").with_extension("wstrike");
    let target = scratch("mixed-lists-native").with_extension("docx");
    let mut old = LegacyDocument::default();
    let run = |text: &str, bold, italic| TextRun {
        text: text.into(),
        bold,
        italic,
    };
    old.blocks.push(Block::plain("Before"));
    old.blocks.push(Block::ListItem {
        ordered: true,
        runs: vec![run("One ", false, false), run("bold", true, false)],
    });
    old.blocks.push(Block::ListItem {
        ordered: true,
        runs: vec![run("Two", false, true)],
    });
    old.blocks.push(Block::plain("Between"));
    old.blocks.push(Block::ListItem {
        ordered: true,
        runs: vec![run("Again", false, false)],
    });
    old.blocks.push(Block::ListItem {
        ordered: false,
        runs: vec![run("Red", true, false)],
    });
    old.blocks.push(Block::ListItem {
        ordered: false,
        runs: vec![run("Blue\tCaf\u{E9} \u{1F31F}", false, false)],
    });
    old.blocks.push(Block::plain("After"));
    let mut editor = Editor::from_document(old).unwrap();
    save_document(&mut editor, &source).unwrap();
    let original = std::fs::read(&source).unwrap();
    let mut session = Session::new(wordcraft_doc::Document::from_text("Keep this"));
    session
        .run("file.importLegacy", &json!({"path": source}))
        .unwrap();
    assert_mixed_lists(&session.doc);
    assert!(session.path.is_none());
    assert!(session.dirty);
    session
        .run("file.saveAs", &json!({"path": target}))
        .unwrap();
    assert_mixed_lists(&io::open_path(&target).unwrap());
    assert_eq!(std::fs::read(&source).unwrap(), original);
    let _ = std::fs::remove_file(source);
    let _ = std::fs::remove_file(target);
}

#[test]
fn unsupported_list_structure_and_malformed_source_keep_current_document() {
    let mut session = Session::new(wordcraft_doc::Document::from_text("Keep this"));
    let original_id = session.document_id();
    let active_path = scratch("active-document").with_extension("docx");
    session.path = Some(active_path.clone());
    session.dirty = true;
    for (name, runs, reason) in [
        ("empty-list", vec![], "empty list item"),
        (
            "object-marker",
            vec![TextRun {
                text: "bad \u{FFFC}".into(),
                bold: false,
                italic: false,
            }],
            "unsupported object or break",
        ),
        (
            "embedded-break",
            vec![TextRun {
                text: "bad\nitem".into(),
                bold: false,
                italic: false,
            }],
            "unsupported object or break",
        ),
        (
            "control-0001",
            vec![TextRun { text: "bad\u{0001}item".into(), bold: false, italic: false }],
            "XML-invalid character U+0001",
        ),
        (
            "control-000b",
            vec![TextRun { text: "bad\u{000B}item".into(), bold: false, italic: false }],
            "XML-invalid character U+000B",
        ),
    ] {
        let source = scratch(name).with_extension("wstrike");
        let mut old = LegacyDocument::default();
        old.blocks.push(Block::plain("Already converted before invalid item"));
        old.blocks.push(Block::ListItem {
            ordered: true,
            runs,
        });
        let mut editor = Editor::from_document(old).unwrap();
        save_document(&mut editor, &source).unwrap();
        let original = std::fs::read(&source).unwrap();
        let error = session
            .run("file.importLegacy", &json!({"path": source}))
            .unwrap_err();
        assert!(error.to_string().contains("block 1:"), "{error}");
        assert!(error.to_string().contains(reason), "{error}");
        assert_eq!(session.document_id(), original_id);
        assert_eq!(session.doc.plain_text(StoryRef::Body), "Keep this");
        assert_eq!(session.path.as_ref(), Some(&active_path));
        assert!(session.dirty);
        assert_eq!(std::fs::read(&source).unwrap(), original);
        let _ = std::fs::remove_file(source);
    }

    let malformed = scratch("malformed").with_extension("wstrike");
    std::fs::write(&malformed, b"not a document").unwrap();
    let error = session
        .run("file.importLegacy", &json!({"path": malformed}))
        .unwrap_err();
    assert!(!error.to_string().is_empty());
    assert_eq!(session.document_id(), original_id);
    assert_eq!(session.doc.plain_text(StoryRef::Body), "Keep this");
    assert_eq!(session.path.as_ref(), Some(&active_path));
    assert!(session.dirty);
    let _ = std::fs::remove_file(malformed);
}
