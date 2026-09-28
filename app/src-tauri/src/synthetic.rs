//! Synthetic metadata add-on boundary (owner-approved scope extension).
//!
//! MAT2 remains the only sanitisation engine. This module talks to the
//! bundled synthetic engine (scripts/packaging/synthetic_engine, frozen into
//! the packaged runtime) over a typed stdin/stdout JSON protocol. Security
//! invariants:
//! - the frontend submits ONLY the typed SyntheticOptions below — never tag
//!   names, values, paths or writer arguments;
//! - all writer arguments are constructed inside the engine from validated
//!   profiles; invocation is a fixed program + argv vector, never a shell;
//! - the engine only ever receives MAT2-cleaned STAGED paths (enforced by
//!   the jobs pipeline, which also snapshots/restores the clean bytes);
//! - the profile pack is bundled, read-only, SHA-256 pinned;
//! - job seeds are CSPRNG-derived, memory-only, never logged/embedded.

use std::ffi::OsString;
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};

pub const EXPECTED_PACK_SHA256: &str =
    "5bf6b12939d05defb4f8fd9b132a7a12be8c0cc211ce4522160905696e8efb1d";

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ProfileScope {
    PerFile,
    Batch,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum IdentityMode {
    Alias,
    Empty,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LocationMode {
    Off,
    City,
    Gps,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TechnicalMode {
    Synthetic,
    Empty,
}

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SerialMode {
    Empty,
    Generate,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
#[serde(rename_all = "camelCase")]
pub struct SyntheticOptions {
    pub enabled: bool,
    pub profile_scope: ProfileScope,
    pub identity_mode: IdentityMode,
    pub location_mode: LocationMode,
    pub technical_mode: TechnicalMode,
    pub serial_mode: SerialMode,
    #[serde(default)]
    pub tripwire: Option<crate::tripwire::TripwireOptions>,
}

impl Default for SyntheticOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            profile_scope: ProfileScope::PerFile,
            identity_mode: IdentityMode::Alias,
            location_mode: LocationMode::Off,
            technical_mode: TechnicalMode::Synthetic,
            serial_mode: SerialMode::Empty,
            tripwire: None,
        }
    }
}

impl SyntheticOptions {
    pub fn to_engine_json(&self) -> serde_json::Value {
        let scope = match self.profile_scope {
            ProfileScope::PerFile => "per_file",
            ProfileScope::Batch => "batch",
        };
        let identity = match self.identity_mode {
            IdentityMode::Alias => "alias",
            IdentityMode::Empty => "empty",
        };
        let location = match self.location_mode {
            LocationMode::Off => "off",
            LocationMode::City => "city",
            LocationMode::Gps => "gps",
        };
        let technical = match self.technical_mode {
            TechnicalMode::Synthetic => "synthetic",
            TechnicalMode::Empty => "empty",
        };
        let serial = match self.serial_mode {
            SerialMode::Empty => "empty",
            SerialMode::Generate => "generate",
        };
        serde_json::json!({
            "profile_scope": scope,
            "identity_mode": identity,
            "location_mode": location,
            "technical_mode": technical,
            "serial_mode": serial,
        })
    }
}

#[derive(Clone, Debug)]
pub struct SyntheticRuntime {
    program: PathBuf,
    prefix: Vec<OsString>,
    python_path: Option<PathBuf>,
    pack_path: PathBuf,
}

impl SyntheticRuntime {
    /// Resolve the synthetic engine alongside a resolved MAT2 runtime:
    /// frozen runtime → its `synthetic` subcommand + pack bundled inside;
    /// dev runtime → venv python running scripts/packaging/synthetic_engine
    /// with the repo's addon-fauxmeta pack. Env override:
    /// MAT2_WRAPPER_SYNTH_PACK (pack) — program always follows the MAT2
    /// runtime so the bundled exiftool/mutagen are the ones used.
    pub fn resolve(
        mat2: &crate::mat2_runner::Mat2Runtime,
        _resource_dir: Option<&Path>,
    ) -> Result<SyntheticRuntime, String> {
        use crate::mat2_runner::RuntimeKind;
        let pack_override = std::env::var("MAT2_WRAPPER_SYNTH_PACK")
            .ok()
            .map(PathBuf::from);

        match mat2.kind {
            RuntimeKind::Frozen => {
                let internal = mat2
                    .program
                    .parent()
                    .map(|p| p.join("_internal"))
                    .ok_or_else(|| "frozen runtime has no parent dir".to_string())?;
                let pack = pack_override
                    .unwrap_or_else(|| internal.join("synthetic_metadata_profiles_v1.json"));
                if !pack.exists() {
                    return Err(format!(
                        "synthetic pack not found in runtime bundle: {:?}",
                        pack
                    ));
                }
                Ok(SyntheticRuntime {
                    program: mat2.program.clone(),
                    prefix: vec![OsString::from("synthetic")],
                    python_path: None,
                    pack_path: pack,
                })
            }
            RuntimeKind::Dev => {
                let manifest = Path::new(env!("CARGO_MANIFEST_DIR"));
                let root = manifest
                    .ancestors()
                    .nth(2)
                    .ok_or_else(|| "cannot derive project root".to_string())?;
                let packaging = root.join("scripts/packaging");
                let pack = pack_override.unwrap_or_else(|| {
                    root.join("addon-fauxmeta/synthetic_metadata_profiles_v1.json")
                });
                if !packaging.join("synthetic_engine/__main__.py").exists() {
                    return Err("synthetic engine sources not found in dev tree".to_string());
                }
                if !pack.exists() {
                    return Err(format!("synthetic pack not found: {:?}", pack));
                }
                Ok(SyntheticRuntime {
                    program: mat2.program.clone(),
                    prefix: vec![OsString::from("-m"), OsString::from("synthetic_engine")],
                    python_path: Some(packaging),
                    pack_path: pack,
                })
            }
        }
    }

    pub fn pack_sha256(&self) -> Result<String, String> {
        let bytes = std::fs::read(&self.pack_path)
            .map_err(|e| format!("cannot read synthetic pack: {e}"))?;
        use sha2::{Digest, Sha256};
        let mut h = Sha256::new();
        h.update(&bytes);
        Ok(format!("{:x}", h.finalize()))
    }

    #[cfg(test)]
    pub(crate) fn with_pack(mut self, pack: PathBuf) -> Self {
        self.pack_path = pack;
        self
    }

    fn request(&self, req: serde_json::Value) -> Result<serde_json::Value, String> {
        // SECURITY INVARIANT: fixed program + argv prefix, JSON over stdin —
        // never a shell, never frontend-controlled arguments.
        let mut cmd = Command::new(&self.program);
        cmd.args(&self.prefix)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(pp) = &self.python_path {
            cmd.env("PYTHONPATH", pp);
        }
        let mut child = cmd
            .spawn()
            .map_err(|e| format!("failed to spawn synthetic engine: {e}"))?;
        {
            let stdin = child.stdin.as_mut().ok_or("no stdin on engine child")?;
            stdin
                .write_all(serde_json::to_string(&req).unwrap_or_default().as_bytes())
                .map_err(|e| format!("cannot write engine request: {e}"))?;
        }
        let out = child
            .wait_with_output()
            .map_err(|e| format!("engine wait failed: {e}"))?;
        if !out.status.success() {
            return Err(format!(
                "synthetic engine failed (exit {:?}): {}",
                out.status.code(),
                String::from_utf8_lossy(&out.stderr).trim()
            ));
        }
        serde_json::from_slice(&out.stdout)
            .map_err(|e| format!("engine produced invalid JSON: {e}"))
    }

    pub fn validate_pack(&self) -> Result<serde_json::Value, String> {
        self.request(serde_json::json!({
            "action": "validate_pack",
            "pack_path": self.pack_path,
        }))
    }

    pub fn preview(
        &self,
        options: &SyntheticOptions,
        job_seed: &str,
        selection_id: &str,
        ext: &str,
    ) -> Result<serde_json::Value, String> {
        self.request(serde_json::json!({
            "action": "preview",
            "pack_path": self.pack_path,
            "options": options.to_engine_json(),
            "job_seed": job_seed,
            "selection_id": selection_id,
            "ext": ext,
        }))
    }

    // 8 typed parameters mirror the engine apply protocol exactly; bundling
    // them into a struct would only relocate the contract.
    #[allow(clippy::too_many_arguments)]
    pub fn apply(
        &self,
        options: &SyntheticOptions,
        job_seed: &str,
        selection_id: &str,
        staged_cleaned: &Path,
        ext: &str,
        original_values: &[String],
        tripwire_source_url: Option<&str>,
    ) -> Result<ApplyResponse, String> {
        let mut req = serde_json::json!({
            "action": "apply",
            "pack_path": self.pack_path,
            "options": options.to_engine_json(),
            "job_seed": job_seed,
            "selection_id": selection_id,
            "file": {
                "path": staged_cleaned,
                "ext": ext,
                "original_values": original_values,
            },
        });
        if let Some(url) = tripwire_source_url {
            req["tripwire"] = serde_json::json!({ "source_url": url });
        }
        let resp = self.request(req)?;
        serde_json::from_value(resp).map_err(|e| format!("bad engine response: {e}"))
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct SyntheticField {
    pub field: String,
    pub value: String,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
pub struct ApplyResponse {
    pub ok: bool,
    #[serde(default)]
    pub stage: Option<String>,
    pub synthetic_state: String,
    #[serde(default)]
    pub written: Vec<SyntheticField>,
    #[serde(default)]
    pub error: Option<String>,
    #[serde(default)]
    pub verification: Option<serde_json::Value>,
    #[serde(default)]
    pub tripwire_state: Option<String>,
    #[serde(default)]
    pub tripwire_error: Option<String>,
}

/// Format-constant keys whose values are structural, not identifying.
/// Mirrors STRUCTURAL_KEYS in the engine test-suite helper: removed-values
/// computation excludes them so Phase-B absence checks target only
/// identifying metadata (HANDOFF §20).
pub const STRUCTURAL_KEYS: &[&str] = &[
    "colorspace",
    "componentsconfiguration",
    "ycbcrpositioning",
    "exifversion",
    "flashpixversion",
    "exifbyteorder",
    "encodingprocess",
    "bitspersample",
    "colorcomponents",
    "compression",
    "jfifversion",
    "resolutionunit",
    "xresolution",
    "yresolution",
    "imagewidth",
    "imageheight",
    "exifimagewidth",
    "exifimageheight",
    "interopindex",
    "interopversion",
    "filesource",
    "scenetype",
    "customrendered",
    "digitalzoomratio",
    "sensingmethod",
    "scenecapturetype",
    "gaincontrol",
    "contrast",
    "saturation",
    "sharpness",
    "subjectdistancerange",
    "exposuremode",
    "whitebalance",
    "fnumber",
    "exposuretime",
    "exposurecompensation",
    "focallength",
    "isospeedratings",
    "iso",
    "lightsource",
    "meteringmode",
    "flash",
    "aperture",
    "shutterspeed",
    "maxaperturevalue",
    "brightness",
    "subsectime",
    "subsectimeoriginal",
    "subsectimedigitized",
];

/// Identifying values MAT2 removed (diff Removed entries), minus structural
/// keys — the absence targets for Phase-B verification.
pub fn removed_original_values(diffs: &[crate::model::MetadataDiff]) -> Vec<String> {
    diffs
        .iter()
        .filter(|d| d.status == crate::model::DiffStatus::Removed)
        .filter(|d| {
            let last = d.key.rsplit(" / ").next().unwrap_or(&d.key).to_lowercase();
            !STRUCTURAL_KEYS.contains(&last.as_str())
        })
        .filter_map(|d| d.before.clone())
        .filter(|v| v.trim().len() >= 4)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{DiffStatus, MetadataDiff};

    fn diff(key: &str, before: Option<&str>, status: DiffStatus) -> MetadataDiff {
        MetadataDiff {
            key: key.to_string(),
            before: before.map(str::to_string),
            after: None,
            status,
        }
    }

    #[test]
    fn options_serialize_for_frontend_and_engine() {
        let json = r#"{
            "enabled": true,
            "profileScope": "batch",
            "identityMode": "empty",
            "locationMode": "gps",
            "technicalMode": "empty",
            "serialMode": "generate"
        }"#;
        let opts: SyntheticOptions = serde_json::from_str(json).unwrap();
        assert!(opts.enabled);
        let engine = opts.to_engine_json();
        assert_eq!(engine["profile_scope"], "batch");
        assert_eq!(engine["identity_mode"], "empty");
        assert_eq!(engine["location_mode"], "gps");
        assert_eq!(engine["technical_mode"], "empty");
        assert_eq!(engine["serial_mode"], "generate");
    }

    #[test]
    fn defaults_are_private() {
        let d = SyntheticOptions::default();
        assert!(!d.enabled);
        assert_eq!(d.location_mode, LocationMode::Off);
        assert_eq!(d.serial_mode, SerialMode::Empty);
        assert_eq!(d.profile_scope, ProfileScope::PerFile);
    }

    #[test]
    fn removed_values_exclude_structural_keys() {
        let diffs = vec![
            diff("Author", Some("Alice Example"), DiffStatus::Removed),
            diff("ColorSpace", Some("Uncalibrated"), DiffStatus::Removed),
            diff(
                "ComponentsConfiguration",
                Some("Y, Cb, Cr, -"),
                DiffStatus::Removed,
            ),
            diff(
                "GPSPosition",
                Some("43 deg 28' 2.81\" N"),
                DiffStatus::Removed,
            ),
            diff("Make", Some("NIKON"), DiffStatus::Removed),
            diff("ExifImageWidth", Some("640"), DiffStatus::Removed),
            diff("Software", Some("Nikon Transfer"), DiffStatus::Changed),
            diff("member.jpg / ColorSpace", Some("sRGB"), DiffStatus::Removed),
        ];
        let values = removed_original_values(&diffs);
        assert!(values.contains(&"Alice Example".to_string()));
        assert!(values.contains(&"NIKON".to_string()));
        assert!(values.iter().any(|v| v.contains("43 deg")));
        assert!(!values.iter().any(|v| v == "Uncalibrated"));
        assert!(!values.iter().any(|v| v.contains("Y, Cb")));
        assert!(!values.iter().any(|v| v == "sRGB"));
        assert!(
            !values.contains(&"Nikon Transfer".to_string()),
            "Changed is not an absence target"
        );
    }

    #[test]
    fn dev_runtime_resolves_and_pack_is_pinned() {
        let Ok(mat2) = crate::mat2_runner::Mat2Runtime::resolve() else {
            eprintln!("SKIP: dev runtime unavailable");
            return;
        };
        let synth = SyntheticRuntime::resolve(&mat2, None).expect("synthetic runtime resolves");
        assert_eq!(synth.pack_sha256().unwrap(), EXPECTED_PACK_SHA256);
        let v = synth.validate_pack().unwrap();
        assert_eq!(v["ok"], serde_json::json!(true));
        assert_eq!(v["sha256"], serde_json::json!(EXPECTED_PACK_SHA256));
    }

    #[test]
    fn preview_and_route_via_engine() {
        let Ok(mat2) = crate::mat2_runner::Mat2Runtime::resolve() else {
            eprintln!("SKIP: dev runtime unavailable");
            return;
        };
        let synth = SyntheticRuntime::resolve(&mat2, None).unwrap();
        let seed = format!("{}{}", uuid::Uuid::new_v4(), uuid::Uuid::new_v4());
        let p = synth
            .preview(&SyntheticOptions::default(), &seed, "sel-preview", "jpg")
            .unwrap();
        assert_eq!(p["ok"], serde_json::json!(true));
        assert!(p["profile"]["archetype"].is_string());
        let blob = serde_json::to_string(&p).unwrap();
        assert!(!blob.contains(&seed), "seed must never leave as data");
        assert!(!blob.contains("sel-preview"));

        let unsupported = synth.preview(&SyntheticOptions::default(), &seed, "s", "txt");
        assert!(unsupported.is_err() || unsupported.unwrap()["ok"] == serde_json::json!(false));
    }
}
