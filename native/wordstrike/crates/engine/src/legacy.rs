//! Loss-aware conversion of the original `.wstrike` archive into a WordCraft document.

use std::path::Path;

use wordcraft_doc::numbering::ListKind;
use wordcraft_doc::para::{COLUMN_BREAK, Float, OBJ, PAGE_BREAK, Run};
use wordcraft_doc::props::NumRef;
use wordcraft_doc::{CharProps, Document, InlineObject, Paragraph, para_block};

pub fn convert_path(path: &Path) -> Result<Document, String> {
    // This reader checks ZIP paths, entry sizes, schema, image hashes and old history.
    let editor = legacy_wordstrike::open_document(path)?;
    let source = editor.document();
    let mut doc = Document::new();
    let setup = source.page_setup;
    doc.last_section.page_w = setup.width_pt as f32;
    doc.last_section.page_h = setup.height_pt as f32;
    doc.last_section.landscape = setup.width_pt > setup.height_pt;
    let margin = setup.margin_pt as f32;
    doc.last_section.margin_left = margin;
    doc.last_section.margin_right = margin;
    doc.last_section.margin_top = margin;
    doc.last_section.margin_bottom = margin;
    doc.body.clear();
    // The old format has no list id: only adjacent items of the same kind belong to one list.
    let mut current_list: Option<(bool, u32)> = None;
    for (index, block) in source.blocks.iter().enumerate() {
        if !matches!(block, legacy_wordstrike::Block::ListItem { .. }) {
            current_list = None;
        }
        let paragraph = match block {
            legacy_wordstrike::Block::Title { text } if !text.contains(OBJ) => Paragraph::with_text(text, CharProps::default()).styled("Title"),
            legacy_wordstrike::Block::Heading { level, text } if (1..=2).contains(level) => {
                if text.contains(OBJ) {
                    return Err(format!("block {index}: object marker in heading has no source object"));
                }
                Paragraph::with_text(text, CharProps::default()).styled(&format!("Heading{level}"))
            }
            legacy_wordstrike::Block::Title { .. } => {
                return Err(format!("block {index}: object marker in title has no source object"));
            }
            legacy_wordstrike::Block::Heading { .. } => {
                return Err(format!("block {index}: unsupported heading level"));
            }
            legacy_wordstrike::Block::Paragraph { runs } => {
                let mut paragraph = Paragraph::new();
                for run in runs {
                    if run.text.contains(OBJ) {
                        return Err(format!("block {index}: object marker in text has no source object"));
                    }
                    let props = CharProps { bold: Some(run.bold), italic: Some(run.italic), ..CharProps::default() };
                    paragraph.runs.push(Run { len: run.text.len(), props });
                    paragraph.text.push_str(&run.text);
                }
                paragraph
            }
            legacy_wordstrike::Block::ListItem { ordered, runs } => {
                if runs.is_empty() || runs.iter().all(|run| run.text.is_empty()) {
                    return Err(format!("block {index}: empty list item is unsupported"));
                }
                let mut paragraph = Paragraph::new();
                for run in runs {
                    if run
                        .text
                        .chars()
                        .any(|c| matches!(c, OBJ | PAGE_BREAK | COLUMN_BREAK | '\n' | '\r'))
                    {
                        return Err(format!(
                            "block {index}: list item contains an unsupported object or break"
                        ));
                    }
                    let props = CharProps {
                        bold: Some(run.bold),
                        italic: Some(run.italic),
                        ..CharProps::default()
                    };
                    paragraph.runs.push(Run {
                        len: run.text.len(),
                        props,
                    });
                    paragraph.text.push_str(&run.text);
                }
                let num = match current_list {
                    Some((was_ordered, num)) if was_ordered == *ordered => num,
                    _ => {
                        let kind = if *ordered {
                            ListKind::Numbered
                        } else {
                            ListKind::Bullet
                        };
                        let num = doc.numbering.add_list(kind);
                        current_list = Some((*ordered, num));
                        num
                    }
                };
                paragraph.props.numbering = Some(NumRef { num, level: 0 });
                paragraph.props.style = Some("ListParagraph".into());
                paragraph
            }
            legacy_wordstrike::Block::PageBreak => Paragraph::with_text(&PAGE_BREAK.to_string(), CharProps::default()),
            legacy_wordstrike::Block::Image { hash, alt } => {
                let bytes = source.assets.get(hash).ok_or_else(|| format!("block {index}: image bytes are missing"))?;
                if bytes.len() > 10 * 1024 * 1024 {
                    return Err(format!("block {index}: image exceeds 10 MiB"));
                }
                let format = image::guess_format(bytes).map_err(|e| e.to_string())?;
                let ext = match format {
                    image::ImageFormat::Png => "png",
                    image::ImageFormat::Jpeg => "jpeg",
                    _ => return Err(format!("block {index}: only PNG and JPEG images can be imported")),
                };
                let reader = image::ImageReader::with_format(std::io::Cursor::new(bytes), format);
                let (width, height) = reader.into_dimensions().map_err(|e| e.to_string())?;
                if width == 0 || height == 0 || width > 8192 || height > 8192 || u64::from(width) * u64::from(height) > 20_000_000 {
                    return Err(format!("block {index}: image dimensions exceed the import limit"));
                }
                image::load_from_memory_with_format(bytes, format).map_err(|e| format!("block {index}: image is incomplete or invalid: {e}"))?;
                let w = (setup.width_pt - 2.0 * setup.margin_pt).min(420.0) as f32;
                let h = w * height as f32 / width as f32;
                let media = doc.add_media(bytes.clone(), ext);
                let mut paragraph = Paragraph::new();
                paragraph.text.push(OBJ);
                paragraph.runs.push(Run { len: OBJ.len_utf8(), props: CharProps::default() });
                paragraph.objects.push(InlineObject::Image { media, w, h, alt: alt.clone(), float: Float::default(), crop: [0.0; 4], ole: None });
                paragraph
            }
        };
        doc.body.push(para_block(paragraph));
    }
    if doc.media.len() != source.assets.len() {
        return Err("legacy document has unattached image assets; import would discard them".into());
    }
    doc.ensure_nonempty();
    Ok(doc)
}
