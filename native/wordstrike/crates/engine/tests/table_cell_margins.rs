//! Real-engine regressions for partial table-margin edits (WordCraft #3490 / #3509).

use serde_json::json;
use wordcraft_doc::styles::{Style, StyleKind, TableStyleParts};
use wordcraft_doc::{Document, StoryRef};
use wordcraft_engine::{CmdError, Session, io};

fn table_session() -> Session {
    let mut s = Session::new(Document::new());
    s.run("insert.table", &json!({"rows": 1, "cols": 1})).unwrap();
    s
}

fn margins(s: &Session) -> Option<[f32; 4]> {
    let (tp, _, _) = s.sel.focus.path.cell().unwrap();
    s.doc.table(StoryRef::Body, &tp).unwrap().props.cell_margins
}

fn styled_session() -> Session {
    let mut s = table_session();
    s.doc.styles.upsert(Style {
        id: "MarginBase".into(),
        kind: StyleKind::Table,
        table: Some(TableStyleParts { cell_margins: Some([3.0, 7.0, 11.0, 13.0]), ..Default::default() }),
        ..Default::default()
    });
    s.doc.styles.upsert(Style { id: "MarginChild".into(), kind: StyleKind::Table, based_on: Some("MarginBase".into()), ..Default::default() });
    let (tp, _, _) = s.sel.focus.path.cell().unwrap();
    let props = &mut s.doc.table_mut(StoryRef::Body, &tp).unwrap().props;
    props.style = Some("MarginChild".into());
    props.cell_margins = None;
    s
}

/// Sequential and multi-side edits retain omitted values, including explicit zeroes.
#[test]
fn table_cell_margins_preserves_unpassed_sides() {
    let mut s = table_session();
    for (params, expected) in [
        (json!({"top": 20.0}), [20.0, 5.4, 0.0, 5.4]),
        (json!({"left": 10.0}), [20.0, 10.0, 0.0, 5.4]),
        (json!({"bottom": 8.0}), [20.0, 10.0, 8.0, 5.4]),
        (json!({"right": 15.0}), [20.0, 10.0, 8.0, 15.0]),
        (json!({"top": 0.0, "bottom": 0.0}), [0.0, 10.0, 0.0, 15.0]),
        (json!({}), [0.0, 10.0, 0.0, 15.0]),
    ] {
        s.run("table.cellMargins", &params).unwrap();
        assert_eq!(margins(&s), Some(expected), "{params}");
    }
}

/// A table without direct margins uses the resolved (including inherited) style.
#[test]
fn table_cell_margins_falls_back_to_inherited_style() {
    let mut s = styled_session();
    s.run("table.cellMargins", &json!({"top": 20.0})).unwrap();
    assert_eq!(margins(&s), Some([20.0, 7.0, 11.0, 13.0]));
}

/// Direct table properties win over a style; subsequent edits use the new properties.
#[test]
fn table_cell_margins_prefers_explicit_values_over_style() {
    let mut s = styled_session();
    let (tp, _, _) = s.sel.focus.path.cell().unwrap();
    s.doc.table_mut(StoryRef::Body, &tp).unwrap().props.cell_margins = Some([0.0, 17.0, 19.0, 23.0]);
    s.run("table.cellMargins", &json!({"bottom": 29.0})).unwrap();
    assert_eq!(margins(&s), Some([0.0, 17.0, 29.0, 23.0]));
    s.run("table.cellMargins", &json!({"right": 31.0})).unwrap();
    assert_eq!(margins(&s), Some([0.0, 17.0, 29.0, 31.0]));
}

/// The real command remains one undo step and restores style fallback on undo.
#[test]
fn table_cell_margins_partial_update_undoes_and_redoes() {
    let mut s = styled_session();
    let before = s.undo_labels().len();
    s.run("table.cellMargins", &json!({"left": 21.0})).unwrap();
    assert_eq!(margins(&s), Some([3.0, 21.0, 11.0, 13.0]));
    assert_eq!(s.undo_labels().len(), before + 1);
    s.run("edit.undo", &json!({})).unwrap();
    assert_eq!(margins(&s), None);
    assert_eq!(s.undo_labels().len(), before);
    s.run("edit.redo", &json!({})).unwrap();
    assert_eq!(margins(&s), Some([3.0, 21.0, 11.0, 13.0]));
}

/// Preserve the existing clamping, table refusal, and paragraph cache invalidation.
#[test]
fn table_cell_margins_keeps_clamp_refusal_and_invalidation() {
    let mut outside = Session::new(Document::new());
    assert!(matches!(outside.run("table.cellMargins", &json!({"top": 20})), Err(CmdError::Disabled(_))));
    assert!(!outside.dirty);
    assert!(!outside.can_undo());

    let mut s = table_session();
    let before = s.doc.para_at(&s.sel.focus).unwrap().rev;
    s.run("table.cellMargins", &json!({"top": -1.0, "left": 200.0})).unwrap();
    assert_eq!(margins(&s), Some([0.0, 144.0, 0.0, 5.4]));
    assert_ne!(s.doc.para_at(&s.sel.focus).unwrap().rev, before);
}

/// Exercise native Save As and reopen, not a model serialization stand-in.
#[test]
fn table_cell_margins_partial_update_survives_docx_save_and_reopen() {
    struct Scratch(std::path::PathBuf);
    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }
    let unique = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_nanos();
    let dir = Scratch(std::env::temp_dir().join(format!("wordstrike-table-margins-{}-{unique}", std::process::id())));
    std::fs::create_dir(&dir.0).unwrap();
    let path = dir.0.join("partial-margins.docx");
    let mut s = styled_session();
    let (tp, _, _) = s.sel.focus.path.cell().unwrap();
    s.run("table.cellMargins", &json!({"top": 20.0})).unwrap();
    s.run("table.cellMargins", &json!({"left": 21.0})).unwrap();
    assert!(s.dirty);
    s.run("file.saveAs", &json!({"path": path})).unwrap();
    assert!(!s.dirty);
    assert_eq!(io::open_path(&path).unwrap().table(StoryRef::Body, &tp).unwrap().props.cell_margins, Some([20.0, 21.0, 11.0, 13.0]));
}
