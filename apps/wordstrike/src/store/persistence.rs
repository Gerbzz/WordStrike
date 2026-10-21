//! No-clobber publication for legacy archives; recovery sources remain independent.
use std::fs::File;
use std::io;
use std::path::{Path, PathBuf};

fn absolute_sibling(path: &Path) -> io::Result<PathBuf> {
    let name = path
        .file_name()
        .ok_or_else(|| io::Error::other("document filename is missing"))?;
    let parent = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    // Resolve only the parent, preserving the final directory entry. On Windows this
    // supplies verbatim paths to tempfile's raw Win32 calls as well as our backup move.
    Ok(std::fs::canonicalize(parent)?.join(name))
}

/// Copy into an owned sibling, then publish its complete, synced bytes without replacement.
/// Keep `source` independent and available on both success and failure (Resume/rollback).
/// The caller verifies the prepared bytes against the archive digest before publication.
pub(super) fn publish(
    source: &Path,
    destination: &Path,
    verify: impl FnOnce(&Path) -> io::Result<()>,
) -> io::Result<()> {
    let mut source = File::open(source)?;
    publish_with(
        destination,
        |file| io::copy(&mut source, file).map(|_| ()),
        verify,
    )
}

fn publish_with(
    destination: &Path,
    prepare: impl FnOnce(&mut File) -> io::Result<()>,
    verify: impl FnOnce(&Path) -> io::Result<()>,
) -> io::Result<()> {
    let destination = absolute_sibling(destination)?;
    let parent = destination
        .parent()
        .ok_or_else(|| io::Error::other("document parent is missing"))?;
    let mut temp = tempfile::Builder::new()
        .prefix(".wordstrike-publish-")
        .tempfile_in(parent)?;
    prepare(temp.as_file_mut())?;
    temp.as_file().sync_all()?;
    // Close before renaming on Windows. Only this disposable candidate is cleaned on failure.
    let temp = temp.into_temp_path();
    verify(&temp)?;
    temp.persist_noclobber(&destination)
        .map_err(|error| error.error)
}

/// Preserve the original under an unused backup name. Never delete it on failure.
/// Use only an atomic no-replace rename for a user's original. A hard-link/unlink
/// fallback could delete an external writer's replacement between the two operations.
/// Like the previous store, directory/power-loss durability is not guaranteed.
#[cfg(any(
    target_os = "linux",
    target_os = "android",
    target_vendor = "apple",
    target_os = "redox"
))]
pub(super) fn move_noclobber(source: &Path, destination: &Path) -> io::Result<()> {
    use rustix::fs::{CWD, RenameFlags, renameat_with};
    renameat_with(CWD, source, CWD, destination, RenameFlags::NOREPLACE).map_err(Into::into)
}

#[cfg(not(any(
    windows,
    target_os = "linux",
    target_os = "android",
    target_vendor = "apple",
    target_os = "redox"
)))]
pub(super) fn move_noclobber(_source: &Path, _destination: &Path) -> io::Result<()> {
    Err(io::Error::new(
        io::ErrorKind::Unsupported,
        "atomic no-clobber backup moves are unavailable on this platform; original retained",
    ))
}

#[cfg(windows)]
pub(super) fn move_noclobber(source: &Path, destination: &Path) -> io::Result<()> {
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::MoveFileExW;

    fn wide(path: &Path) -> io::Result<Vec<u16>> {
        let mut path: Vec<_> = absolute_sibling(path)?.as_os_str().encode_wide().collect();
        if path.contains(&0) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "document path contains NUL",
            ));
        }
        path.push(0);
        Ok(path)
    }

    let source = wide(source)?;
    let destination = wide(destination)?;
    // Do not wrap a user's original in TempPath on Windows: tempfile's persistence changes
    // file attributes. No REPLACE_EXISTING or COPY_ALLOWED flags; no destination is clobbered,
    // and a cross-volume move fails rather than falling back to a partially visible copy.
    // SAFETY: both buffers are NUL-terminated UTF-16, live through this synchronous call,
    // and MoveFileExW does not retain either pointer.
    if unsafe { MoveFileExW(source.as_ptr(), destination.as_ptr(), 0) } == 0 {
        Err(io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::io::Write;
    use std::sync::{Arc, Barrier};

    #[test]
    fn publication_retains_an_independent_recovery_source() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("staged.tmp");
        let target = dir.path().join("document.wstrike");
        fs::write(&source, b"Complete archive").unwrap();
        publish(&source, &target, |_| Ok(())).unwrap();
        assert_eq!(fs::read(&source).unwrap(), b"Complete archive");
        fs::write(&target, b"External change").unwrap();
        assert_eq!(fs::read(&source).unwrap(), b"Complete archive");
        assert_eq!(fs::read(&target).unwrap(), b"External change");
    }

    #[test]
    fn a_racing_backup_is_never_replaced() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("document.wstrike");
        let backup = dir.path().join("document.wstrike.previous");
        fs::write(&source, b"Original archive").unwrap();
        assert!(!backup.exists());
        fs::write(&backup, b"Racing backup").unwrap();
        assert!(move_noclobber(&source, &backup).is_err());
        assert_eq!(fs::read(&source).unwrap(), b"Original archive");
        assert_eq!(fs::read(&backup).unwrap(), b"Racing backup");
    }

    #[test]
    fn publication_failure_preserves_both_sources_and_neighbours() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("staged.tmp");
        let target = dir.path().join("document.wstrike");
        fs::write(&source, b"Staged archive").unwrap();
        fs::write(&target, b"External archive").unwrap();
        assert!(publish(&source, &target, |_| Ok(())).is_err());
        assert_eq!(fs::read(&source).unwrap(), b"Staged archive");
        assert_eq!(fs::read(&target).unwrap(), b"External archive");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn missing_source_and_directory_destination_leave_neighbours_intact() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("staged.tmp");
        let target = dir.path().join("document.wstrike");
        fs::create_dir(&target).unwrap();
        fs::write(target.join("keep"), b"Keep neighbour").unwrap();
        assert!(publish(&source, &target, |_| Ok(())).is_err());
        fs::write(&source, b"Staged archive").unwrap();
        assert!(publish(&source, &target, |_| Ok(())).is_err());
        assert_eq!(fs::read(&source).unwrap(), b"Staged archive");
        assert_eq!(fs::read(target.join("keep")).unwrap(), b"Keep neighbour");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn concurrent_publications_have_one_complete_winner_and_keep_both_sources() {
        let dir = tempfile::tempdir().unwrap();
        let target = dir.path().join("document.wstrike");
        let barrier = Arc::new(Barrier::new(2));
        let mut writers = Vec::new();
        let archives = [vec![b'a'; 128 * 1024], vec![b'b'; 128 * 1024]];
        for (index, bytes) in archives.iter().enumerate() {
            let source = dir.path().join(format!("staged-{index}.tmp"));
            fs::write(&source, bytes).unwrap();
            let target = target.clone();
            let barrier = barrier.clone();
            writers.push(std::thread::spawn(move || {
                barrier.wait();
                publish(&source, &target, |_| Ok(()))
            }));
        }
        let results: Vec<_> = writers
            .into_iter()
            .map(|writer| writer.join().unwrap())
            .collect();
        assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
        let published = fs::read(&target).unwrap();
        assert!(archives.contains(&published));
        for (index, bytes) in archives.iter().enumerate() {
            assert_eq!(
                fs::read(dir.path().join(format!("staged-{index}.tmp"))).unwrap(),
                *bytes
            );
        }
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 3);
    }

    #[test]
    fn partial_and_complete_preparation_failures_keep_original_and_remove_only_candidate() {
        for bytes in [b"Partial".as_slice(), b"Complete archive".as_slice()] {
            let dir = tempfile::tempdir().unwrap();
            let target = dir.path().join("document.wstrike");
            fs::write(&target, b"Original archive").unwrap();
            let error = publish_with(
                &target,
                |file| {
                    file.write_all(bytes)?;
                    Err(io::Error::other("injected preparation failure"))
                },
                |_| panic!("must not verify failed preparation"),
            )
            .unwrap_err();
            assert!(error.to_string().contains("injected preparation failure"));
            assert_eq!(fs::read(&target).unwrap(), b"Original archive");
            assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
        }
    }

    #[test]
    fn verification_failure_retains_source_without_publishing() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("staged.tmp");
        let target = dir.path().join("document.wstrike");
        fs::write(&source, b"Changed staged archive").unwrap();
        let error = publish(&source, &target, |prepared| {
            assert_eq!(fs::read(prepared)?, b"Changed staged archive");
            Err(io::Error::other("archive digest changed"))
        })
        .unwrap_err();
        assert!(error.to_string().contains("archive digest changed"));
        assert!(!target.exists());
        assert_eq!(fs::read(&source).unwrap(), b"Changed staged archive");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn destination_appearing_after_preparation_blocks_publication() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("staged.tmp");
        let target = dir.path().join("document.wstrike");
        fs::write(&source, b"Staged archive").unwrap();
        assert!(!target.exists());
        assert!(
            publish(&source, &target, |_| {
                fs::write(&target, b"Racing external archive")
            })
            .is_err()
        );
        assert_eq!(fs::read(&source).unwrap(), b"Staged archive");
        assert_eq!(fs::read(&target).unwrap(), b"Racing external archive");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[test]
    fn successful_backup_move_keeps_original_bytes() {
        let dir = tempfile::tempdir().unwrap();
        let source = dir.path().join("document.wstrike");
        let backup = dir.path().join("document.wstrike.previous");
        fs::write(&source, b"Original archive").unwrap();
        move_noclobber(&source, &backup).unwrap();
        assert!(!source.exists());
        assert_eq!(fs::read(&backup).unwrap(), b"Original archive");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 1);
    }

    #[test]
    fn failed_rollback_does_not_replace_a_racing_main_or_consume_backup() {
        let dir = tempfile::tempdir().unwrap();
        let backup = dir.path().join("document.wstrike.previous");
        let target = dir.path().join("document.wstrike");
        fs::write(&backup, b"Original archive").unwrap();
        assert!(
            publish(&backup, &target, |_| {
                fs::write(&target, b"Racing main archive")
            })
            .is_err()
        );
        assert_eq!(fs::read(&target).unwrap(), b"Racing main archive");
        assert_eq!(fs::read(&backup).unwrap(), b"Original archive");
        assert_eq!(fs::read_dir(dir.path()).unwrap().count(), 2);
    }

    #[cfg(windows)]
    #[test]
    fn long_windows_paths_support_publication_backup_and_independent_rollback() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir
            .path()
            .join("long-parent-segment-".repeat(8))
            .join("nested-segment-".repeat(8));
        fs::create_dir_all(&nested).unwrap();
        let source = nested.join("staged.tmp");
        let target = nested.join("document.wstrike");
        let backup = nested.join("document.wstrike.previous");
        assert!(target.as_os_str().len() > 260);
        fs::write(&source, b"Complete archive").unwrap();
        publish(&source, &target, |candidate| {
            // No executable manifest/registry opt-in is needed for either Win32 path.
            assert!(candidate.as_os_str().to_string_lossy().starts_with(r"\\?\"));
            Ok(())
        })
        .unwrap();
        move_noclobber(&target, &backup).unwrap();
        assert!(!target.exists());
        publish(&backup, &target, |_| Ok(())).unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"Complete archive");
        fs::write(&target, b"External edit").unwrap();
        assert_eq!(fs::read(&source).unwrap(), b"Complete archive");
        assert_eq!(fs::read(&backup).unwrap(), b"Complete archive");
        assert_eq!(fs::read_dir(&nested).unwrap().count(), 3);
    }

    #[cfg(any(
        target_os = "linux",
        target_os = "android",
        target_vendor = "apple",
        target_os = "redox"
    ))]
    #[test]
    fn external_replacement_survives_a_racing_atomic_backup_move() {
        for _ in 0..64 {
            let dir = tempfile::tempdir().unwrap();
            let source = dir.path().join("document.wstrike");
            let backup = dir.path().join("document.wstrike.previous");
            let external = dir.path().join("external.tmp");
            fs::write(&source, b"Original archive").unwrap();
            fs::write(&external, b"External replacement").unwrap();
            let barrier = Arc::new(Barrier::new(2));
            std::thread::scope(|scope| {
                scope.spawn(|| {
                    barrier.wait();
                    fs::rename(&external, &source).unwrap();
                });
                barrier.wait();
                move_noclobber(&source, &backup).unwrap();
            });
            // A replacement either arrived before the atomic move (now the backup) or
            // after it (now the main). A link/unlink gap can lose it from both paths.
            assert!([&source, &backup].iter().any(|path| {
                fs::read(path).ok().as_deref() == Some(b"External replacement".as_slice())
            }));
        }
    }
}
