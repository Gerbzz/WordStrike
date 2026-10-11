use std::collections::{BTreeMap, BTreeSet};
use std::io::{Cursor, Read};

use quick_xml::Reader;
use quick_xml::events::Event;
use sha2::{Digest, Sha256};
use zip::ZipArchive;

use crate::{Block, Document, ImportPreview, TextRun};

const MAX_ARCHIVE: usize = 16 * 1024 * 1024;
const MAX_PART: u64 = 8 * 1024 * 1024;
const MAX_TOTAL_EXPANDED: u64 = 48 * 1024 * 1024;
const MAX_XML_NODES: usize = 200_000;
const MAX_XML_DEPTH: usize = 128;

#[derive(Debug)]
enum Node {
    Element(Element),
    Text(String),
}

#[derive(Debug)]
struct Element {
    name: String,
    attrs: BTreeMap<String, String>,
    children: Vec<Node>,
}

impl Element {
    fn attr(&self, key: &str) -> Option<&str> {
        self.attrs.get(key).map(String::as_str)
    }

    fn child(&self, key: &str) -> Option<&Element> {
        self.children.iter().find_map(|child| match child {
            Node::Element(element) if element.name == key => Some(element),
            _ => None,
        })
    }

    fn children_named<'a>(&'a self, key: &'a str) -> impl Iterator<Item = &'a Element> {
        self.children.iter().filter_map(move |child| match child {
            Node::Element(element) if element.name == key => Some(element),
            _ => None,
        })
    }

    fn descendants<'a>(&'a self, key: &str, found: &mut Vec<&'a Element>) {
        for child in &self.children {
            if let Node::Element(element) = child {
                if element.name == key {
                    found.push(element);
                }
                element.descendants(key, found);
            }
        }
    }

    fn find(&self, key: &str) -> Option<&Element> {
        let mut found = Vec::new();
        self.descendants(key, &mut found);
        found.into_iter().next()
    }

    fn text(&self) -> String {
        self.children
            .iter()
            .map(|child| match child {
                Node::Element(element) => element.text(),
                Node::Text(text) => text.clone(),
            })
            .collect::<Vec<_>>()
            .join("")
    }
}

fn local(name: &[u8]) -> String {
    String::from_utf8_lossy(name)
        .rsplit(':')
        .next()
        .unwrap_or("")
        .to_string()
}

fn element(event: &quick_xml::events::BytesStart<'_>) -> Result<Element, String> {
    let mut attrs = BTreeMap::new();
    for attr in event.attributes() {
        let attr = attr.map_err(|e| e.to_string())?;
        let name = local(attr.key.as_ref());
        let value = attr
            .normalized_value(quick_xml::XmlVersion::Implicit1_0)
            .map_err(|e| e.to_string())?
            .into_owned();
        if attrs.insert(name, value).is_some() {
            return Err("repeated XML attribute".into());
        }
    }
    Ok(Element {
        name: local(event.name().as_ref()),
        attrs,
        children: Vec::new(),
    })
}

fn parse_xml(bytes: &[u8]) -> Result<Element, String> {
    let source = std::str::from_utf8(bytes).map_err(|e| format!("invalid XML UTF-8: {e}"))?;
    let mut reader = Reader::from_str(source);
    let mut stack: Vec<Element> = Vec::new();
    let mut root = None;
    let mut nodes = 0usize;
    loop {
        let event = reader
            .read_event()
            .map_err(|e| format!("invalid XML: {e}"))?;
        nodes += 1;
        if nodes > MAX_XML_NODES {
            return Err("DOCX XML node count exceeds limit".into());
        }
        match event {
            Event::Start(event) => {
                if stack.len() >= MAX_XML_DEPTH {
                    return Err("DOCX XML nesting exceeds limit".into());
                }
                stack.push(element(&event)?);
            }
            Event::Empty(event) => {
                let child = element(&event)?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(Node::Element(child));
                } else if root.replace(child).is_some() {
                    return Err("more than one XML root".into());
                }
            }
            Event::End(_) => {
                let child = stack.pop().ok_or("unexpected XML end")?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(Node::Element(child));
                } else if root.replace(child).is_some() {
                    return Err("more than one XML root".into());
                }
            }
            Event::Text(event) => {
                let decoded = event.decode().map_err(|e| e.to_string())?;
                let value = quick_xml::escape::unescape(&decoded).map_err(|e| e.to_string())?;
                if let Some(parent) = stack.last_mut() {
                    parent.children.push(Node::Text(value.into_owned()));
                }
            }
            Event::CData(_) | Event::DocType(_) | Event::GeneralRef(_) => {
                return Err("unsupported XML construct in DOCX".into());
            }
            Event::Eof => break,
            _ => {}
        }
    }
    if !stack.is_empty() {
        return Err("unterminated XML element".into());
    }
    root.ok_or("empty XML document".into())
}

fn read_part(zip: &mut ZipArchive<Cursor<&[u8]>>, name: &str) -> Result<Option<Vec<u8>>, String> {
    let Ok(entry) = zip.by_name(name) else {
        return Ok(None);
    };
    if entry.size() > MAX_PART {
        return Err(format!("DOCX part is too large: {name}"));
    }
    let mut bytes = Vec::new();
    entry
        .take(MAX_PART + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_PART {
        return Err(format!("DOCX part expanded too far: {name}"));
    }
    Ok(Some(bytes))
}

fn scan_archive(zip: &mut ZipArchive<Cursor<&[u8]>>) -> Result<(), String> {
    if zip.len() > 128 {
        return Err("DOCX has too many ZIP entries".into());
    }
    let mut expanded = 0u64;
    let mut names = BTreeSet::new();
    for index in 0..zip.len() {
        let entry = zip.by_index(index).map_err(|e| e.to_string())?;
        let name = entry.name().map_err(|e| e.to_string())?;
        if !names.insert(name.to_string())
            || name.starts_with('/')
            || name.contains('\\')
            || name.split('/').any(|part| part == "..")
        {
            return Err("DOCX has an unsafe ZIP entry name".into());
        }
        expanded = expanded.saturating_add(entry.size());
        if expanded > MAX_TOTAL_EXPANDED {
            return Err("DOCX expanded size exceeds 48 MiB".into());
        }
    }
    Ok(())
}

fn image_relationships(
    zip: &mut ZipArchive<Cursor<&[u8]>>,
) -> Result<BTreeMap<String, Option<String>>, String> {
    let Some(bytes) = read_part(zip, "word/_rels/document.xml.rels")? else {
        return Ok(BTreeMap::new());
    };
    let root = parse_xml(&bytes)?;
    let mut out = BTreeMap::new();
    for rel in root.children_named("Relationship") {
        if !rel
            .attr("Type")
            .is_some_and(|kind| kind.ends_with("/image"))
        {
            continue;
        }
        let id = rel.attr("Id").ok_or("image relationship has no ID")?;
        let target = rel
            .attr("Target")
            .ok_or("image relationship has no target")?;
        let accepted = rel.attr("TargetMode") != Some("External")
            && target.starts_with("media/")
            && !target
                .split('/')
                .any(|part| part == ".." || part.is_empty())
            && !target.contains('\\');
        out.insert(id.to_string(), accepted.then(|| format!("word/{target}")));
    }
    Ok(out)
}

fn runs(paragraph: &Element, preview: &mut ImportPreview) -> Vec<TextRun> {
    let mut result = Vec::new();
    for child in &paragraph.children {
        let Node::Element(run) = child else { continue };
        if run.name == "r" {
            for child in &run.children {
                if let Node::Element(element) = child {
                    if !["rPr", "t", "drawing"].contains(&element.name.as_str()) {
                        preview.issue(
                            "inline-content",
                            "A Word run contains unsupported content; import would lose it.",
                            true,
                        );
                    }
                }
            }
            if let Some(props) = run.child("rPr") {
                for child in &props.children {
                    if let Node::Element(property) = child {
                        if !["b", "i"].contains(&property.name.as_str()) {
                            preview.issue("run-format", "A Word run uses formatting outside the supported bold/italic subset.", true);
                        }
                    }
                }
            }
            let mut texts = Vec::new();
            run.descendants("t", &mut texts);
            let text: String = texts.iter().map(|part| part.text()).collect();
            if !text.is_empty() {
                let props = run.child("rPr");
                result.push(TextRun {
                    text,
                    bold: props
                        .and_then(|p| p.child("b"))
                        .is_some_and(|p| !matches!(p.attr("val"), Some("0" | "false" | "off"))),
                    italic: props
                        .and_then(|p| p.child("i"))
                        .is_some_and(|p| !matches!(p.attr("val"), Some("0" | "false" | "off"))),
                });
            }
            if run.find("tab").is_some() || run.find("br").is_some() {
                preview.issue(
                    "inline-control",
                    "Tab and break placement needs manual review.",
                    true,
                );
            }
        } else if !["pPr", "bookmarkStart", "bookmarkEnd"].contains(&run.name.as_str()) {
            preview.issue(
                "inline-content",
                "An unsupported inline element would lose content.",
                true,
            );
        }
    }
    result
}

fn add_images(
    paragraph: &Element,
    relationships: &BTreeMap<String, Option<String>>,
    zip: &mut ZipArchive<Cursor<&[u8]>>,
    preview: &mut ImportPreview,
) -> Result<bool, String> {
    if paragraph.find("pict").is_some() {
        preview.issue("legacy-image", "Legacy Word pictures are not supported; convert the picture to a PNG or JPEG drawing in the source document.", true);
    }
    let mut drawings = Vec::new();
    paragraph.descendants("drawing", &mut drawings);
    if drawings.is_empty() {
        return Ok(false);
    }
    for (index, drawing) in drawings.iter().enumerate() {
        let mut blips = Vec::new();
        drawing.descendants("blip", &mut blips);
        if blips.len() != 1 || drawing.find("pic").is_none() {
            preview.issue(
                "drawing",
                &format!(
                    "Drawing {} is not one supported embedded picture; import would lose content.",
                    index + 1
                ),
                true,
            );
            continue;
        }
        let alt = drawing
            .find("docPr")
            .and_then(|part| part.attr("descr").or_else(|| part.attr("title")))
            .unwrap_or("")
            .trim();
        if alt.is_empty() {
            preview.issue("image-alt", "Embedded image has no alternative text.", true);
        }
        let blip = blips[0];
        let Some(id) = blip.attr("embed") else {
            preview.issue("image", "Embedded image has no relationship ID.", true);
            continue;
        };
        let Some(Some(path)) = relationships.get(id) else {
            preview.issue(
                "image",
                "Embedded image relationship is absent or external.",
                true,
            );
            continue;
        };
        let Some(bytes) = read_part(zip, path)? else {
            preview.issue("image", "Embedded image file is missing.", true);
            continue;
        };
        let format_ok = matches!(
            image::guess_format(&bytes),
            Ok(image::ImageFormat::Png | image::ImageFormat::Jpeg)
        );
        let dimensions_ok = image::ImageReader::new(Cursor::new(&bytes))
            .with_guessed_format()
            .ok()
            .and_then(|reader| reader.into_dimensions().ok())
            .is_some_and(|(width, height)| {
                width > 0
                    && height > 0
                    && width <= 8192
                    && height <= 8192
                    && u64::from(width) * u64::from(height) <= 20_000_000
            });
        let accepted = format_ok && dimensions_ok && image::load_from_memory(&bytes).is_ok();
        if !accepted {
            preview.issue("image", "Embedded image is not a valid PNG or JPEG.", true);
            continue;
        }
        let hash: String = Sha256::digest(&bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        preview.candidate.assets.insert(hash.clone(), bytes);
        preview.candidate.blocks.push(Block::Image {
            hash,
            alt: alt.into(),
        });
    }
    Ok(true)
}

/// Preview a bounded OOXML subset without mutating an open document.
/// DesignCraft's pinned text reader can later supply rich story conversion,
/// but its image/track-change losses must remain visible at this boundary.
pub fn preview_docx(bytes: &[u8]) -> Result<ImportPreview, String> {
    if bytes.len() > MAX_ARCHIVE {
        return Err("DOCX exceeds 16 MiB compressed".into());
    }
    let mut zip = ZipArchive::new(Cursor::new(bytes)).map_err(|e| e.to_string())?;
    scan_archive(&mut zip)?;
    let source =
        read_part(&mut zip, "word/document.xml")?.ok_or("DOCX is missing word/document.xml")?;
    let root = parse_xml(&source)?;
    let body = root.find("body").ok_or("DOCX has no document body")?;
    let relationships = image_relationships(&mut zip)?;
    let mut preview = ImportPreview {
        candidate: Document::default(),
        issues: Vec::new(),
        applicable: true,
    };

    for marker in [
        "ins",
        "del",
        "moveFrom",
        "moveTo",
        "rPrChange",
        "pPrChange",
        "tblPrChange",
        "trPrChange",
        "tcPrChange",
        "sectPrChange",
        "numberingChange",
    ] {
        if root.find(marker).is_some() {
            preview.issue(
                "tracked-changes",
                "Tracked text or formatting revisions require resolution in the source document.",
                true,
            );
        }
    }
    if root.find("tbl").is_some() {
        preview.issue(
            "table",
            "Word tables are outside the first import subset.",
            true,
        );
    }
    if root.find("sectPr").is_some() {
        preview.issue(
            "section-layout",
            "Word section page layout is outside the supported import subset.",
            true,
        );
    }
    for child in &body.children {
        let Node::Element(paragraph) = child else {
            continue;
        };
        if paragraph.name == "sectPr" {
            continue;
        }
        if paragraph.name != "p" {
            if paragraph.name != "tbl" {
                preview.issue("block", "Unsupported Word block requires review.", true);
            }
            continue;
        }
        if let Some(props) = paragraph.child("pPr") {
            for child in &props.children {
                if let Node::Element(property) = child {
                    if !["pStyle", "numPr"].contains(&property.name.as_str()) {
                        preview.issue(
                            "paragraph-format",
                            "Word paragraph formatting is outside the supported import subset.",
                            true,
                        );
                    }
                }
            }
        }
        let text_runs = runs(paragraph, &mut preview);
        let has_image = add_images(paragraph, &relationships, &mut zip, &mut preview)?;
        if has_image {
            if !text_runs.is_empty() {
                preview.issue(
                    "mixed-paragraph",
                    "Text interleaved with an image cannot yet retain its exact order.",
                    true,
                );
            }
            continue;
        }
        let text = text_runs
            .iter()
            .map(|run| run.text.as_str())
            .collect::<Vec<_>>()
            .join("");
        let props = paragraph.child("pPr");
        let style = props
            .and_then(|part| part.child("pStyle"))
            .and_then(|part| part.attr("val"));
        let block = match style {
            Some("Title") => {
                if text_runs.iter().any(|run| run.bold || run.italic) {
                    preview.issue(
                        "heading-format",
                        "Styled title runs cannot yet be preserved.",
                        true,
                    );
                }
                Block::Title { text }
            }
            Some("Heading1" | "Heading 1") => {
                if text_runs.iter().any(|run| run.bold || run.italic) {
                    preview.issue(
                        "heading-format",
                        "Styled heading runs cannot yet be preserved.",
                        true,
                    );
                }
                Block::Heading { level: 1, text }
            }
            Some("Heading2" | "Heading 2") => {
                if text_runs.iter().any(|run| run.bold || run.italic) {
                    preview.issue(
                        "heading-format",
                        "Styled heading runs cannot yet be preserved.",
                        true,
                    );
                }
                Block::Heading { level: 2, text }
            }
            _ if props.and_then(|part| part.child("numPr")).is_some() => {
                preview.issue("list-numbering", "List numbering is provisionally mapped to decimal; verify the source list style.", false);
                Block::ListItem {
                    ordered: true,
                    runs: text_runs,
                }
            }
            _ => {
                if style.is_some() {
                    preview.issue(
                        "paragraph-style",
                        "An unsupported Word paragraph style cannot be preserved.",
                        true,
                    );
                }
                Block::Paragraph { runs: text_runs }
            }
        };
        preview.candidate.blocks.push(block);
    }
    if preview.candidate.blocks.is_empty() {
        preview.issue("empty", "No supported content was found.", true);
    }
    Ok(preview)
}
