use super::*;
use crate::{Block, Command};

fn editor(text: &str) -> Editor {
    let mut editor = Editor::new();
    editor
        .apply(Command::Insert {
            at: 0,
            block: Block::plain(text),
        })
        .unwrap();
    editor
}

fn replace(editor: &mut Editor, text: &str) {
    editor
        .apply(Command::Replace {
            at: 0,
            block: Block::plain(text),
        })
        .unwrap();
}

fn assert_text(path: &Path, text: &str) {
    assert_eq!(
        open_document(path).unwrap().document().blocks,
        vec![Block::plain(text)]
    );
}

fn staged_files(path: &Path) -> Vec<PathBuf> {
    let prefix = format!("{}.", path.file_name().unwrap().to_string_lossy());
    fs::read_dir(path.parent().unwrap())
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|candidate| {
            let name = candidate.file_name().unwrap().to_string_lossy();
            name.starts_with(&prefix) && name.ends_with(".tmp")
        })
        .collect()
}

fn pending(path: &Path) -> (PathBuf, PathBuf) {
    save_document(&mut editor("Original"), path).unwrap();
    let staged = path.with_extension("wstrike.resume.tmp");
    save_document(&mut editor("Staged edit"), &staged).unwrap();
    let backup = previous(path);
    write_recovery(
        path,
        &backup,
        &staged,
        file_digest(path).unwrap(),
        file_digest(&staged).unwrap(),
    )
    .unwrap();
    (backup, staged)
}

#[test]
fn failed_first_publication_keeps_staged_archive_and_editor_unbound() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("document.wstrike");
    let mut editor = editor("Unsaved work");
    let error = save_document_with(&mut editor, &path, |_, _, _| {
        Err("injected publication failure".into())
    })
    .unwrap_err();
    assert!(error.contains("injected publication failure"), "{error}");
    assert!(!path.exists());
    assert!(editor.disk_baseline.is_none());
    assert!(!recovery_path(&path).exists());
    let staged = staged_files(&path);
    assert_eq!(staged.len(), 1);
    assert_text(&staged[0], "Unsaved work");
    assert!(error.contains(&staged[0].display().to_string()), "{error}");
    save_document(&mut editor, &dir.path().join("copy.wstrike")).unwrap();
}

#[test]
fn failed_replacement_restores_original_and_retains_independent_backup_and_staged_edit() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("document.wstrike");
    let mut editor = editor("Original");
    save_document(&mut editor, &path).unwrap();
    let baseline = editor.disk_baseline.clone();
    let original = fs::read(&path).unwrap();
    replace(&mut editor, "Local edit");
    let error = save_document_with(&mut editor, &path, |_, target, _| {
        assert!(!target.exists());
        Err("injected publication failure".into())
    })
    .unwrap_err();
    assert!(error.contains("injected publication failure"), "{error}");
    assert!(error.contains("original restored"), "{error}");
    assert_eq!(fs::read(&path).unwrap(), original);
    assert_eq!(fs::read(previous(&path)).unwrap(), original);
    assert_eq!(editor.disk_baseline, baseline);
    assert!(!recovery_path(&path).exists());
    let staged = staged_files(&path);
    assert_eq!(staged.len(), 1);
    assert_text(&staged[0], "Local edit");
    fs::write(&path, b"External edit after rollback").unwrap();
    assert_eq!(fs::read(previous(&path)).unwrap(), original);
    assert_text(&staged[0], "Local edit");
}

#[test]
fn racing_first_save_is_not_overwritten_and_its_staged_edit_is_retained() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("document.wstrike");
    let mut editor = editor("Local work");
    let error = save_document_with(&mut editor, &path, |source, target, digest| {
        fs::write(target, b"External writer").unwrap();
        publish_archive(source, target, digest)
    })
    .unwrap_err();
    assert!(error.contains("without replacing another file"), "{error}");
    assert_eq!(fs::read(&path).unwrap(), b"External writer");
    assert!(editor.disk_baseline.is_none());
    let staged = staged_files(&path);
    assert_eq!(staged.len(), 1);
    assert_text(&staged[0], "Local work");
}

#[test]
fn racing_replacement_blocks_rollback_and_keeps_recovery_actionable() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("document.wstrike");
    let mut editor = editor("Original");
    save_document(&mut editor, &path).unwrap();
    let baseline = editor.disk_baseline.clone();
    replace(&mut editor, "Local edit");
    let error = save_document_with(&mut editor, &path, |source, target, digest| {
        fs::write(target, b"External writer").unwrap();
        publish_archive(source, target, digest)
    })
    .unwrap_err();
    assert!(error.contains("main path is occupied"), "{error}");
    assert_eq!(fs::read(&path).unwrap(), b"External writer");
    assert_eq!(editor.disk_baseline, baseline);
    assert_text(&previous(&path), "Original");
    let record = read_recovery(&path).unwrap().unwrap();
    let staged = recovery_sibling(&path, &record.staged);
    assert_text(&staged, "Local edit");
    assert!(
        pending_pre_move_document(&path)
            .unwrap_err()
            .contains("changed outside")
    );
    assert!(recover_missing_document(&path).is_err());
    assert!(
        save_document(&mut editor, &path)
            .unwrap_err()
            .contains("changed outside")
    );
    assert_eq!(fs::read(&path).unwrap(), b"External writer");
}

#[test]
fn changed_archive_is_refused_before_publication() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("document.wstrike");
    let mut editor = editor("Local edit");
    let error = save_document_with(&mut editor, &path, |source, target, digest| {
        fs::write(source, b"Changed staged bytes").unwrap();
        publish_archive(source, target, digest)
    })
    .unwrap_err();
    assert!(
        error.contains("archive changed while preparing publication"),
        "{error}"
    );
    assert!(!path.exists());
    assert!(editor.disk_baseline.is_none());
    let staged = staged_files(&path);
    assert_eq!(staged.len(), 1);
    assert_eq!(fs::read(&staged[0]).unwrap(), b"Changed staged bytes");
}

#[test]
fn resume_keeps_original_and_staged_archives_independent_from_main() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("document.wstrike");
    let (backup, staged) = pending(&path);
    assert_eq!(
        pending_pre_move_document(&path).unwrap(),
        Some(staged.clone())
    );
    assert_eq!(resume_pre_move_document(&path).unwrap(), staged);
    assert_text(&path, "Staged edit");
    assert_text(&staged, "Staged edit");
    assert_text(&backup, "Original");
    assert!(!recovery_path(&path).exists());
    fs::write(&path, b"External edit after resume").unwrap();
    assert_text(&staged, "Staged edit");
    assert_text(&backup, "Original");
}

#[test]
fn failed_resume_restores_main_and_retains_record_backup_and_staged_archive() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("document.wstrike");
    let (backup, staged) = pending(&path);
    let error = resume_pre_move_with(&path, |_, target, _| {
        assert!(!target.exists());
        Err("injected recovery publication failure".into())
    })
    .unwrap_err();
    assert!(
        error.contains("injected recovery publication failure"),
        "{error}"
    );
    assert!(error.contains("original restored"), "{error}");
    assert_text(&path, "Original");
    assert_text(&backup, "Original");
    assert_text(&staged, "Staged edit");
    assert!(recovery_path(&path).exists());
    // An occupied backup requires review; the record must never be silently discarded.
    assert!(
        pending_pre_move_document(&path)
            .unwrap_err()
            .contains("backup already exists")
    );
}

#[test]
fn a_racing_main_during_resume_retains_every_copy_and_refuses_further_recovery() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("document.wstrike");
    let (backup, staged) = pending(&path);
    let error = resume_pre_move_with(&path, |source, target, digest| {
        fs::write(target, b"Racing main").unwrap();
        publish_archive(source, target, digest)
    })
    .unwrap_err();
    assert!(error.contains("main path is occupied"), "{error}");
    assert_eq!(fs::read(&path).unwrap(), b"Racing main");
    assert_text(&backup, "Original");
    assert_text(&staged, "Staged edit");
    assert!(recovery_path(&path).exists());
    assert!(
        resume_pre_move_document(&path)
            .unwrap_err()
            .contains("changed outside")
    );
    assert_eq!(fs::read(&path).unwrap(), b"Racing main");
}

#[test]
fn recorded_missing_main_can_be_recovered_without_consuming_either_archive() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("document.wstrike");
    let (backup, staged) = pending(&path);
    persistence::move_noclobber(&path, &backup).unwrap();
    let (mut recovered, found) = recover_missing_document(&path).unwrap().unwrap();
    assert_eq!(found, backup);
    assert_eq!(recovered.document().blocks, vec![Block::plain("Original")]);
    save_document(&mut recovered, &path).unwrap();
    assert_text(&path, "Original");
    assert_text(&backup, "Original");
    assert_text(&staged, "Staged edit");
    assert!(!recovery_path(&path).exists());
}
