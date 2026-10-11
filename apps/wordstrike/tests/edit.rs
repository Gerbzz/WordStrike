use std::fs;

use uuid::Uuid;
use wordstrike::{
    Block, Command, Document, Editor, ImportPreview, PageSetup, open_document, preview_docx,
    save_document,
};

#[test]
fn structured_edit_image_undo_and_native_reopen() {
    let mut editor = Editor::new();
    editor
        .apply(Command::Insert {
            at: 0,
            block: Block::Title {
                text: "Quarterly Field Notes".into(),
            },
        })
        .unwrap();
    editor
        .apply(Command::Insert {
            at: 1,
            block: Block::Heading {
                level: 1,
                text: "Findings".into(),
            },
        })
        .unwrap();
    editor
        .apply(Command::Insert {
            at: 2,
            block: Block::plain("Alpha beta — café"),
        })
        .unwrap();
    editor
        .insert_image(
            3,
            include_bytes!("../../../fixtures/wordstrike/blue-chart.png"),
            "Blue bar chart",
        )
        .unwrap();
    editor
        .apply(Command::Insert {
            at: 4,
            block: Block::PageBreak,
        })
        .unwrap();
    editor
        .apply(Command::Insert {
            at: 5,
            block: Block::Heading {
                level: 2,
                text: "Next steps".into(),
            },
        })
        .unwrap();
    editor
        .apply(Command::Insert {
            at: 6,
            block: Block::plain("Publish locally."),
        })
        .unwrap();
    let before = editor.document().blocks.clone();
    editor
        .apply(Command::Replace {
            at: 6,
            block: Block::plain("Revised locally."),
        })
        .unwrap();
    editor.undo().unwrap();
    assert_eq!(editor.document().blocks, before);
    editor.redo().unwrap();
    let root = std::env::temp_dir().join(format!("wordstrike-edit-{}", Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let path = root.join("report.wstrike");
    save_document(&mut editor, &path).unwrap();
    let mut reopened = open_document(&path).unwrap();
    assert_eq!(reopened.document().blocks, editor.document().blocks);
    assert_eq!(reopened.document().assets, editor.document().assets);
    reopened.undo().unwrap();
    assert_eq!(reopened.document().blocks, before);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn bad_page_setup_and_archive_leave_source_intact() {
    let mut editor = Editor::new();
    let before = editor.document().clone();
    assert!(
        editor
            .apply(Command::SetPageSetup(PageSetup {
                width_pt: f64::NAN,
                ..PageSetup::default()
            }))
            .is_err()
    );
    assert_eq!(editor.document(), &before);
    let mut raw = Document::default();
    let mut candidate = Document::default();
    candidate.page_setup.width_pt = f64::NAN;
    assert!(
        raw.apply_import(ImportPreview {
            candidate,
            issues: Vec::new(),
            applicable: true,
        })
        .is_err()
    );
    assert_eq!(raw, Document::default());
    let root = std::env::temp_dir().join(format!("wordstrike-bad-{}", Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let path = root.join("bad.wstrike");
    fs::write(&path, b"not a document").unwrap();
    assert!(open_document(&path).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"not a document");
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn supported_docx_import_keeps_original_png_in_editable_file() {
    let preview = preview_docx(include_bytes!(
        "../../../fixtures/wordstrike/with-image.docx"
    ))
    .unwrap();
    let mut editor = Editor::new();
    editor.apply_import(preview).unwrap();
    assert_eq!(
        editor.document().assets.values().next().unwrap(),
        include_bytes!("../../../fixtures/wordstrike/blue-chart.png")
    );
    let root = std::env::temp_dir().join(format!("wordstrike-docx-{}", Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let path = root.join("import.wstrike");
    save_document(&mut editor, &path).unwrap();
    let reopened = open_document(&path).unwrap();
    assert_eq!(reopened.document().assets, editor.document().assets);
    fs::remove_dir_all(root).unwrap();
}

#[test]
fn external_document_edit_blocks_stale_save_and_keeps_disk_version() {
    let root = std::env::temp_dir().join(format!("wordstrike-conflict-{}", Uuid::new_v4()));
    fs::create_dir(&root).unwrap();
    let path = root.join("report.wstrike");
    let mut first = Editor::new();
    first
        .apply(Command::Insert {
            at: 0,
            block: Block::plain("Original"),
        })
        .unwrap();
    save_document(&mut first, &path).unwrap();
    let mut other = open_document(&path).unwrap();
    other
        .apply(Command::Replace {
            at: 0,
            block: Block::plain("External edit"),
        })
        .unwrap();
    save_document(&mut other, &path).unwrap();
    let backup = path.with_extension("wstrike.previous");
    let first_backup = fs::read(&backup).unwrap();
    other
        .apply(Command::Replace {
            at: 0,
            block: Block::plain("External edit again"),
        })
        .unwrap();
    save_document(&mut other, &path).unwrap();
    assert_eq!(fs::read(&backup).unwrap(), first_backup);
    let external_bytes = fs::read(&path).unwrap();
    first
        .apply(Command::Replace {
            at: 0,
            block: Block::plain("Stale local edit"),
        })
        .unwrap();
    assert!(
        save_document(&mut first, &path)
            .unwrap_err()
            .contains("changed outside")
    );
    assert_eq!(fs::read(&path).unwrap(), external_bytes);
    assert_eq!(
        open_document(&path).unwrap().document().blocks,
        vec![Block::plain("External edit again")]
    );
    assert!(backup.exists());
}
