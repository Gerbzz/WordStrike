use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::{Block, Document, ImportPreview, PageSetup};

const MAX_BLOCKS: usize = 2_000;
const MAX_HISTORY: usize = 100;
const MAX_ASSET_BYTES: usize = 10 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub(crate) struct State {
    pub blocks: Vec<Block>,
    pub page_setup: PageSetup,
}

#[derive(Clone, Debug)]
pub struct Editor {
    pub(crate) document: Document,
    pub(crate) history: Vec<State>,
    pub(crate) cursor: usize,
    pub(crate) disk_baseline: Option<(PathBuf, [u8; 32])>,
    pub(crate) recovery_backup: Option<PathBuf>,
}

#[derive(Clone, Debug)]
pub enum Command {
    Insert { at: usize, block: Block },
    Replace { at: usize, block: Block },
    Remove { at: usize },
    SetPageSetup(PageSetup),
}

impl Editor {
    pub fn new() -> Self {
        let document = Document::default();
        let state = State {
            blocks: Vec::new(),
            page_setup: document.page_setup,
        };
        Self {
            document,
            history: vec![state],
            cursor: 0,
            disk_baseline: None,
            recovery_backup: None,
        }
    }

    pub fn from_document(document: Document) -> Result<Self, String> {
        validate(&document)?;
        let state = State {
            blocks: document.blocks.clone(),
            page_setup: document.page_setup,
        };
        Ok(Self {
            document,
            history: vec![state],
            cursor: 0,
            disk_baseline: None,
            recovery_backup: None,
        })
    }

    pub fn document(&self) -> &Document {
        &self.document
    }

    pub fn apply(&mut self, command: Command) -> Result<(), String> {
        let mut next = self.document.clone();
        match command {
            Command::Insert { at, block } => {
                if at > next.blocks.len() {
                    return Err("insert position is out of range".into());
                }
                next.blocks.insert(at, block);
            }
            Command::Replace { at, block } => {
                let slot = next.blocks.get_mut(at).ok_or("block is out of range")?;
                *slot = block;
            }
            Command::Remove { at } => {
                if at >= next.blocks.len() {
                    return Err("block is out of range".into());
                }
                next.blocks.remove(at);
            }
            Command::SetPageSetup(setup) => next.page_setup = setup,
        }
        self.commit(next)
    }

    pub fn apply_import(&mut self, preview: ImportPreview) -> Result<(), String> {
        if !preview.applicable || preview.issues.iter().any(|issue| issue.blocking) {
            return Err("resolve blocking DOCX import issues before applying".into());
        }
        let mut next = preview.candidate;
        next.page_setup = self.document.page_setup;
        // Keep earlier assets for undo states that refer to them.
        next.assets.extend(self.document.assets.clone());
        self.commit(next)
    }

    pub fn insert_image(&mut self, at: usize, bytes: &[u8], alt: &str) -> Result<(), String> {
        if bytes.is_empty() || bytes.len() > MAX_ASSET_BYTES || alt.trim().is_empty() {
            return Err("image must have bytes under 10 MiB and alternative text".into());
        }
        let format = image::guess_format(bytes).map_err(|e| e.to_string())?;
        if !matches!(format, image::ImageFormat::Png | image::ImageFormat::Jpeg) {
            return Err("only PNG and JPEG images are supported".into());
        }
        let (width, height) = image::ImageReader::new(std::io::Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|e| format!("cannot identify image: {e}"))?
            .into_dimensions()
            .map_err(|e| format!("cannot read image dimensions: {e}"))?;
        if width == 0
            || height == 0
            || width > 8192
            || height > 8192
            || u64::from(width) * u64::from(height) > 20_000_000
        {
            return Err("image dimensions exceed the supported limit".into());
        }
        image::load_from_memory(bytes).map_err(|e| format!("cannot decode image: {e}"))?;
        let hash: String = Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        let mut next = self.document.clone();
        if at > next.blocks.len() {
            return Err("insert position is out of range".into());
        }
        next.assets
            .entry(hash.clone())
            .or_insert_with(|| bytes.to_vec());
        next.blocks.insert(
            at,
            Block::Image {
                hash,
                alt: alt.into(),
            },
        );
        self.commit(next)
    }

    fn commit(&mut self, next: Document) -> Result<(), String> {
        validate(&next)?;
        if next == self.document {
            return Ok(());
        }
        self.history.truncate(self.cursor + 1);
        self.history.push(State {
            blocks: next.blocks.clone(),
            page_setup: next.page_setup,
        });
        if self.history.len() > MAX_HISTORY {
            self.history.remove(0);
        } else {
            self.cursor += 1;
        }
        self.document = next;
        Ok(())
    }

    pub fn undo(&mut self) -> Result<(), String> {
        if self.cursor == 0 {
            return Err("nothing to undo".into());
        }
        self.cursor -= 1;
        self.restore();
        Ok(())
    }

    pub fn redo(&mut self) -> Result<(), String> {
        if self.cursor + 1 >= self.history.len() {
            return Err("nothing to redo".into());
        }
        self.cursor += 1;
        self.restore();
        Ok(())
    }

    fn restore(&mut self) {
        let state = &self.history[self.cursor];
        self.document.blocks = state.blocks.clone();
        self.document.page_setup = state.page_setup;
    }
}

impl Default for Editor {
    fn default() -> Self {
        Self::new()
    }
}

pub(crate) fn validate(document: &Document) -> Result<(), String> {
    validate_parts(&document.blocks, document.page_setup, &document.assets)?;
    for (hash, bytes) in &document.assets {
        if bytes.is_empty()
            || bytes.len() > MAX_ASSET_BYTES
            || Sha256::digest(bytes)
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
                != *hash
        {
            return Err("document asset hash or size is invalid".into());
        }
    }
    Ok(())
}

pub(crate) fn validate_parts(
    blocks: &[Block],
    setup: PageSetup,
    assets: &BTreeMap<String, Vec<u8>>,
) -> Result<(), String> {
    if !setup.width_pt.is_finite()
        || !setup.height_pt.is_finite()
        || !setup.margin_pt.is_finite()
        || !(300.0..=2000.0).contains(&setup.width_pt)
        || !(400.0..=3000.0).contains(&setup.height_pt)
        || setup.margin_pt < 18.0
        || 2.0 * setup.margin_pt >= setup.width_pt.min(setup.height_pt)
        || blocks.len() > MAX_BLOCKS
        || assets.len() > 100
    {
        return Err("unsupported page setup, block count, or asset count".into());
    }
    for block in blocks {
        match block {
            Block::Title { text } if text.len() <= 100_000 => (),
            Block::Heading { level: 1 | 2, text } if text.len() <= 100_000 => (),
            Block::Heading { .. } => return Err("heading exceeds text limit".into()),
            Block::Paragraph { runs } | Block::ListItem { runs, .. }
                if runs.iter().map(|run| run.text.len()).sum::<usize>() <= 100_000 =>
            {
                ()
            }
            Block::Image { hash, alt }
                if !alt.trim().is_empty() && alt.len() <= 1000 && assets.contains_key(hash) =>
            {
                ()
            }
            Block::PageBreak => (),
            _ => return Err("invalid text block or missing image".into()),
        }
    }
    Ok(())
}
