use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// 모드 종류. JSON/프런트와 맞추기 위해 소문자 직렬화.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModType {
    Lua,
    Pak,
    Hybrid,
    Unknown,
}

impl Default for ModType {
    fn default() -> Self {
        ModType::Unknown
    }
}

/// 자기기술적 모드 메타데이터(spec §4 D7).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    pub version: String,
    #[serde(rename = "type", default)]
    pub mod_type: ModType,
    #[serde(rename = "updateURL", default, skip_serializing_if = "Option::is_none")]
    pub update_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entry: Option<String>,
}

/// 파일명 → 안전한 id: 영숫자·`_`·`-`만 남기고 나머지는 `_`, 양끝 `_` 정리.
pub fn sanitize_id(raw: &str) -> String {
    let s: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = s.trim_matches('_').to_string();
    if trimmed.is_empty() {
        "mod".to_string()
    } else {
        trimmed
    }
}

/// 판별표(research/01): `.pak/.utoc/.ucas`→pak, `Scripts/main.lua`(대소문자 무관)→lua, 둘 다→hybrid.
pub fn detect_type(mod_dir: &Path) -> ModType {
    let mut has_pak = false;
    let mut has_lua = false;
    detect_walk(mod_dir, &mut has_pak, &mut has_lua);
    match (has_lua, has_pak) {
        (true, true) => ModType::Hybrid,
        (true, false) => ModType::Lua,
        (false, true) => ModType::Pak,
        (false, false) => ModType::Unknown,
    }
}

fn detect_walk(dir: &Path, has_pak: &mut bool, has_lua: &mut bool) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            detect_walk(&path, has_pak, has_lua);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            match ext.to_ascii_lowercase().as_str() {
                "pak" | "utoc" | "ucas" => *has_pak = true,
                "lua" => {
                    let is_main = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .map(|n| n.eq_ignore_ascii_case("main.lua"))
                        .unwrap_or(false);
                    let in_scripts = path
                        .parent()
                        .and_then(|p| p.file_name())
                        .and_then(|n| n.to_str())
                        .map(|n| n.eq_ignore_ascii_case("scripts"))
                        .unwrap_or(false);
                    if is_main && in_scripts {
                        *has_lua = true;
                    }
                }
                _ => {}
            }
        }
    }
}

/// pak/utoc 페어링 키 = "부모디렉터리/파일스템"(같은 폴더 안에서만 짝으로 인정).
fn dir_stem_key(p: &Path) -> Option<String> {
    let stem = p.file_stem()?.to_str()?;
    let parent = p.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
    Some(format!("{parent}/{stem}"))
}

/// 단일 레거시 pak(짝 .utoc 없음)이 하나라도 있으면 변환 필요.
/// 이미 3종(.pak+.utoc)인 pak은 변환 불필요.
pub fn pak_needs_conversion(mod_dir: &Path) -> bool {
    let mut paks: Vec<PathBuf> = Vec::new();
    let mut utoc_stems: std::collections::HashSet<String> = std::collections::HashSet::new();
    collect_pak_and_utoc(mod_dir, &mut paks, &mut utoc_stems);
    paks.iter().any(|p| {
        dir_stem_key(p)
            .map(|key| !utoc_stems.contains(&key))
            .unwrap_or(false)
    })
}

pub(crate) fn collect_pak_and_utoc(
    dir: &Path,
    paks: &mut Vec<PathBuf>,
    utoc_stems: &mut std::collections::HashSet<String>,
) {
    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            collect_pak_and_utoc(&path, paks, utoc_stems);
        } else if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            match ext.to_ascii_lowercase().as_str() {
                "pak" => paks.push(path.clone()),
                "utoc" => {
                    if let Some(key) = dir_stem_key(&path) {
                        utoc_stems.insert(key);
                    }
                }
                _ => {}
            }
        }
    }
}

/// 짝 .utoc가 같은 디렉터리에 없는 단일 레거시 pak 경로를 찾는다(pak_needs_conversion과 동일 페어링).
pub(crate) fn find_single_legacy_pak(dir: &Path) -> Option<PathBuf> {
    let mut paks = Vec::new();
    let mut utoc_keys = std::collections::HashSet::new();
    collect_pak_and_utoc(dir, &mut paks, &mut utoc_keys);
    paks.into_iter()
        .find(|p| dir_stem_key(p).map(|k| !utoc_keys.contains(&k)).unwrap_or(false))
}

/// JSON → Manifest. 필수 필드(id/name/version) 누락 시 Err.
pub fn parse(json: &str) -> Result<Manifest, String> {
    serde_json::from_str::<Manifest>(json).map_err(|e| format!("Failed to parse manifest: {e}"))
}

/// manifest.json 부재 시 디렉터리 내용으로 합성.
pub fn synthesize(mod_dir: &Path, fallback_id: &str) -> Manifest {
    Manifest {
        id: sanitize_id(fallback_id),
        name: fallback_id.to_string(),
        version: "0.0.0".to_string(),
        mod_type: detect_type(mod_dir),
        update_url: None,
        entry: None,
    }
}

/// manifest.json 있으면 읽고(type unknown이면 디렉터리로 보강), 없으면 합성.
pub fn load_or_synthesize(mod_dir: &Path, fallback_id: &str) -> Result<Manifest, String> {
    let mf_path = mod_dir.join("manifest.json");
    if mf_path.exists() {
        let raw = fs::read_to_string(&mf_path).map_err(|e| e.to_string())?;
        let mut m = parse(&raw)?;
        // SECURITY: sanitize manifest-supplied id to prevent path traversal.
        // e.g. "../../../etc/evil" or "/abs/path" → safe single path segment.
        m.id = sanitize_id(&m.id);
        if m.mod_type == ModType::Unknown {
            m.mod_type = detect_type(mod_dir);
        }
        Ok(m)
    } else {
        Ok(synthesize(mod_dir, fallback_id))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn touch(path: &Path) {
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).unwrap();
        }
        fs::write(path, b"x").unwrap();
    }

    #[test]
    fn sanitize_strips_unsafe_chars() {
        assert_eq!(sanitize_id("Auto Hatch!"), "Auto_Hatch");
        assert_eq!(sanitize_id("my.mod"), "my_mod");
        assert_eq!(sanitize_id("___"), "mod");
    }

    #[test]
    fn parse_reads_full_manifest() {
        let json = r#"{"id":"autohatch","name":"Auto Hatch","version":"1.2.0","type":"lua","updateURL":"https://x/y.json","entry":"Scripts/main.lua"}"#;
        let m = parse(json).unwrap();
        assert_eq!(m.id, "autohatch");
        assert_eq!(m.mod_type, ModType::Lua);
        assert_eq!(m.update_url.as_deref(), Some("https://x/y.json"));
    }

    #[test]
    fn detect_type_lua_pak_hybrid() {
        let base = std::env::temp_dir().join("pmm_detect_type");
        let _ = fs::remove_dir_all(&base);

        let lua = base.join("lua");
        touch(&lua.join("Scripts/main.lua"));
        assert_eq!(detect_type(&lua), ModType::Lua);

        let pak = base.join("pak");
        touch(&pak.join("Content/mod.pak"));
        assert_eq!(detect_type(&pak), ModType::Pak);

        let hybrid = base.join("hybrid");
        touch(&hybrid.join("Scripts/main.lua"));
        touch(&hybrid.join("data.pak"));
        assert_eq!(detect_type(&hybrid), ModType::Hybrid);

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn load_synthesizes_when_no_manifest() {
        let dir = std::env::temp_dir().join("pmm_synth");
        let _ = fs::remove_dir_all(&dir);
        touch(&dir.join("Scripts/main.lua"));
        let m = load_or_synthesize(&dir, "AutoHatch").unwrap();
        assert_eq!(m.id, "AutoHatch");
        assert_eq!(m.name, "AutoHatch");
        assert_eq!(m.mod_type, ModType::Lua);
        assert_eq!(m.version, "0.0.0");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_backfills_unknown_type_from_directory() {
        let dir = std::env::temp_dir().join("pmm_backfill_unknown");
        let _ = fs::remove_dir_all(&dir);

        // Create manifest.json with "type":"unknown"
        touch(&dir.join("manifest.json"));
        fs::write(
            &dir.join("manifest.json"),
            r#"{"id":"testmod","name":"Test Mod","version":"1.0.0","type":"unknown"}"#,
        )
        .unwrap();

        // Create Scripts/main.lua to make detection return Lua
        touch(&dir.join("Scripts/main.lua"));

        let m = load_or_synthesize(&dir, "fallback").unwrap();
        assert_eq!(m.id, "testmod");
        assert_eq!(m.name, "Test Mod");
        assert_eq!(m.mod_type, ModType::Lua);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn manifest_id_is_sanitized_against_traversal() {
        let dir = std::env::temp_dir().join("pmm_traversal_test");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        fs::write(
            &dir.join("manifest.json"),
            r#"{"id":"../../../etc/evil","name":"Evil Mod","version":"1.0.0","type":"lua"}"#,
        )
        .unwrap();
        let m = load_or_synthesize(&dir, "fallback").unwrap();
        assert!(!m.id.contains('/'), "id must not contain path separators");
        assert!(!m.id.contains(".."), "id must not contain ..");
        assert!(!m.id.is_empty(), "id must not be empty");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn detect_type_utoc_ucas_extensions() {
        let base = std::env::temp_dir().join("pmm_detect_utoc_ucas");
        let _ = fs::remove_dir_all(&base);

        let utoc = base.join("utoc");
        touch(&utoc.join("Content/mod.utoc"));
        assert_eq!(detect_type(&utoc), ModType::Pak);

        let ucas = base.join("ucas");
        touch(&ucas.join("Content/mod.ucas"));
        assert_eq!(detect_type(&ucas), ModType::Pak);

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn pak_needs_conversion_single_legacy_true() {
        let base = std::env::temp_dir().join("pmm_needs_conv_single");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("Paks")).unwrap();
        fs::write(base.join("Paks/Mod_P.pak"), b"x").unwrap();
        assert!(pak_needs_conversion(&base));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn pak_needs_conversion_three_container_false() {
        let base = std::env::temp_dir().join("pmm_needs_conv_three");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("Paks")).unwrap();
        for ext in ["pak", "ucas", "utoc"] {
            fs::write(base.join(format!("Paks/Mod.{ext}")), b"x").unwrap();
        }
        assert!(!pak_needs_conversion(&base));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn pak_needs_conversion_cross_dir_stem_does_not_pair() {
        let base = std::env::temp_dir().join("pmm_needs_conv_crossdir");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(base.join("DirA")).unwrap();
        fs::create_dir_all(base.join("DirB")).unwrap();
        fs::write(base.join("DirA/Foo.pak"), b"x").unwrap();   // legacy pak, no sibling utoc
        fs::write(base.join("DirB/Foo.utoc"), b"x").unwrap();  // unrelated utoc, same stem, other dir
        assert!(pak_needs_conversion(&base), "different-dir utoc must not pair with the pak");
        let _ = fs::remove_dir_all(&base);
    }
}
