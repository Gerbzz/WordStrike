#![cfg(feature = "donor-export")]

use wordstrike::{Block, Document, Editor, export_pdf_bytes, open_document, save_document};

#[test]
fn simple_pages_export_as_real_pdf_with_explicit_break_and_image() {
    let mut doc = Document::default();
    doc.blocks.push(Block::Title {
        text: "Quarterly Field Notes".into(),
    });
    doc.blocks.push(Block::Heading {
        level: 1,
        text: "Findings".into(),
    });
    doc.blocks.push(Block::plain("Alpha beta — café"));
    doc.blocks.push(Block::PageBreak);
    doc.blocks.push(Block::Heading {
        level: 2,
        text: "Next steps".into(),
    });
    doc.blocks.push(Block::plain("Publish locally."));
    let mut editor = Editor::from_document(doc).unwrap();
    editor
        .insert_image(
            3,
            include_bytes!("../../../fixtures/wordstrike/blue-chart.png"),
            "Blue bar chart",
        )
        .unwrap();
    let doc = editor.document().clone();
    let output = export_pdf_bytes(&doc).expect("donor PDF export");
    assert_eq!(output.pages, 2);
    assert!(output.bytes.starts_with(b"%PDF-"));
    assert_eq!(
        hayro_syntax::Pdf::new(output.bytes.clone())
            .unwrap()
            .pages()
            .len(),
        2
    );
    assert!(
        output
            .warnings
            .iter()
            .any(|warning| warning.contains("Pagination"))
    );
    if let Some(directory) = std::env::var_os("STRIKE_WORD_ACCEPTANCE_DIR") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        let native = directory.join("report.wstrike");
        save_document(&mut editor, &native).unwrap();
        let reopened = open_document(&native).unwrap();
        assert_eq!(reopened.document(), &doc);
        std::fs::write(directory.join("report.pdf"), &output.bytes).unwrap();
        std::fs::write(
            directory.join("report-loss.txt"),
            output.warnings.join("\n"),
        )
        .unwrap();
    }
}

#[test]
fn unsupported_inline_style_is_reported_and_source_runs_survive() {
    let preview = wordstrike::preview_docx(include_bytes!(
        "../../../fixtures/wordstrike/with-image.docx"
    ))
    .unwrap();
    assert!(preview.applicable);
    let doc = preview.candidate;
    let source = doc.clone();
    let output = export_pdf_bytes(&doc).expect("donor PDF with embedded image");
    assert!(output.bytes.starts_with(b"%PDF-"));
    assert_eq!(doc, source);
    assert!(
        output
            .warnings
            .iter()
            .any(|warning| warning.contains("bold/italic"))
    );
    if let Some(directory) = std::env::var_os("STRIKE_WORD_ACCEPTANCE_DIR") {
        let directory = std::path::PathBuf::from(directory);
        std::fs::create_dir_all(&directory).unwrap();
        std::fs::write(directory.join("docx-import.pdf"), &output.bytes).unwrap();
    }
}
