//! Conservative paged WordStrike blocks mapped to the pinned DesignCraft document/PDF crates.
//! Unsupported inline styling is reported; the editable `.wstrike` retains its runs and images.

use std::collections::BTreeMap;
use std::io::Cursor;
use std::sync::Arc;

use designcraft_compose::Cache;
use designcraft_doc::build::NewDocument;
use designcraft_doc::{
    Asset, AssetId, Content, Document as DonorDocument, Fitting, Graphic, Item, ItemId, Margins,
    ParaFormat, Shape, SpreadRef,
};
use designcraft_geom::{Affine, Rect, shapes};
use designcraft_pdf::PdfOptions;

use crate::model::validate;
use crate::{Block, Document};

const MAX_PAGES: usize = 200;

fn checked_image(bytes: &[u8]) -> Result<(u32, u32), String> {
    let (width, height) = image::ImageReader::new(Cursor::new(bytes))
        .with_guessed_format()
        .map_err(|e| format!("cannot identify document image: {e}"))?
        .into_dimensions()
        .map_err(|e| format!("cannot read document image dimensions: {e}"))?;
    if width == 0
        || height == 0
        || width > 8192
        || height > 8192
        || u64::from(width) * u64::from(height) > 20_000_000
    {
        return Err("document image dimensions exceed the supported limit".into());
    }
    image::load_from_memory(bytes).map_err(|e| format!("cannot decode document image: {e}"))?;
    Ok((width, height))
}

pub struct ExportBytes {
    pub bytes: Vec<u8>,
    pub pages: usize,
    pub warnings: Vec<String>,
}

struct Placement {
    page: usize,
    y: f64,
    height: f64,
    block: Block,
}

fn text_and_size(block: &Block) -> Option<(String, f64, bool)> {
    match block {
        Block::Title { text } => Some((text.clone(), 26.0, false)),
        Block::Heading { level, text } => {
            Some((text.clone(), if *level == 1 { 20.0 } else { 16.0 }, false))
        }
        Block::Paragraph { runs } => Some((
            runs.iter().map(|r| r.text.as_str()).collect(),
            11.0,
            runs.iter().any(|r| r.bold || r.italic),
        )),
        Block::ListItem { ordered, runs } => Some((
            format!(
                "{}{}",
                if *ordered { "1. " } else { "• " },
                runs.iter().map(|r| r.text.as_str()).collect::<String>()
            ),
            11.0,
            runs.iter().any(|r| r.bold || r.italic),
        )),
        _ => None,
    }
}

fn placements(source: &Document) -> Result<(Vec<Placement>, usize, Vec<String>), String> {
    let setup = source.page_setup;
    let available_width = setup.width_pt - 2.0 * setup.margin_pt;
    let available_height = setup.height_pt - 2.0 * setup.margin_pt - 24.0;
    let mut page = 0;
    let mut y = setup.margin_pt;
    let mut result = Vec::new();
    let mut warnings = Vec::new();
    let mut image_sizes = BTreeMap::new();
    for block in &source.blocks {
        if matches!(block, Block::PageBreak) {
            page += 1;
            y = setup.margin_pt;
            continue;
        }
        let height = if let Some((text, size, flattened)) = text_and_size(block) {
            if flattened && !warnings.iter().any(|w| w == "Inline bold/italic is retained in the editable file but flattened in this PDF.") {
                warnings.push("Inline bold/italic is retained in the editable file but flattened in this PDF.".into());
            }
            let columns = ((available_width / (size * 0.65)).floor() as usize).max(8);
            let lines: usize = text
                .lines()
                .map(|line| (line.chars().count().max(1) + columns - 1) / columns)
                .sum::<usize>()
                .max(1);
            (lines as f64 + 1.0) * size * 1.5
        } else if let Block::Image { hash, .. } = block {
            let bytes = source
                .assets
                .get(hash)
                .ok_or("document image asset is missing")?;
            let (width, height) = if let Some(size) = image_sizes.get(hash) {
                *size
            } else {
                let size = checked_image(bytes)?;
                image_sizes.insert(hash.clone(), size);
                size
            };
            let ratio = f64::from(height) / f64::from(width);
            available_width.min(420.0) * ratio + 18.0
        } else {
            return Err("unsupported document block".into());
        };
        if !height.is_finite() || height > available_height {
            return Err("a block is too tall for one page; split it before PDF export".into());
        }
        if y + height > setup.height_pt - setup.margin_pt - 24.0 {
            page += 1;
            y = setup.margin_pt;
        }
        if page >= MAX_PAGES {
            return Err("document exceeds 200 export pages".into());
        }
        result.push(Placement {
            page,
            y,
            height,
            block: block.clone(),
        });
        y += height;
    }
    if page >= MAX_PAGES {
        return Err("document exceeds 200 export pages".into());
    }
    warnings.push("Pagination uses conservative block height estimates; inspect the exported pages for clipping and font fallback.".into());
    Ok((result, page + 1, warnings))
}

pub fn export_pdf_bytes(source: &Document) -> Result<ExportBytes, String> {
    validate(source)?;
    let (placements, pages, mut warnings) = placements(source)?;
    let setup = source.page_setup;
    let mut donor = DonorDocument::new(&NewDocument {
        title: "WordStrike document".into(),
        width: setup.width_pt,
        height: setup.height_pt,
        pages,
        facing_pages: false,
        primary_text_frame: false,
        margins: Margins::uniform(setup.margin_pt),
        ..NewDocument::default()
    });
    let layer = donor.default_layer();
    let mut asset_ids = BTreeMap::new();
    for placement in placements {
        let (spread, page) = donor
            .page_loc(placement.page)
            .ok_or("donor page is missing")?;
        let page_x = donor.spreads[spread].pages[page].x;
        let bounds = Rect::new(
            page_x + setup.margin_pt,
            placement.y,
            page_x + setup.width_pt - setup.margin_pt,
            placement.y + placement.height,
        );
        let spread_ref = SpreadRef::Doc(spread);
        if let Some((text, size, _)) = text_and_size(&placement.block) {
            let mut para = ParaFormat::default();
            para.chars.size = Some(size);
            donor
                .add_text_frame(spread_ref, bounds, layer, &text, para)
                .map_err(|e| e.to_string())?;
        } else if let Block::Image { hash, alt } = placement.block {
            let id = if let Some(id) = asset_ids.get(&hash) {
                *id
            } else {
                let bytes = source
                    .assets
                    .get(&hash)
                    .ok_or("document image asset is missing")?;
                let pixels = checked_image(bytes)?;
                let id = AssetId(donor.alloc());
                donor.assets.insert(
                    id,
                    Arc::new(Asset {
                        id,
                        name: format!("{hash}.image"),
                        mime: designcraft_images::mime(bytes).into(),
                        link: None,
                        data: Arc::new(bytes.clone()),
                        pixels: Some(pixels),
                        page: 0,
                    }),
                );
                asset_ids.insert(hash, id);
                id
            };
            let pixels = donor
                .assets
                .get(&id)
                .and_then(|asset| asset.pixels)
                .ok_or("donor image dimensions are missing")?;
            let item_id = ItemId(donor.alloc());
            let mut item = Item::new(item_id, layer, Shape::Rectangle, shapes::rectangle(bounds));
            item.alt_text = alt;
            let mut graphic = Graphic {
                asset: id,
                size: (f64::from(pixels.0), f64::from(pixels.1)),
                xf: Affine::IDENTITY,
                auto_fit: Fitting::FitProportionally,
                fit_align: 4,
                crop: [0.0; 4],
            };
            graphic.xf = graphic
                .fitted(bounds, Fitting::FitProportionally)
                .ok_or("document image cannot fit its frame")?;
            item.content = Content::Graphic(graphic);
            donor
                .insert_item(spread_ref, item, None)
                .map_err(|e| e.to_string())?;
        }
    }
    for index in 0..pages {
        let (spread, page) = donor.page_loc(index).ok_or("donor page is missing")?;
        let page_x = donor.spreads[spread].pages[page].x;
        let rect = Rect::new(
            page_x + setup.margin_pt,
            setup.height_pt - setup.margin_pt,
            page_x + setup.width_pt - setup.margin_pt,
            setup.height_pt - setup.margin_pt + 16.0,
        );
        let mut para = ParaFormat::default();
        para.chars.size = Some(9.0);
        donor
            .add_text_frame(
                SpreadRef::Doc(spread),
                rect,
                layer,
                &(index + 1).to_string(),
                para,
            )
            .map_err(|e| e.to_string())?;
    }
    let report = designcraft_pdf::export_pdf_with_report(
        &donor,
        &Cache::new(),
        &PdfOptions {
            tagged: true,
            ..PdfOptions::default()
        },
    )
    .map_err(|e| format!("donor PDF export failed: {e}"))?;
    if report.pages != pages {
        return Err("donor PDF page count differs from the layout".into());
    }
    warnings.extend(report.warnings);
    Ok(ExportBytes {
        bytes: report.bytes,
        pages,
        warnings,
    })
}
