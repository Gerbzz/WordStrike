//! A small native paged editor. PDF export uses the selected DesignCraft crates.
//! Source-only until the next bounded build and interactive accessibility check.

use std::collections::BTreeMap;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::PathBuf;

use eframe::egui;
use uuid::Uuid;

use crate::store::{
    abandon_pre_move_document, pending_pre_move_document, recover_missing_document,
    resume_pre_move_document,
};
use crate::{
    Block, Command, Document, Editor, ImportPreview, PageSetup, TextRun, export_pdf_bytes,
    open_document, preview_docx, save_document,
};

pub struct NativeEditor {
    path: PathBuf,
    editor: Editor,
    saved_document: Document,
    reopen_requested: bool,
    pending_recovery: Option<PathBuf>,
    import_path: String,
    image_path: String,
    pending_import: Option<ImportPreview>,
    textures: BTreeMap<String, egui::TextureHandle>,
    status: String,
}

impl NativeEditor {
    pub fn open_or_create(path: PathBuf) -> Result<Self, String> {
        if path.extension().is_none_or(|ext| ext != "wstrike") {
            return Err("native document path must end in .wstrike".into());
        }
        let (editor, status, pending_recovery) = if path.exists() {
            let editor = open_document(&path)?;
            let (pending, status) = Self::inspect_recovery(&path);
            (editor, status, pending)
        } else if let Some((editor, backup)) = recover_missing_document(&path)? {
            (
                editor,
                format!(
                    "Recovered the interrupted save from {}. Save restores the missing document; the backup remains available.",
                    backup.display()
                ),
                None,
            )
        } else {
            (Editor::new(), "Ready".to_string(), None)
        };
        let saved_document = editor.document().clone();
        Ok(Self {
            path,
            editor,
            saved_document,
            reopen_requested: false,
            pending_recovery,
            import_path: String::new(),
            image_path: String::new(),
            pending_import: None,
            textures: BTreeMap::new(),
            status,
        })
    }

    fn inspect_recovery(path: &PathBuf) -> (Option<PathBuf>, String) {
        match pending_pre_move_document(path) {
            Ok(Some(staged)) => (
                Some(staged.clone()),
                format!(
                    "Interrupted save: original remains at {} and staged edits at {}. Choose Resume or Abandon.",
                    path.display(),
                    staged.display()
                ),
            ),
            Ok(None) => (None, "Ready".into()),
            Err(error) => (None, error),
        }
    }

    fn apply(&mut self, command: Command) {
        self.status = match self.editor.apply(command) {
            Ok(()) => "Edited; save the document to keep this change.".into(),
            Err(error) => error,
        };
    }

    fn save(&mut self) {
        self.status = match save_document(&mut self.editor, &self.path) {
            Ok(()) => {
                self.saved_document = self.editor.document().clone();
                self.pending_recovery = None;
                format!("Saved {}", self.path.display())
            }
            Err(error) => {
                self.pending_recovery = pending_pre_move_document(&self.path).ok().flatten();
                error
            }
        };
    }

    fn resume_interrupted_save(&mut self) {
        if self.editor.document() != &self.saved_document {
            self.status = "Unsaved edits are present. Save a separate copy before resuming the staged document.".into();
            return;
        }
        self.status = match resume_pre_move_document(&self.path)
            .and_then(|staged| open_document(&self.path).map(|editor| (staged, editor)))
        {
            Ok((staged, editor)) => {
                self.editor = editor;
                self.saved_document = self.editor.document().clone();
                self.pending_recovery = None;
                self.pending_import = None;
                self.textures.clear();
                format!(
                    "Resumed staged document from {}. The original remains in the recovery backup.",
                    staged.display()
                )
            }
            Err(error) => error,
        };
    }

    fn abandon_interrupted_save(&mut self) {
        self.status = match abandon_pre_move_document(&self.path) {
            Ok(staged) => {
                self.pending_recovery = None;
                format!(
                    "Abandoned interrupted save. Original remains open; staged edits remain at {}. Save is available.",
                    staged.display()
                )
            }
            Err(error) => error,
        };
    }

    fn save_copy(&mut self) {
        let parent = self
            .path
            .parent()
            .unwrap_or_else(|| std::path::Path::new("."));
        let stem = self
            .path
            .file_stem()
            .and_then(|part| part.to_str())
            .unwrap_or("document");
        let path = parent.join(format!("{stem}-copy-{}.wstrike", Uuid::new_v4()));
        let mut copy = self.editor.clone();
        copy.disk_baseline = None;
        copy.recovery_backup = None;
        self.status = match save_document(&mut copy, &path) {
            Ok(()) => {
                self.path = path;
                self.editor = copy;
                self.saved_document = self.editor.document().clone();
                self.pending_recovery = None;
                format!("Saved editable copy at {}", self.path.display())
            }
            Err(error) => error,
        };
    }

    fn request_reopen(&mut self) {
        if self.editor.document() != &self.saved_document {
            self.reopen_requested = true;
            self.status =
                "Unsaved edits are present. Confirm discard to reopen the disk version.".into();
        } else {
            self.confirm_reopen();
        }
    }

    fn confirm_reopen(&mut self) {
        let opened = if self.path.exists() {
            open_document(&self.path).map(|editor| (editor, None))
        } else {
            recover_missing_document(&self.path).and_then(|result| {
                result
                    .map(|(editor, backup)| (editor, Some(backup)))
                    .ok_or_else(|| "No saved document exists at this path.".into())
            })
        };
        self.status = match opened {
            Ok((editor, backup)) => {
                self.editor = editor;
                self.saved_document = self.editor.document().clone();
                self.reopen_requested = false;
                self.pending_import = None;
                self.textures.clear();
                if let Some(backup) = backup {
                    self.pending_recovery = None;
                    format!(
                        "Recovered interrupted save from {}; Save restores the missing document.",
                        backup.display()
                    )
                } else {
                    let (pending, status) = Self::inspect_recovery(&self.path);
                    self.pending_recovery = pending;
                    status
                }
            }
            Err(error) => error,
        };
    }

    fn import_docx(&mut self) {
        let result = (|| -> Result<ImportPreview, String> {
            let path = self.import_path.trim();
            if fs::metadata(path).map_err(|e| e.to_string())?.len() > 16 * 1024 * 1024 {
                return Err("DOCX exceeds 16 MiB".into());
            }
            preview_docx(&fs::read(path).map_err(|e| e.to_string())?)
        })();
        self.status = match result {
            Ok(preview) => {
                let issues = preview.issues.len();
                let applicable = preview.applicable;
                self.pending_import = Some(preview);
                if applicable {
                    format!("DOCX preview ready with {issues} issue(s). Review and apply below.")
                } else {
                    format!(
                        "DOCX has {issues} blocking issue(s); current document was not changed."
                    )
                }
            }
            Err(error) => error,
        };
    }

    fn image(&mut self, at: usize) {
        let path = PathBuf::from(self.image_path.trim());
        self.status = match fs::metadata(&path).and_then(|metadata| {
            if metadata.len() > 10 * 1024 * 1024 {
                Err(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    "image exceeds 10 MiB",
                ))
            } else {
                fs::read(&path).map(|bytes| (metadata, bytes))
            }
        }) {
            Err(error) => error.to_string(),
            Ok((_, bytes)) => {
                let alt = path
                    .file_stem()
                    .and_then(|name| name.to_str())
                    .unwrap_or("Image");
                match self.editor.insert_image(at, &bytes, alt) {
                    Ok(()) => "Image inserted; review its alternative text.".into(),
                    Err(error) => error,
                }
            }
        };
    }

    fn export_pdf(&mut self) {
        let path = self.path.with_extension("pdf");
        self.status = match export_pdf_bytes(self.editor.document()).and_then(|report| {
            let mut file = OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&path)
                .map_err(|e| format!("{}: {e}", path.display()))?;
            if let Err(error) = file.write_all(&report.bytes).and_then(|()| file.sync_all()) {
                drop(file);
                let _ = fs::remove_file(&path);
                return Err(format!("{}: {error}", path.display()));
            }
            Ok(format!(
                "Exported {} page(s) to {}. {}",
                report.pages,
                path.display(),
                report.warnings.join(" ")
            ))
        }) {
            Ok(message) => message,
            Err(error) => error,
        };
    }

    fn texture(&mut self, ui: &egui::Ui, hash: &str) -> Option<egui::TextureId> {
        if !self.textures.contains_key(hash) {
            let bytes = self.editor.document().assets.get(hash)?;
            let (width, height) = image::ImageReader::new(std::io::Cursor::new(bytes))
                .with_guessed_format()
                .ok()?
                .into_dimensions()
                .ok()?;
            if width == 0
                || height == 0
                || width > 8192
                || height > 8192
                || u64::from(width) * u64::from(height) > 20_000_000
            {
                return None;
            }
            let rgba = image::load_from_memory(bytes).ok()?.to_rgba8();
            let image = egui::ColorImage::from_rgba_unmultiplied(
                [rgba.width() as usize, rgba.height() as usize],
                rgba.as_raw(),
            );
            self.textures.insert(
                hash.into(),
                ui.ctx()
                    .load_texture(hash, image, egui::TextureOptions::LINEAR),
            );
        }
        self.textures.get(hash).map(egui::TextureHandle::id)
    }

    fn draw_block(&mut self, ui: &mut egui::Ui, index: usize, block: Block) {
        ui.horizontal(|ui| {
            ui.label(format!("{:02}", index + 1));
            if ui.small_button("Remove").clicked() {
                self.apply(Command::Remove { at: index });
            }
        });
        match block.clone() {
            Block::Title { mut text } => {
                ui.label("Title");
                if ui
                    .add(egui::TextEdit::singleline(&mut text).font(egui::TextStyle::Heading))
                    .changed()
                {
                    self.apply(Command::Replace {
                        at: index,
                        block: Block::Title { text },
                    });
                }
            }
            Block::Heading { level, mut text } => {
                ui.label(format!("Heading {level}"));
                if ui.text_edit_singleline(&mut text).changed() {
                    self.apply(Command::Replace {
                        at: index,
                        block: Block::Heading { level, text },
                    });
                }
            }
            Block::Paragraph { runs } | Block::ListItem { runs, .. } => {
                let rich = runs.len() != 1 || runs.iter().any(|run| run.bold || run.italic);
                let mut text: String = runs.iter().map(|run| run.text.as_str()).collect();
                if rich {
                    ui.label("Imported inline formatting is preserved. Convert to plain text to edit this block.");
                    ui.label(&text);
                    if ui.button("Convert to plain text").clicked() {
                        let plain = vec![TextRun {
                            text,
                            bold: false,
                            italic: false,
                        }];
                        let replacement = if let Block::ListItem { ordered, .. } = block {
                            Block::ListItem {
                                ordered,
                                runs: plain,
                            }
                        } else {
                            Block::Paragraph { runs: plain }
                        };
                        self.apply(Command::Replace {
                            at: index,
                            block: replacement,
                        });
                    }
                } else if ui.text_edit_multiline(&mut text).changed() {
                    let plain = vec![TextRun {
                        text,
                        bold: false,
                        italic: false,
                    }];
                    let replacement = if let Block::ListItem { ordered, .. } = block {
                        Block::ListItem {
                            ordered,
                            runs: plain,
                        }
                    } else {
                        Block::Paragraph { runs: plain }
                    };
                    self.apply(Command::Replace {
                        at: index,
                        block: replacement,
                    });
                }
            }
            Block::Image { hash, mut alt } => {
                ui.label("Image alternative text");
                if ui.text_edit_singleline(&mut alt).changed() {
                    self.apply(Command::Replace {
                        at: index,
                        block: Block::Image {
                            hash: hash.clone(),
                            alt,
                        },
                    });
                }
                if let Some(texture) = self.texture(ui, &hash) {
                    ui.image((texture, egui::vec2(280.0, 157.5)));
                } else {
                    ui.colored_label(egui::Color32::RED, "Image bytes unavailable");
                }
            }
            Block::PageBreak => {
                ui.separator();
                ui.label("Hard page break");
            }
        }
        ui.add_space(12.0);
    }
}

impl eframe::App for NativeEditor {
    fn ui(&mut self, root_ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = root_ui.ctx().clone();
        egui::Panel::top("toolbar").show(root_ui, |ui| {
            ui.horizontal_wrapped(|ui| {
                if ui.button("Save").clicked() {
                    self.save();
                }
                if ui.button("Save copy").clicked() {
                    self.save_copy();
                }
                if self.pending_recovery.is_some() {
                    if ui.button("Resume interrupted save").clicked() {
                        self.resume_interrupted_save();
                    }
                    if ui.button("Abandon staged save").clicked() {
                        self.abandon_interrupted_save();
                    }
                }
                if ui.button("Reopen saved").clicked() {
                    self.request_reopen();
                }
                if ui.button("Undo").clicked() {
                    self.status = self
                        .editor
                        .undo()
                        .map(|_| "Undid edit".into())
                        .unwrap_or_else(|e| e);
                }
                if ui.button("Redo").clicked() {
                    self.status = self
                        .editor
                        .redo()
                        .map(|_| "Redid edit".into())
                        .unwrap_or_else(|e| e);
                }
                if ui.button("Export PDF").clicked() {
                    self.export_pdf();
                }
            });
            ui.label(&self.status);
        });
        egui::Panel::left("structure")
            .resizable(true)
            .default_size(250.0)
            .show(root_ui, |ui| {
                ui.heading("Structure");
                for (index, block) in self.editor.document().blocks.iter().enumerate() {
                    match block {
                        Block::Title { text } => {
                            ui.label(format!("Title  {text}"));
                        }
                        Block::Heading { level, text } => {
                            ui.label(format!("H{level}  {text}"));
                        }
                        _ => {
                            let _ = index;
                        }
                    }
                }
                ui.separator();
                let end = self.editor.document().blocks.len();
                if ui.button("Add title").clicked() {
                    self.apply(Command::Insert {
                        at: end,
                        block: Block::Title {
                            text: "Title".into(),
                        },
                    });
                }
                if ui.button("Add heading").clicked() {
                    self.apply(Command::Insert {
                        at: end,
                        block: Block::Heading {
                            level: 1,
                            text: "Heading".into(),
                        },
                    });
                }
                if ui.button("Add paragraph").clicked() {
                    self.apply(Command::Insert {
                        at: end,
                        block: Block::plain(""),
                    });
                }
                if ui.button("Add bullet").clicked() {
                    self.apply(Command::Insert {
                        at: end,
                        block: Block::ListItem {
                            ordered: false,
                            runs: vec![TextRun {
                                text: "Item".into(),
                                bold: false,
                                italic: false,
                            }],
                        },
                    });
                }
                if ui.button("Add page break").clicked() {
                    self.apply(Command::Insert {
                        at: end,
                        block: Block::PageBreak,
                    });
                }
                ui.label("Local PNG/JPEG path");
                ui.text_edit_singleline(&mut self.image_path);
                if ui.button("Insert image").clicked() {
                    self.image(end);
                }
                ui.separator();
                ui.label("DOCX import path");
                ui.text_edit_singleline(&mut self.import_path);
                if ui.button("Preview DOCX").clicked() {
                    self.import_docx();
                }
                if let Some(preview) = &self.pending_import {
                    for issue in &preview.issues {
                        ui.label(format!("{}: {}", issue.kind, issue.detail));
                    }
                    if preview.applicable && ui.button("Apply reviewed import").clicked() {
                        let preview = self.pending_import.take();
                        if let Some(preview) = preview {
                            self.status = self
                                .editor
                                .apply_import(preview)
                                .map(|_| "Imported into editable document".into())
                                .unwrap_or_else(|e| e);
                        }
                    }
                }
            });
        egui::Panel::right("page_setup")
            .resizable(true)
            .default_size(200.0)
            .show(root_ui, |ui| {
                ui.heading("Page setup");
                let mut setup = self.editor.document().page_setup;
                ui.label("Width / height / margin (pt)");
                ui.add(egui::DragValue::new(&mut setup.width_pt));
                ui.add(egui::DragValue::new(&mut setup.height_pt));
                ui.add(egui::DragValue::new(&mut setup.margin_pt));
                if setup != self.editor.document().page_setup {
                    self.apply(Command::SetPageSetup(setup));
                }
                ui.label("PDF pagination is conservative. Inspect export warnings and every page.");
            });
        egui::CentralPanel::default().show(root_ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.heading("Document blocks and page breaks");
                let blocks = self.editor.document().blocks.clone();
                ui.group(|ui| {
                    for (index, block) in blocks.into_iter().enumerate() {
                        let page_break = matches!(block, Block::PageBreak);
                        self.draw_block(ui, index, block);
                        if page_break {
                            ui.separator();
                            ui.heading("Next page");
                        }
                    }
                });
            });
        });
        if self.reopen_requested {
            egui::Window::new("Discard unsaved edits?")
                .collapsible(false)
                .show(&ctx, |ui| {
                    ui.label("Reopening reads the disk document and discards unsaved edits in this window.");
                    if ui.button("Discard edits and reopen").clicked() {
                        self.confirm_reopen();
                    }
                    if ui.button("Keep editing").clicked() {
                        self.reopen_requested = false;
                    }
                });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sha2::{Digest, Sha256};
    use uuid::Uuid;

    fn pending_save(path: &std::path::Path) -> (PathBuf, PathBuf) {
        let backup = path.with_extension("wstrike.previous");
        let staged = path.with_extension("wstrike.test.tmp");
        let mut candidate = open_document(path).unwrap();
        candidate.disk_baseline = None;
        candidate
            .apply(Command::Replace {
                at: 0,
                block: Block::plain("Staged edit"),
            })
            .unwrap();
        save_document(&mut candidate, &staged).unwrap();
        let original_digest: [u8; 32] = Sha256::digest(fs::read(path).unwrap()).into();
        let published_digest: [u8; 32] = Sha256::digest(fs::read(&staged).unwrap()).into();
        let record = serde_json::json!({
            "version": 2,
            "backup": backup.file_name().unwrap().to_string_lossy(),
            "staged": staged.file_name().unwrap().to_string_lossy(),
            "original_digest": original_digest,
            "published_digest": published_digest,
        });
        fs::write(
            path.with_extension("wstrike.recovery"),
            serde_json::to_vec(&record).unwrap(),
        )
        .unwrap();
        (backup, staged)
    }

    #[test]
    fn reopen_requires_explicit_discard_when_document_is_dirty() {
        let root = std::env::temp_dir().join(format!("wordstrike-reopen-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let path = root.join("document.wstrike");
        let mut app = NativeEditor::open_or_create(path).unwrap();
        app.apply(Command::Insert {
            at: 0,
            block: Block::plain("Saved"),
        });
        app.save();
        app.apply(Command::Replace {
            at: 0,
            block: Block::plain("Unsaved"),
        });
        app.request_reopen();
        assert!(app.reopen_requested);
        assert_eq!(app.editor.document().blocks, vec![Block::plain("Unsaved")]);
        app.reopen_requested = false;
        assert_eq!(app.editor.document().blocks, vec![Block::plain("Unsaved")]);
        app.request_reopen();
        app.confirm_reopen();
        assert_eq!(app.editor.document().blocks, vec![Block::plain("Saved")]);
        assert!(!app.reopen_requested);
    }

    #[test]
    fn save_copy_after_external_conflict_keeps_both_documents() {
        let root = std::env::temp_dir().join(format!("wordstrike-copy-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let original = root.join("document.wstrike");
        let mut app = NativeEditor::open_or_create(original.clone()).unwrap();
        app.apply(Command::Insert {
            at: 0,
            block: Block::plain("Original"),
        });
        app.save();
        let mut external = open_document(&original).unwrap();
        external
            .apply(Command::Replace {
                at: 0,
                block: Block::plain("External"),
            })
            .unwrap();
        save_document(&mut external, &original).unwrap();
        app.apply(Command::Replace {
            at: 0,
            block: Block::plain("Local"),
        });
        app.save();
        assert!(app.status.contains("changed outside"));
        app.save_copy();
        assert_ne!(app.path, original);
        assert_eq!(
            open_document(&original).unwrap().document().blocks,
            vec![Block::plain("External")]
        );
        assert_eq!(
            open_document(&app.path).unwrap().document().blocks,
            vec![Block::plain("Local")]
        );
    }

    #[test]
    fn interrupted_save_opens_recorded_backup_and_restores_missing_path() {
        let root = std::env::temp_dir().join(format!("wordstrike-recovery-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let path = root.join("document.wstrike");
        let mut app = NativeEditor::open_or_create(path.clone()).unwrap();
        app.apply(Command::Insert {
            at: 0,
            block: Block::plain("Retained original"),
        });
        app.save();
        let (backup, staged) = pending_save(&path);
        fs::rename(&path, &backup).unwrap();
        app.request_reopen();
        assert!(app.status.contains("Recovered interrupted save"));
        let mut restarted = NativeEditor::open_or_create(path.clone()).unwrap();
        assert!(restarted.status.contains("Recovered"));
        assert_eq!(
            restarted.editor.document().blocks,
            vec![Block::plain("Retained original")]
        );
        restarted.save();
        assert_eq!(
            open_document(&path).unwrap().document().blocks,
            vec![Block::plain("Retained original")]
        );
        assert!(backup.exists());
        assert_eq!(
            open_document(&staged).unwrap().document().blocks,
            vec![Block::plain("Staged edit")]
        );
        assert!(!path.with_extension("wstrike.recovery").exists());
    }

    #[test]
    fn pre_move_recovery_resumes_staged_edit_and_retains_original() {
        let root = std::env::temp_dir().join(format!("wordstrike-premoves-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let path = root.join("document.wstrike");
        let mut first = NativeEditor::open_or_create(path.clone()).unwrap();
        first.apply(Command::Insert {
            at: 0,
            block: Block::plain("Original"),
        });
        first.save();
        let (backup, staged) = pending_save(&path);

        let mut restarted = NativeEditor::open_or_create(path.clone()).unwrap();
        assert_eq!(restarted.pending_recovery, Some(staged.clone()));
        assert_eq!(
            restarted.editor.document().blocks,
            vec![Block::plain("Original")]
        );
        restarted.save();
        assert!(restarted.status.contains("Resume or Abandon"));
        restarted.resume_interrupted_save();
        assert_eq!(
            restarted.editor.document().blocks,
            vec![Block::plain("Staged edit")]
        );
        assert_eq!(
            open_document(&path).unwrap().document().blocks,
            vec![Block::plain("Staged edit")]
        );
        assert_eq!(
            open_document(&backup).unwrap().document().blocks,
            vec![Block::plain("Original")]
        );
        assert!(staged.exists());
        assert!(!path.with_extension("wstrike.recovery").exists());
    }

    #[test]
    fn pre_move_abandon_preserves_staged_edit_and_allows_normal_save() {
        let root = std::env::temp_dir().join(format!("wordstrike-abandon-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let path = root.join("document.wstrike");
        let mut first = NativeEditor::open_or_create(path.clone()).unwrap();
        first.apply(Command::Insert {
            at: 0,
            block: Block::plain("Original"),
        });
        first.save();
        let (_, staged) = pending_save(&path);

        let mut restarted = NativeEditor::open_or_create(path.clone()).unwrap();
        restarted.abandon_interrupted_save();
        assert!(restarted.status.contains("Save is available"));
        restarted.apply(Command::Replace {
            at: 0,
            block: Block::plain("New local edit"),
        });
        restarted.save();
        assert_eq!(
            open_document(&path).unwrap().document().blocks,
            vec![Block::plain("New local edit")]
        );
        assert_eq!(
            open_document(&staged).unwrap().document().blocks,
            vec![Block::plain("Staged edit")]
        );
    }

    #[test]
    fn pre_move_external_edit_blocks_recovery_without_losing_staged_copy() {
        let root =
            std::env::temp_dir().join(format!("wordstrike-external-recovery-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let path = root.join("document.wstrike");
        let mut first = NativeEditor::open_or_create(path.clone()).unwrap();
        first.apply(Command::Insert {
            at: 0,
            block: Block::plain("Original"),
        });
        first.save();
        let (backup, staged) = pending_save(&path);
        let external_path = root.join("external.wstrike");
        let mut external = Editor::new();
        external
            .apply(Command::Insert {
                at: 0,
                block: Block::plain("External"),
            })
            .unwrap();
        save_document(&mut external, &external_path).unwrap();
        fs::copy(&external_path, &path).unwrap();

        let mut restarted = NativeEditor::open_or_create(path.clone()).unwrap();
        assert!(restarted.status.contains("changed outside"));
        restarted.save();
        assert!(restarted.status.contains("changed outside"));
        restarted.resume_interrupted_save();
        assert!(restarted.status.contains("changed outside"));
        restarted.abandon_interrupted_save();
        assert!(restarted.status.contains("changed outside"));
        assert_eq!(
            open_document(&path).unwrap().document().blocks,
            vec![Block::plain("External")]
        );
        assert_eq!(
            open_document(&staged).unwrap().document().blocks,
            vec![Block::plain("Staged edit")]
        );
        assert!(!backup.exists());
        assert!(path.with_extension("wstrike.recovery").exists());
    }

    #[test]
    fn post_publication_record_is_cleared_on_next_save() {
        let root = std::env::temp_dir().join(format!("wordstrike-postpublish-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let path = root.join("document.wstrike");
        let mut first = NativeEditor::open_or_create(path.clone()).unwrap();
        first.apply(Command::Insert {
            at: 0,
            block: Block::plain("Original"),
        });
        first.save();
        let (backup, staged) = pending_save(&path);
        fs::rename(&path, &backup).unwrap();
        fs::hard_link(&staged, &path).unwrap();

        let mut restarted = NativeEditor::open_or_create(path.clone()).unwrap();
        assert_eq!(
            restarted.editor.document().blocks,
            vec![Block::plain("Staged edit")]
        );
        restarted.save();
        assert!(restarted.status.starts_with("Saved"));
        assert!(!path.with_extension("wstrike.recovery").exists());
        assert_eq!(
            open_document(&backup).unwrap().document().blocks,
            vec![Block::plain("Original")]
        );
        assert!(staged.exists());
    }

    #[test]
    fn missing_document_with_unrecorded_backup_does_not_open_blank() {
        let root = std::env::temp_dir().join(format!("wordstrike-backup-{}", Uuid::new_v4()));
        fs::create_dir(&root).unwrap();
        let path = root.join("document.wstrike");
        let mut app = NativeEditor::open_or_create(path.clone()).unwrap();
        app.save();
        let backup = path.with_extension("wstrike.previous");
        fs::rename(&path, &backup).unwrap();
        let error = NativeEditor::open_or_create(path).err().unwrap();
        assert!(error.contains("recovery copies exist"));
        assert!(error.contains(&backup.display().to_string()));
    }
}
