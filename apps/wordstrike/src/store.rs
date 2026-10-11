//! Bounded editable `.wstrike` archive. Layout/PDF bytes are derived, never saved here.

use std::collections::{BTreeMap, BTreeSet};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;
use zip::write::SimpleFileOptions;
use zip::{CompressionMethod, ZipArchive, ZipWriter};

use crate::model::{State, validate, validate_parts};
use crate::{Document, Editor};

const MAX_ARCHIVE: u64 = 64 * 1024 * 1024;
const MAX_JSON: usize = 20 * 1024 * 1024;
const MAX_ASSET: usize = 10 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct Manifest {
    version: u32,
    history: Vec<State>,
    cursor: usize,
    assets: Vec<String>,
}

#[derive(Serialize, Deserialize)]
struct RecoveryRecord {
    version: u32,
    backup: String,
    staged: String,
    #[serde(default)]
    original_digest: Option<[u8; 32]>,
    published_digest: [u8; 32],
}

fn hash(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

fn previous(path: &Path) -> std::path::PathBuf {
    path.with_extension("wstrike.previous")
}

fn recovery_path(path: &Path) -> PathBuf {
    path.with_extension("wstrike.recovery")
}

fn read_recovery(path: &Path) -> Result<Option<RecoveryRecord>, String> {
    let marker = recovery_path(path);
    if !path_present(&marker) {
        return Ok(None);
    }
    if fs::metadata(&marker).map_err(|e| e.to_string())?.len() > 4096 {
        return Err(format!(
            "recovery record is too large: {}",
            marker.display()
        ));
    }
    let record: RecoveryRecord =
        serde_json::from_slice(&fs::read(&marker).map_err(|e| e.to_string())?)
            .map_err(|e| format!("invalid recovery record {}: {e}", marker.display()))?;
    let backup_prefix = previous(path)
        .file_name()
        .ok_or("document filename is missing")?
        .to_string_lossy()
        .into_owned();
    let staged_prefix = format!(
        "{}.",
        path.file_name()
            .ok_or("document filename is missing")?
            .to_string_lossy()
    );
    if !matches!(record.version, 1 | 2)
        || (record.version == 2 && record.original_digest.is_none())
        || !safe_recovery_name(&record.backup)
        || !safe_recovery_name(&record.staged)
        || !(record.backup == backup_prefix
            || record.backup.starts_with(&format!("{backup_prefix}.")))
        || !record.staged.starts_with(&staged_prefix)
        || !record.staged.ends_with(".tmp")
    {
        return Err(format!("invalid recovery record {}", marker.display()));
    }
    Ok(Some(record))
}

fn safe_recovery_name(name: &str) -> bool {
    !name.is_empty()
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains(':')
        && !name.contains('\0')
        && name != "."
        && name != ".."
}

fn resolve_stale_recovery(path: &Path) -> Result<(), String> {
    if let Some(record) = read_recovery(path)? {
        if path_present(path) && file_digest(path).ok() == Some(record.published_digest) {
            fs::remove_file(recovery_path(path)).map_err(|e| e.to_string())?;
        } else {
            if path_present(path)
                && record
                    .original_digest
                    .is_some_and(|digest| file_digest(path).ok() != Some(digest))
            {
                return Err(format!(
                    "document changed outside WordStrike during interrupted save; main file at {}, staged copy at {}. Use Save copy after review",
                    path.display(),
                    recovery_sibling(path, &record.staged).display(),
                ));
            }
            return Err(format!(
                "an interrupted document save needs Resume or Abandon before Save; staged copy at {}, recovery record at {}",
                recovery_sibling(path, &record.staged).display(),
                recovery_path(path).display(),
            ));
        }
    }
    Ok(())
}

fn recovery_sibling(path: &Path, name: &str) -> PathBuf {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
        .join(name)
}

/// A record with the original still at the main path is an actionable pre-move save.
/// The staged archive remains available even if another process edits the main file.
pub(crate) fn pending_pre_move_document(path: &Path) -> Result<Option<PathBuf>, String> {
    let Some(record) = read_recovery(path)? else {
        return Ok(None);
    };
    if file_digest(path).ok() == Some(record.published_digest) {
        return Ok(None);
    }
    let Some(original_digest) = record.original_digest else {
        return Err(format!(
            "legacy recovery record at {} needs manual review",
            recovery_path(path).display()
        ));
    };
    let staged = recovery_sibling(path, &record.staged);
    if file_digest(&staged).ok() != Some(record.published_digest) {
        return Err(format!(
            "staged document changed or is missing at {}; inspect it before recovery",
            staged.display()
        ));
    }
    if file_digest(path).ok() != Some(original_digest) {
        return Err(format!(
            "document changed outside WordStrike during interrupted save; original and staged copy retained at {} and {}",
            path.display(),
            staged.display()
        ));
    }
    let backup = recovery_sibling(path, &record.backup);
    if path_present(&backup) {
        return Err(format!(
            "recovery backup already exists at {}; inspect it before resuming",
            backup.display()
        ));
    }
    Ok(Some(staged))
}

pub(crate) fn resume_pre_move_document(path: &Path) -> Result<PathBuf, String> {
    let staged =
        pending_pre_move_document(path)?.ok_or("no interrupted document save can be resumed")?;
    open_document(&staged)
        .map_err(|e| format!("staged document is invalid at {}: {e}", staged.display()))?;
    let record = read_recovery(path)?.ok_or("recovery record disappeared")?;
    let original = record
        .original_digest
        .ok_or("recovery record has no original digest")?;
    let backup = recovery_sibling(path, &record.backup);
    fs::rename(path, &backup).map_err(|e| {
        format!(
            "could not preserve original document at {}: {e}",
            backup.display()
        )
    })?;
    if file_digest(&backup).ok() != Some(original)
        || file_digest(&staged).ok() != Some(record.published_digest)
    {
        if !path_present(path) {
            let _ = fs::hard_link(&backup, path);
        }
        return Err(format!(
            "document changed during recovery; original at {}, staged copy at {}",
            backup.display(),
            staged.display()
        ));
    }
    if let Err(error) = fs::hard_link(&staged, path) {
        if !path_present(path) {
            let _ = fs::hard_link(&backup, path);
        }
        return Err(format!(
            "could not publish staged document: {error}; original at {}, staged copy at {}",
            backup.display(),
            staged.display()
        ));
    }
    let _ = fs::remove_file(recovery_path(path));
    Ok(staged)
}

pub(crate) fn abandon_pre_move_document(path: &Path) -> Result<PathBuf, String> {
    let staged =
        pending_pre_move_document(path)?.ok_or("no interrupted document save can be abandoned")?;
    fs::remove_file(recovery_path(path))
        .map_err(|e| format!("cannot clear recovery record: {e}"))?;
    Ok(staged)
}

fn write_recovery(
    path: &Path,
    backup: &Path,
    staged: &Path,
    original_digest: [u8; 32],
    published_digest: [u8; 32],
) -> Result<(), String> {
    let marker = recovery_path(path);
    let record = RecoveryRecord {
        version: 2,
        backup: backup
            .file_name()
            .ok_or("backup filename is missing")?
            .to_string_lossy()
            .into_owned(),
        staged: staged
            .file_name()
            .ok_or("staged filename is missing")?
            .to_string_lossy()
            .into_owned(),
        original_digest: Some(original_digest),
        published_digest,
    };
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&marker)
        .map_err(|e| format!("cannot create recovery record {}: {e}", marker.display()))?;
    file.write_all(&serde_json::to_vec(&record).map_err(|e| e.to_string())?)
        .and_then(|()| file.sync_all())
        .map_err(|e| format!("cannot sync recovery record {}: {e}", marker.display()))?;
    Ok(())
}

pub(crate) fn recover_missing_document(path: &Path) -> Result<Option<(Editor, PathBuf)>, String> {
    if path_present(path) {
        return Err("document path is present; do not recover over it".into());
    }
    if let Some(record) = read_recovery(path)? {
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        let backup = parent.join(record.backup);
        if !path_present(&backup) {
            return Err(format!(
                "document save was interrupted; backup is missing at {}. Staged file: {}",
                backup.display(),
                parent.join(record.staged).display()
            ));
        }
        if record
            .original_digest
            .is_some_and(|digest| file_digest(&backup).ok() != Some(digest))
        {
            return Err(format!(
                "document recovery backup changed outside WordStrike: {}",
                backup.display()
            ));
        }
        let mut editor = open_document(&backup)?;
        editor.disk_baseline = None;
        editor.recovery_backup = Some(backup.clone());
        return Ok(Some((editor, backup)));
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let prefix = previous(path)
        .file_name()
        .ok_or("document filename is missing")?
        .to_string_lossy()
        .into_owned();
    let backups: Vec<_> = fs::read_dir(parent)
        .map_err(|e| e.to_string())?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|candidate| {
            candidate.file_name().is_some_and(|name| {
                let name = name.to_string_lossy();
                name.as_ref() == prefix || name.starts_with(&format!("{prefix}."))
            })
        })
        .collect();
    if !backups.is_empty() {
        return Err(format!(
            "document is missing; recovery copies exist at {}. Choose a copy before creating a new document",
            backups
                .iter()
                .map(|p| p.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        ));
    }
    Ok(None)
}

fn target_path(path: &Path) -> Result<PathBuf, String> {
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path.file_name().ok_or("document path needs a filename")?;
    Ok(fs::canonicalize(parent)
        .map_err(|e| e.to_string())?
        .join(name))
}

fn file_digest(path: &Path) -> Result<[u8; 32], String> {
    let mut file = File::open(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    Ok(digest.finalize().into())
}

fn path_present(path: &Path) -> bool {
    match fs::symlink_metadata(path) {
        Ok(_) => true,
        Err(error) => error.kind() != std::io::ErrorKind::NotFound,
    }
}

fn checked_existing(path: &Path, expected: Option<[u8; 32]>) -> Result<(), String> {
    match expected {
        Some(digest) if file_digest(path).ok() == Some(digest) => Ok(()),
        None if !path_present(path) => Ok(()),
        _ => Err(format!(
            "{} changed outside WordStrike; reopen to review it or use Save copy",
            path.display()
        )),
    }
}

fn available_backup(path: &Path) -> PathBuf {
    let first = previous(path);
    if !path_present(&first) {
        first
    } else {
        path.with_extension(format!("wstrike.previous.{}", Uuid::new_v4()))
    }
}

pub fn save_document(editor: &mut Editor, path: &Path) -> Result<(), String> {
    validate(&editor.document)?;
    if editor.history.is_empty()
        || editor.history.len() > 100
        || editor.cursor >= editor.history.len()
    {
        return Err("document undo history is invalid".into());
    }
    let current = State {
        blocks: editor.document.blocks.clone(),
        page_setup: editor.document.page_setup,
    };
    if editor.history[editor.cursor] != current {
        return Err("document history disagrees with current content".into());
    }
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    if !parent.is_dir() {
        return Err("document parent directory does not exist".into());
    }
    let target = target_path(path)?;
    let restoring = if let Some(backup) = &editor.recovery_backup {
        let record =
            read_recovery(path)?.ok_or("recovery record is missing; save a separate copy")?;
        let parent = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .unwrap_or_else(|| Path::new("."));
        if path_present(path) || parent.join(record.backup) != *backup {
            return Err("recovery source changed; save a separate copy".into());
        }
        true
    } else {
        resolve_stale_recovery(path)?;
        false
    };
    let expected = match &editor.disk_baseline {
        Some((bound, digest)) if *bound == target => Some(*digest),
        Some(_) => {
            return Err(
                "this editor is bound to another file; create a separate document copy".into(),
            );
        }
        None => None,
    };
    checked_existing(path, expected)?;
    let manifest = Manifest {
        version: 1,
        history: editor.history.clone(),
        cursor: editor.cursor,
        assets: editor.document.assets.keys().cloned().collect(),
    };
    let manifest_json = serde_json::to_vec(&manifest).map_err(|e| e.to_string())?;
    let document_json = serde_json::to_vec(&current).map_err(|e| e.to_string())?;
    if manifest_json.len() > MAX_JSON || document_json.len() > MAX_JSON {
        return Err("document metadata exceeds 20 MiB".into());
    }
    let expanded = editor.document.assets.values().fold(
        (manifest_json.len() + document_json.len()) as u64,
        |total, bytes| total.saturating_add(bytes.len() as u64),
    );
    if expanded > MAX_ARCHIVE {
        return Err("document contents exceed the 64 MiB reopen limit".into());
    }
    let temp = path.with_extension(format!("wstrike.{}.tmp", Uuid::new_v4()));
    let file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)
        .map_err(|e| e.to_string())?;
    let staged = (|| -> Result<(), String> {
        let mut zip = ZipWriter::new(file);
        let options = SimpleFileOptions::default().compression_method(CompressionMethod::Deflated);
        for (name, bytes) in [
            ("manifest.json", manifest_json.as_slice()),
            ("document.json", document_json.as_slice()),
        ] {
            zip.start_file(name, options).map_err(|e| e.to_string())?;
            zip.write_all(bytes).map_err(|e| e.to_string())?;
        }
        for (hash, bytes) in &editor.document.assets {
            zip.start_file(format!("assets/{hash}.bin"), options)
                .map_err(|e| e.to_string())?;
            zip.write_all(bytes).map_err(|e| e.to_string())?;
        }
        zip.finish()
            .map_err(|e| e.to_string())?
            .sync_all()
            .map_err(|e| e.to_string())?;
        Ok(())
    })();
    if let Err(error) = staged {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    if fs::metadata(&temp).map_err(|e| e.to_string())?.len() > MAX_ARCHIVE {
        let _ = fs::remove_file(&temp);
        return Err("document ZIP exceeds the 64 MiB reopen limit".into());
    }
    let staged_digest = file_digest(&temp)?;
    checked_existing(path, expected)
        .map_err(|error| format!("{error}; staged copy retained at {}", temp.display()))?;
    let backup = if let Some(digest) = expected {
        let backup = available_backup(path);
        write_recovery(path, &backup, &temp, digest, staged_digest)?;
        fs::rename(path, &backup).map_err(|e| {
            let _ = fs::remove_file(recovery_path(path));
            format!(
                "could not preserve current document; staged copy at {}: {e}",
                temp.display()
            )
        })?;
        if file_digest(&backup).ok() != Some(digest) {
            if !path_present(path) {
                let _ = fs::hard_link(&backup, path);
            }
            return Err(format!(
                "document changed during save; external original retained at {}, staged copy at {}",
                backup.display(),
                temp.display()
            ));
        }
        Some(backup)
    } else {
        None
    };
    if let Err(error) = fs::hard_link(&temp, path) {
        if let Some(backup) = &backup {
            if !path_present(path) {
                let _ = fs::hard_link(backup, path);
            }
        }
        if expected.is_some() && file_digest(path).ok() == expected {
            let _ = fs::remove_file(recovery_path(path));
        }
        return Err(format!(
            "could not publish document without replacing another file: {error}; staged copy at {}{}",
            temp.display(),
            backup
                .as_ref()
                .map(|p| format!(", previous file at {}", p.display()))
                .unwrap_or_default()
        ));
    }
    let _ = fs::remove_file(&temp);
    if backup.is_some() || restoring {
        let _ = fs::remove_file(recovery_path(path));
    }
    editor.disk_baseline = Some((target, staged_digest));
    editor.recovery_backup = None;
    Ok(())
}

pub fn open_document(path: &Path) -> Result<Editor, String> {
    let baseline = file_digest(path)?;
    let size = fs::metadata(path).map_err(|e| e.to_string())?.len();
    if size > MAX_ARCHIVE {
        return Err("document archive exceeds 64 MiB".into());
    }
    let mut zip =
        ZipArchive::new(File::open(path).map_err(|e| e.to_string())?).map_err(|e| e.to_string())?;
    if zip.len() > 102 {
        return Err("document archive has too many entries".into());
    }
    let mut names = BTreeSet::new();
    let mut manifest = None;
    let mut current = None;
    let mut assets = BTreeMap::new();
    let mut expanded = 0u64;
    for index in 0..zip.len() {
        let mut entry = zip.by_index(index).map_err(|e| e.to_string())?;
        let name = entry.name().map_err(|e| e.to_string())?.to_string();
        if !names.insert(name.clone())
            || name.contains('\\')
            || name.starts_with('/')
            || name.split('/').any(|s| s == "..")
        {
            return Err("unsafe document ZIP entry".into());
        }
        let limit = if name == "manifest.json" || name == "document.json" {
            MAX_JSON
        } else {
            MAX_ASSET
        };
        expanded = expanded.saturating_add(entry.size());
        if expanded > MAX_ARCHIVE || entry.size() > limit as u64 {
            return Err("document ZIP entry exceeds limit".into());
        }
        let mut bytes = Vec::new();
        entry
            .take((limit + 1) as u64)
            .read_to_end(&mut bytes)
            .map_err(|e| e.to_string())?;
        if bytes.len() > limit {
            return Err("document ZIP entry expanded beyond limit".into());
        }
        if name == "manifest.json" {
            manifest = Some(bytes);
        } else if name == "document.json" {
            current = Some(bytes);
        } else if let Some(key) = name
            .strip_prefix("assets/")
            .and_then(|s| s.strip_suffix(".bin"))
        {
            if key.len() != 64
                || !key
                    .bytes()
                    .all(|b| b.is_ascii_hexdigit() && !b.is_ascii_uppercase())
                || hash(&bytes) != key
            {
                return Err("invalid document asset hash".into());
            }
            assets.insert(key.to_string(), bytes);
        } else {
            return Err("unexpected document ZIP entry".into());
        }
    }
    let manifest: Manifest = serde_json::from_slice(&manifest.ok_or("missing document manifest")?)
        .map_err(|e| e.to_string())?;
    let current: State = serde_json::from_slice(&current.ok_or("missing document content")?)
        .map_err(|e| e.to_string())?;
    if manifest.version != 1 {
        return Err("unsupported document version".into());
    }
    if manifest.assets != assets.keys().cloned().collect::<Vec<_>>()
        || manifest.history.is_empty()
        || manifest.history.len() > 100
        || manifest.cursor >= manifest.history.len()
        || manifest.history[manifest.cursor] != current
    {
        return Err("document asset inventory or history is inconsistent".into());
    }
    for state in &manifest.history {
        validate_parts(&state.blocks, state.page_setup, &assets)?;
    }
    let document = Document {
        blocks: current.blocks,
        page_setup: current.page_setup,
        assets,
    };
    validate(&document)?;
    if file_digest(path)? != baseline {
        return Err("document changed while opening; reopen the latest file".into());
    }
    Ok(Editor {
        document,
        history: manifest.history,
        cursor: manifest.cursor,
        disk_baseline: Some((target_path(path)?, baseline)),
        recovery_backup: None,
    })
}
