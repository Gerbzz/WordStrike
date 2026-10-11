use std::io::{Cursor, Read, Write};
use wordstrike::{Block, Document, preview_docx};
use zip::{ZipArchive, ZipWriter, write::SimpleFileOptions};

fn rewrite_document_xml(change: impl FnOnce(String) -> String) -> Vec<u8> {
    let original = include_bytes!("../../../fixtures/wordstrike/with-image.docx");
    let mut input = ZipArchive::new(Cursor::new(original.as_slice())).unwrap();
    let mut output = ZipWriter::new(Cursor::new(Vec::new()));
    let mut change = Some(change);
    for index in 0..input.len() {
        let mut entry = input.by_index(index).unwrap();
        let name = entry.name().unwrap().to_string();
        let mut bytes = Vec::new();
        entry.read_to_end(&mut bytes).unwrap();
        if name == "word/document.xml" {
            bytes = change.take().unwrap()(String::from_utf8(bytes).unwrap()).into_bytes();
        }
        output
            .start_file(name, SimpleFileOptions::default())
            .unwrap();
        output.write_all(&bytes).unwrap();
    }
    output.finish().unwrap().into_inner()
}

#[test]
fn real_docx_preserves_embedded_png_by_relationship() {
    let bytes = include_bytes!("../../../fixtures/wordstrike/with-image.docx");
    let image = include_bytes!("../../../fixtures/wordstrike/blue-chart.png");
    let preview = preview_docx(bytes).unwrap();
    assert!(preview.applicable, "{:?}", preview.issues);
    assert!(
        preview.issues.iter().all(|issue| !issue.blocking),
        "{:?}",
        preview.issues
    );
    assert!(
        preview.candidate.blocks.iter().any(|block| {
            matches!(block, Block::Heading { level: 1, text } if text == "Findings")
        })
    );
    let asset = preview
        .candidate
        .blocks
        .iter()
        .find_map(|block| match block {
            Block::Image { hash, alt } if alt == "Blue bar chart" => Some(hash.clone()),
            _ => None,
        })
        .expect("PNG image with alt text");
    assert_eq!(preview.candidate.assets.get(&asset).unwrap(), image);
    let mut document = Document::default();
    document.apply_import(preview).unwrap();
    assert_eq!(document.assets.get(&asset).unwrap(), image);
}

#[test]
fn tracked_changes_require_resolution_before_import_mutates_source() {
    let bytes = include_bytes!("../../../fixtures/wordstrike/tracked-change.docx");
    let preview = preview_docx(bytes).unwrap();
    assert!(!preview.applicable);
    assert!(
        preview
            .issues
            .iter()
            .any(|issue| issue.kind == "tracked-changes" && issue.blocking)
    );
    let mut document = Document::default();
    document.blocks.push(Block::plain("Keep original"));
    assert!(document.apply_import(preview).is_err());
    assert_eq!(document.blocks, vec![Block::plain("Keep original")]);
}

#[test]
fn unresolved_image_relationship_is_a_blocking_preview_issue() {
    let bytes = include_bytes!("../../../fixtures/wordstrike/missing-image.docx");
    let preview = preview_docx(bytes).unwrap();
    assert!(!preview.applicable);
    assert!(
        preview
            .issues
            .iter()
            .any(|issue| issue.kind == "image" && issue.blocking)
    );
}

#[test]
fn mixed_text_and_drawing_run_is_blocked_without_replacing_document() {
    let bytes = rewrite_document_xml(|xml| {
        xml.replace(
            "<w:r><w:drawing>",
            "<w:r><w:t>Text before image</w:t><w:drawing>",
        )
    });
    let preview = preview_docx(&bytes).unwrap();
    assert!(!preview.applicable);
    assert!(
        preview
            .issues
            .iter()
            .any(|issue| issue.kind == "mixed-paragraph" && issue.blocking)
    );
    let mut document = Document::default();
    document.blocks.push(Block::plain("Keep original"));
    assert!(document.apply_import(preview).is_err());
    assert_eq!(document.blocks, vec![Block::plain("Keep original")]);
}

#[test]
fn legacy_picture_and_unmapped_drawing_are_blocked() {
    let original = rewrite_document_xml(|xml| {
        let start = xml.find("<w:body>").unwrap() + "<w:body>".len();
        let end = xml.find("</w:body>").unwrap();
        format!(
            "{}<w:p><w:r><w:pict><w:shape/></w:pict></w:r></w:p>{}",
            &xml[..start],
            &xml[end..]
        )
    });
    let preview = preview_docx(&original).unwrap();
    assert!(!preview.applicable);
    assert!(
        preview
            .issues
            .iter()
            .any(|issue| issue.kind == "legacy-image" && issue.blocking)
    );
    let drawing = rewrite_document_xml(|xml| {
        let start = xml.find("<w:body>").unwrap() + "<w:body>".len();
        let end = xml.find("</w:body>").unwrap();
        format!(
            "{}<w:p><w:r><w:drawing><w:shape/></w:drawing></w:r></w:p>{}",
            &xml[..start],
            &xml[end..]
        )
    });
    let preview = preview_docx(&drawing).unwrap();
    assert!(!preview.applicable);
    assert!(
        preview
            .issues
            .iter()
            .any(|issue| issue.kind == "drawing" && issue.blocking)
    );
}

#[test]
fn valid_image_plus_unsupported_second_drawing_is_blocked() {
    let bytes = rewrite_document_xml(|xml| {
        assert!(xml.contains("</w:drawing></w:r></w:p>"));
        xml.replace(
            "</w:drawing></w:r></w:p>",
            "</w:drawing></w:r><w:r><w:drawing><w:shape/></w:drawing></w:r></w:p>",
        )
    });
    let preview = preview_docx(&bytes).unwrap();
    assert!(!preview.applicable);
    assert!(preview.issues.iter().any(|issue| {
        issue.kind == "drawing" && issue.blocking && issue.detail.contains("Drawing 2")
    }));
    let mut document = Document::default();
    document.blocks.push(Block::plain("Keep original"));
    assert!(document.apply_import(preview).is_err());
    assert_eq!(document.blocks, vec![Block::plain("Keep original")]);
}

#[test]
fn tracked_run_and_paragraph_format_revisions_are_blocked() {
    for bytes in [
        rewrite_document_xml(|xml| {
            xml.replace(
                "<w:rPr><w:b/></w:rPr>",
                "<w:rPr><w:b/><w:rPrChange w:id=\"1\"/></w:rPr>",
            )
        }),
        rewrite_document_xml(|xml| {
            xml.replace(
                "<w:pStyle w:val=\"Title\"/>",
                "<w:pStyle w:val=\"Title\"/><w:pPrChange w:id=\"2\"/>",
            )
        }),
    ] {
        let preview = preview_docx(&bytes).unwrap();
        assert!(!preview.applicable);
        assert!(
            preview
                .issues
                .iter()
                .any(|issue| issue.kind == "tracked-changes" && issue.blocking)
        );
    }
}
