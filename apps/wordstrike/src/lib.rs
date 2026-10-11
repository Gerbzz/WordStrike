//! Bounded Word import preview and an editable structured document seed.
//! The pinned DesignCraft DOCX reader drops images and accepts tracked changes;
//! this boundary preserves supported image bytes and refuses those changes.

#[cfg(feature = "donor-export")]
mod designcraft_adapter;
mod docx;
mod model;
#[cfg(feature = "native-ui")]
pub mod native_ui;
mod store;

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

#[cfg(feature = "donor-export")]
pub use designcraft_adapter::{ExportBytes, export_pdf_bytes};
pub use docx::preview_docx;
pub use model::{Command, Editor};
pub use store::{open_document, save_document};

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct PageSetup {
    pub width_pt: f64,
    pub height_pt: f64,
    pub margin_pt: f64,
}

impl Default for PageSetup {
    fn default() -> Self {
        Self {
            width_pt: 595.28,
            height_pt: 841.89,
            margin_pt: 56.69,
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Document {
    pub blocks: Vec<Block>,
    #[serde(default)]
    pub page_setup: PageSetup,
    /// SHA-256 hex digest to original, validated PNG/JPEG bytes.
    pub assets: BTreeMap<String, Vec<u8>>,
}

impl Document {
    /// Import only a reviewed, fully supported preview; failure never changes this document.
    pub fn apply_import(&mut self, preview: ImportPreview) -> Result<(), String> {
        if !preview.applicable || preview.issues.iter().any(|issue| issue.blocking) {
            return Err("resolve blocking DOCX import issues before applying".into());
        }
        model::validate(&preview.candidate)?;
        *self = preview.candidate;
        Ok(())
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TextRun {
    pub text: String,
    pub bold: bool,
    pub italic: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum Block {
    Title { text: String },
    Heading { level: u8, text: String },
    Paragraph { runs: Vec<TextRun> },
    ListItem { ordered: bool, runs: Vec<TextRun> },
    Image { hash: String, alt: String },
    PageBreak,
}

impl Block {
    pub fn plain(text: &str) -> Self {
        Self::Paragraph {
            runs: vec![TextRun {
                text: text.into(),
                bold: false,
                italic: false,
            }],
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImportIssue {
    pub kind: String,
    pub detail: String,
    pub blocking: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub struct ImportPreview {
    pub candidate: Document,
    pub issues: Vec<ImportIssue>,
    pub applicable: bool,
}

impl ImportPreview {
    pub(crate) fn issue(&mut self, kind: &str, detail: &str, blocking: bool) {
        if !self
            .issues
            .iter()
            .any(|issue| issue.kind == kind && issue.detail == detail)
        {
            self.issues.push(ImportIssue {
                kind: kind.into(),
                detail: detail.into(),
                blocking,
            });
        }
        if blocking {
            self.applicable = false;
        }
    }
}
