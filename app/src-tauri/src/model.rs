use serde::Serialize;
use serde_json::Value;

#[derive(Serialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum FileStatus {
    Ready,
    Inspecting,
    Queued,
    Processing,
    Verifying,
    SyntheticWriting,
    Processed,
    Warning,
    Failed,
    Unsupported,
    Cancelled,
}

#[derive(Serialize, Clone, Debug)]
pub struct PublicSelectedFile {
    pub id: String,
    pub display_name: String,
    pub extension: Option<String>,
    pub relative_path: Option<String>,
    pub size: u64,
    pub status: FileStatus,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct MetadataEntry {
    pub key: String,
    pub display_value: String,
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub enum DiffStatus {
    Removed,
    Changed,
    Remaining,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct MetadataDiff {
    pub key: String,
    pub before: Option<String>,
    pub after: Option<String>,
    pub status: DiffStatus,
}

#[derive(Clone, Debug)]
pub struct InspectionResult {
    /// Some(...) = the inspection itself failed (message from the adapter);
    /// None + supported=false means "MAT2 does not support this format".
    pub error: Option<String>,
    pub supported: bool,
    pub mimetype: Option<String>,
    pub entries: Vec<MetadataEntry>,
}

#[derive(Serialize, Clone, Debug)]
pub struct InspectionDto {
    pub supported: bool,
    pub mimetype: Option<String>,
    pub error: Option<String>,
    pub entries: Vec<MetadataEntry>,
}

#[derive(Serialize, Clone, Debug, PartialEq, Eq)]
pub struct DependencyStatus {
    pub name: String,
    pub found: bool,
    pub required: bool,
}

#[derive(Serialize, Clone, Debug)]
pub struct DiagnosticsDto {
    pub available: bool,
    pub fatal: bool,
    pub version: Option<String>,
    pub dependencies: Vec<DependencyStatus>,
    pub missing_required: Vec<String>,
    pub missing_optional: Vec<String>,
    pub error: Option<String>,
    pub app_version: String,
}

impl From<InspectionResult> for InspectionDto {
    fn from(r: InspectionResult) -> Self {
        InspectionDto {
            supported: r.supported,
            mimetype: r.mimetype,
            error: r.error,
            entries: r.entries,
        }
    }
}

/// Parse the JSON protocol emitted by resources/mat2_inspect.py.
pub fn parse_inspection_stdout(stdout: &str) -> Result<InspectionResult, String> {
    let value: Value = serde_json::from_str(stdout.trim())
        .map_err(|e| format!("inspection adapter produced invalid JSON: {e}"))?;
    let obj = value
        .as_object()
        .ok_or_else(|| "inspection JSON is not an object".to_string())?;

    let ok = obj.get("ok").and_then(Value::as_bool).unwrap_or(false);
    if !ok {
        let msg = obj
            .get("error")
            .and_then(Value::as_str)
            .unwrap_or("unknown adapter error")
            .to_string();
        return Ok(InspectionResult {
            error: Some(msg),
            supported: false,
            mimetype: None,
            entries: Vec::new(),
        });
    }

    let supported = obj
        .get("supported")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let mimetype = obj
        .get("mimetype")
        .and_then(Value::as_str)
        .map(str::to_string);
    let mut entries = Vec::new();
    if supported && let Some(meta) = obj.get("metadata") {
        flatten_metadata(meta, "", &mut entries);
    }
    Ok(InspectionResult {
        error: None,
        supported,
        mimetype,
        entries,
    })
}

const KEY_SEP: &str = " / ";

/// Flatten libmat2's nested metadata dict into sorted key/display_value pairs.
/// Keys and values are data: never escaped, never interpreted here.
pub fn flatten_metadata(value: &Value, prefix: &str, out: &mut Vec<MetadataEntry>) {
    match value {
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for k in keys {
                let path = if prefix.is_empty() {
                    k.clone()
                } else {
                    format!("{prefix}{KEY_SEP}{k}")
                };
                flatten_metadata(map.get(k).expect("key from map"), &path, out);
            }
        }
        other => {
            let display = match other {
                Value::String(s) => s.clone(),
                v => v.to_string(),
            };
            out.push(MetadataEntry {
                key: prefix.to_string(),
                display_value: display,
            });
        }
    }
}

/// Before/after comparison (INTERFACE.md §15). Statuses:
/// - Removed: present before, absent after
/// - Changed: present in both with different values, OR appeared only after
///   (documented mapping: a newly detectable key changes the detectable
///   metadata set; it is never a success signal)
/// - Remaining: present in both with identical values
pub fn diff_metadata(before: &[MetadataEntry], after: &[MetadataEntry]) -> Vec<MetadataDiff> {
    let mut diffs = Vec::new();
    let mut matched_after: Vec<&str> = Vec::new();

    for b in before {
        match after.iter().find(|a| a.key == b.key) {
            Some(a) => {
                matched_after.push(a.key.as_str());
                diffs.push(MetadataDiff {
                    key: b.key.clone(),
                    before: Some(b.display_value.clone()),
                    after: Some(a.display_value.clone()),
                    status: if a.display_value == b.display_value {
                        DiffStatus::Remaining
                    } else {
                        DiffStatus::Changed
                    },
                });
            }
            None => diffs.push(MetadataDiff {
                key: b.key.clone(),
                before: Some(b.display_value.clone()),
                after: None,
                status: DiffStatus::Removed,
            }),
        }
    }
    for a in after {
        if !matched_after.contains(&a.key.as_str()) {
            diffs.push(MetadataDiff {
                key: a.key.clone(),
                before: None,
                after: Some(a.display_value.clone()),
                status: DiffStatus::Changed,
            });
        }
    }
    diffs
}

#[derive(Serialize, Clone, Copy, Debug, PartialEq, Eq)]
pub struct DiffSummary {
    pub detected_before: usize,
    pub removed: usize,
    pub changed: usize,
    pub still_detectable: usize,
}

pub fn summarize(diffs: &[MetadataDiff], before_len: usize) -> DiffSummary {
    DiffSummary {
        detected_before: before_len,
        removed: diffs
            .iter()
            .filter(|d| d.status == DiffStatus::Removed)
            .count(),
        changed: diffs
            .iter()
            .filter(|d| d.status == DiffStatus::Changed)
            .count(),
        still_detectable: diffs.iter().filter(|d| d.after.is_some()).count(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(k: &str, v: &str) -> MetadataEntry {
        MetadataEntry {
            key: k.to_string(),
            display_value: v.to_string(),
        }
    }

    #[test]
    fn parses_supported_inspection() {
        let json = r#"{"ok": true, "supported": true, "mimetype": "image/jpeg",
            "metadata": {"Comment": "Created with GIMP", "Software": "x"}}"#;
        let r = parse_inspection_stdout(json).unwrap();
        assert!(r.supported && r.error.is_none());
        assert_eq!(r.mimetype.as_deref(), Some("image/jpeg"));
        assert_eq!(
            r.entries,
            vec![
                entry("Comment", "Created with GIMP"),
                entry("Software", "x")
            ]
        );
    }

    #[test]
    fn parses_unsupported_and_error() {
        let r = parse_inspection_stdout(r#"{"ok": true, "supported": false, "mimetype": null}"#)
            .unwrap();
        assert!(!r.supported && r.error.is_none() && r.entries.is_empty());

        let r = parse_inspection_stdout(r#"{"ok": false, "error": "invalid file: boom"}"#).unwrap();
        assert_eq!(r.error.as_deref(), Some("invalid file: boom"));
        assert!(!r.supported);

        assert!(parse_inspection_stdout("not json").is_err());
        assert!(parse_inspection_stdout("[1,2]").is_err());
    }

    #[test]
    fn flattens_nested_and_arrays_with_sorted_keys() {
        let json = serde_json::json!({
            "zeta": "z",
            "member.jpg": {"Comment": "gimp", "Exif": {"Artist": "A"}},
            "brands": ["isom", "mp41"],
            "num": 42
        });
        let mut out = Vec::new();
        flatten_metadata(&json, "", &mut out);
        let keys: Vec<&str> = out.iter().map(|e| e.key.as_str()).collect();
        assert_eq!(
            keys,
            vec![
                "brands",
                "member.jpg / Comment",
                "member.jpg / Exif / Artist",
                "num",
                "zeta"
            ]
        );
        assert_eq!(out[0].display_value, r#"["isom","mp41"]"#);
        assert_eq!(out[3].display_value, "42");
    }

    #[test]
    fn complex_and_hostile_keys_and_values_are_data() {
        let json = serde_json::json!({
            "key: with colon / and sep": "<img src=x onerror=alert(1)>",
            "<script>x</script>": "<b>bold</b> & \"quoted\""
        });
        let mut out = Vec::new();
        flatten_metadata(&json, "", &mut out);
        assert_eq!(out.len(), 2);
        assert!(out.iter().any(|e| e.key == "key: with colon / and sep"
            && e.display_value == "<img src=x onerror=alert(1)>"));
        assert!(out.iter().any(
            |e| e.key == "<script>x</script>" && e.display_value == "<b>bold</b> & \"quoted\""
        ));
    }

    #[test]
    fn diff_removed_changed_remaining() {
        let before = vec![
            entry("Author", "Alice"),
            entry("Creator", "Word"),
            entry("Modified", "2026-01-01"),
        ];
        let after = vec![entry("Modified", "2026-02-02"), entry("Creator", "Word")];
        let d = diff_metadata(&before, &after);
        let get = |k: &str| d.iter().find(|x| x.key == k).unwrap();
        assert_eq!(get("Author").status, DiffStatus::Removed);
        assert_eq!(get("Author").after, None);
        assert_eq!(get("Creator").status, DiffStatus::Remaining);
        assert_eq!(get("Modified").status, DiffStatus::Changed);
        assert_eq!(get("Modified").before.as_deref(), Some("2026-01-01"));
        assert_eq!(get("Modified").after.as_deref(), Some("2026-02-02"));
    }

    #[test]
    fn diff_empty_sides() {
        let before = vec![entry("A", "1")];
        let d = diff_metadata(&before, &[]);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].status, DiffStatus::Removed);

        let after = vec![entry("B", "2")];
        let d = diff_metadata(&[], &after);
        assert_eq!(d.len(), 1);
        assert_eq!(d[0].status, DiffStatus::Changed);
        assert_eq!(d[0].before, None);

        assert!(diff_metadata(&[], &[]).is_empty());
    }

    #[test]
    fn summary_counts() {
        let before = vec![entry("A", "1"), entry("B", "2"), entry("C", "3")];
        let after = vec![entry("B", "changed"), entry("C", "3"), entry("D", "new")];
        let d = diff_metadata(&before, &after);
        let s = summarize(&d, before.len());
        assert_eq!(s.detected_before, 3);
        assert_eq!(s.removed, 1);
        assert_eq!(s.changed, 2);
        assert_eq!(s.still_detectable, 3);
    }
}
