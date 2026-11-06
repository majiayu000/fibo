//! The file edits a log line holds, in the form each CLI records them once they are done.
//! Both name lines by number, which is what lets a turn's edits be added up
//! (`mooring_core::turn_diff`). A call that failed logs none of this.
use mooring_core::turn_diff::{parse_unified, Edit, RawHunk};
use serde_json::Value;

/// A `user` line's `toolUseResult`: a new file's text, or the patch the edit made.
pub(super) fn claude(line: &Value) -> Vec<(String, Edit)> {
    let result = &line["toolUseResult"];
    let Some(path) = result["filePath"].as_str() else {
        return Vec::new();
    };
    if result["type"] == "create" {
        let text = result["content"].as_str().unwrap_or_default().to_owned();
        return vec![(path.to_owned(), Edit::Created(text))];
    }
    let Some(patch) = result["structuredPatch"].as_array() else {
        return Vec::new();
    };
    let hunks: Vec<RawHunk> = patch
        .iter()
        .map(|h| RawHunk {
            old_start: h["oldStart"].as_u64().unwrap_or(0) as usize,
            new_start: h["newStart"].as_u64().unwrap_or(0) as usize,
            lines: (h["lines"].as_array().into_iter().flatten())
                .filter_map(Value::as_str)
                .filter(|l| !l.starts_with('\\'))
                .map(str::to_owned)
                .collect(),
        })
        .collect();
    if hunks.is_empty() {
        return Vec::new();
    }
    vec![(path.to_owned(), Edit::Patched(hunks))]
}

/// A finished `FileChange` item: one edit per path, `add` with its text and `update` with a
/// unified diff.
pub(super) fn codex(line: &Value) -> Vec<(String, Edit)> {
    let item = &line["payload"]["item"];
    if line["payload"]["type"] != "item_completed"
        || item["type"] != "FileChange"
        || item["status"] != "completed"
    {
        return Vec::new();
    }
    let Some(changes) = item["changes"].as_object() else {
        return Vec::new();
    };
    let mut edits = Vec::new();
    for (path, change) in changes {
        if let Some(diff) = change["unified_diff"].as_str() {
            edits.push((path.clone(), Edit::Patched(parse_unified(diff))));
        } else if let ("add", Some(text)) = (
            change["type"].as_str().unwrap_or_default(),
            change["content"].as_str(),
        ) {
            edits.push((path.clone(), Edit::Created(text.to_owned())));
        }
    }
    edits
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn claude_gives_new_files_and_patches_and_skips_the_rest() {
        let created = json!({"type":"user","toolUseResult":{"type":"create","filePath":"/r/a.html",
            "content":"one\ntwo\n","structuredPatch":[],"originalFile":null}});
        assert_eq!(
            claude(&created),
            [("/r/a.html".to_owned(), Edit::Created("one\ntwo\n".into()))]
        );
        let edited = json!({"type":"user","toolUseResult":{"filePath":"/r/a.rs","originalFile":"x",
            "structuredPatch":[{"oldStart":5,"oldLines":1,"newStart":5,"newLines":1,
                "lines":["-a","+b","\\ No newline at end of file"]}]}});
        let [(path, Edit::Patched(hunks))] = &claude(&edited)[..] else {
            panic!("{:?}", claude(&edited));
        };
        assert_eq!(path, "/r/a.rs");
        assert_eq!((hunks[0].old_start, hunks[0].new_start), (5, 5));
        assert_eq!(hunks[0].lines, ["-a", "+b"]);
        // A failed edit's result is a string; a read result has no patch.
        assert!(claude(&json!({"toolUseResult":"Error: no match"})).is_empty());
        let read = json!({"toolUseResult":{"filePath":"/r/a.rs","type":"text"}});
        assert!(claude(&read).is_empty());
    }

    #[test]
    fn codex_gives_finished_file_changes_only() {
        let done = json!({"type":"event_msg","payload":{"type":"item_completed","item":{
            "type":"FileChange","status":"completed","changes":{
                "/r/a.rs":{"type":"update","unified_diff":"@@ -3,1 +3,1 @@\n-a\n+b\n"},
                "/r/b.md":{"type":"add","content":"hello\n"}}}}});
        let edits = codex(&done);
        assert_eq!(edits.len(), 2);
        assert!(matches!(&edits[0].1, Edit::Patched(h) if h[0].lines == ["-a", "+b"]));
        assert_eq!(edits[1].1, Edit::Created("hello\n".into()));
        let failed = json!({"payload":{"type":"item_completed","item":{"type":"FileChange",
            "status":"failed","changes":{"/r/a.rs":{"unified_diff":"@@ -1 +1 @@\n-a\n+b\n"}}}}});
        assert!(codex(&failed).is_empty());
    }
}
