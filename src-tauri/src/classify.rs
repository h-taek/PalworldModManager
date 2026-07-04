use std::path::{Path, PathBuf};
use std::fs;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Bucket { Mods, LogicMods, LuaMods }

/// 배치 연산 종류. 대부분 파일은 그냥 복사하지만, ModConfigMenu 설정(`*.modconfig.json`)은
/// 읽기전용 번들에 복사하면 인게임 저장이 실패하므로 컨테이너 원본 + 번들 심링크로 처리한다.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlaceOp { Copy, LinkModConfig }

#[derive(Debug, Clone, PartialEq)]
pub struct Placement {
    pub src: PathBuf,
    pub bucket: Bucket,
    pub dst_rel: PathBuf,
    pub op: PlaceOp,
}

fn collect_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = match fs::read_dir(dir) { Ok(e) => e, Err(_) => return };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() { collect_files(&path, out); }
        else { out.push(path); }
    }
}

fn name_lower(p: &Path) -> String {
    p.file_name().and_then(|n| n.to_str()).unwrap_or("").to_ascii_lowercase()
}

fn is_pak_asset(p: &Path) -> bool {
    matches!(p.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).as_deref(),
        Some("pak") | Some("ucas") | Some("utoc"))
}

fn is_modconfig(p: &Path) -> bool { name_lower(p).ends_with(".modconfig.json") }

fn is_lua(p: &Path) -> bool {
    p.extension().and_then(|e| e.to_str()).map(|e| e.eq_ignore_ascii_case("lua")).unwrap_or(false)
}

/// rel 경로에 대소문자 무관 세그먼트가 있는지.
fn has_segment(rel: &Path, seg: &str) -> bool {
    rel.components().any(|c| {
        c.as_os_str().to_str().map(|s| s.eq_ignore_ascii_case(seg)).unwrap_or(false)
    })
}

fn pak_bucket(rel: &Path, has_lua_component: bool) -> Bucket {
    if has_segment(rel, "LogicMods") { Bucket::LogicMods }
    else if has_segment(rel, "~mods") { Bucket::Mods }
    else if has_lua_component { Bucket::LogicMods } // 하이브리드: BP pak은 LogicMods
    else { Bucket::Mods }                            // 맨 pak 기본
}

/// Lua/Scripts 파일의 목적지(= Binaries/Win64/Mods 기준 상대경로) 계산.
/// 원본이 `.../Mods/<이름>/...` 트리를 동봉했으면 마지막 `Mods` 뒤의 `<이름>/...`만 취해
/// 로더가 스캔하는 `Mods/<이름>/` 바로 밑에 오도록 평탄화한다(Win 변형·id 껍데기 제거).
/// 그런 마커가 없으면 `<id>/<rel>`로 폴백(모드가 이름 폴더 없이 배포된 경우).
fn lua_dst_rel(rel: &Path, id: &str) -> PathBuf {
    let comps: Vec<_> = rel.components().map(|c| c.as_os_str().to_os_string()).collect();
    let mods_idx = comps.iter().rposition(|c| {
        c.to_str().map(|s| s.eq_ignore_ascii_case("Mods")).unwrap_or(false)
    });
    if let Some(idx) = mods_idx {
        if idx + 1 < comps.len() {
            let mut dst = PathBuf::new();
            for c in &comps[idx + 1..] { dst.push(c); }
            return dst;
        }
    }
    PathBuf::from(id).join(rel)
}

/// 모드 트리를 종류별 목적지로 라우팅. _P 미사용(§5).
pub fn plan_placements(mod_dir: &Path, id: &str) -> Vec<Placement> {
    let mut files = Vec::new();
    collect_files(mod_dir, &mut files);
    files.sort(); // 결정적 순회 + 중복 목적지 해소 우선순위 안정화
    let has_lua_component = files.iter().any(|f| {
        is_lua(f) || {
            let rel = f.strip_prefix(mod_dir).unwrap_or(f);
            has_segment(rel, "Scripts")
        }
    });

    let mut out = Vec::new();
    let mut seen: std::collections::HashSet<(u8, PathBuf)> = std::collections::HashSet::new();
    for f in &files {
        let n = name_lower(f);
        if n == "manifest.json" || n == "enabled.txt" { continue; } // 배포 제외
        let rel = f.strip_prefix(mod_dir).unwrap_or(f).to_path_buf();

        let placement = if is_lua(f) || has_segment(&rel, "Scripts") {
            Some(Placement { src: f.clone(), bucket: Bucket::LuaMods, dst_rel: lua_dst_rel(&rel, id), op: PlaceOp::Copy })
        } else if is_pak_asset(f) {
            let name = PathBuf::from(f.file_name().unwrap());
            Some(Placement { src: f.clone(), bucket: pak_bucket(&rel, has_lua_component), dst_rel: name, op: PlaceOp::Copy })
        } else if is_modconfig(f) {
            // 발견 위치가 곧 배치 위치. Lua 모드 트리(Mods/<이름>/) 안이면 그 폴더로 평탄화,
            // 아니면 BP 설정으로 보고 LogicMods 평평. 둘 다 심링크 연산(컨테이너 원본 우회).
            let (bucket, dst_rel) = if has_segment(&rel, "Mods") {
                (Bucket::LuaMods, lua_dst_rel(&rel, id))
            } else {
                (Bucket::LogicMods, PathBuf::from(f.file_name().unwrap()))
            };
            Some(Placement { src: f.clone(), bucket, dst_rel, op: PlaceOp::LinkModConfig })
        } else {
            None // 그 외 잡파일은 무시(배포 안 함).
        };
        // 동일 (버킷,목적지)로 매핑되는 중복(두 Mods 트리 등)은 처음 것만 채택.
        if let Some(p) = placement {
            let key = (p.bucket as u8, p.dst_rel.clone());
            if seen.insert(key) { out.push(p); }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    /// 진단용(외부 환경 의존): 실제 라이브러리의 모든 모드에 분류를 돌려 결과를 덤프하고
    /// 불변식을 검사한다. 기본 스위트 오염 방지 위해 #[ignore] — 명시 실행:
    /// `cargo test --lib classify::tests::dump_real_library -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn dump_real_library_placements() {
        let lib = std::path::PathBuf::from(std::env::var("HOME").unwrap())
            .join("Library/Application Support/PalworldModManager/mods");
        let entries = fs::read_dir(&lib).expect("라이브러리 폴더");
        let mut mod_dirs: Vec<PathBuf> = entries.flatten()
            .map(|e| e.path()).filter(|p| p.is_dir()).collect();
        mod_dirs.sort();
        assert!(!mod_dirs.is_empty(), "라이브러리에 모드 없음");

        for dir in &mod_dirs {
            let id = dir.file_name().unwrap().to_str().unwrap();
            let ps = plan_placements(dir, id);
            println!("\n=== {id} ({} placements) ===", ps.len());
            let mut lua_names: std::collections::BTreeSet<String> = Default::default();
            for p in &ps {
                let b = match p.bucket { Bucket::Mods => "~mods", Bucket::LogicMods => "LogicMods", Bucket::LuaMods => "Lua" };
                println!("  [{b:9}] {}", p.dst_rel.display());

                // 불변식 1: Lua 목적지에 id·Binaries 껍데기가 남아있으면 안 됨(이중중첩 회귀 방지).
                if p.bucket == Bucket::LuaMods {
                    let rel = &p.dst_rel;
                    assert!(!has_segment(rel, "Binaries"),
                        "{id}: Lua 목적지에 Binaries 잔존 → 이중중첩 회귀: {}", rel.display());
                    let first = rel.components().next().unwrap().as_os_str().to_str().unwrap();
                    lua_names.insert(first.to_string());
                }
                // 불변식 2: pak/모든 목적지는 절대경로화되지 않은 상대경로여야.
                assert!(p.dst_rel.is_relative(), "{id}: 목적지가 상대경로 아님");
            }
            // 불변식 3: (버킷,목적지) 유일(중복 배포 없음).
            let mut keys: Vec<(u8, &Path)> = ps.iter().map(|p| (p.bucket as u8, p.dst_rel.as_path())).collect();
            let total = keys.len();
            keys.sort(); keys.dedup();
            assert_eq!(keys.len(), total, "{id}: 중복 (버킷,목적지) 존재");
            if !lua_names.is_empty() {
                println!("  → mods.txt 이름: {lua_names:?}");
            }
        }
    }

    fn touch(p: &Path) { fs::create_dir_all(p.parent().unwrap()).unwrap(); fs::write(p, b"x").unwrap(); }
    fn find<'a>(v: &'a [Placement], name: &str) -> &'a Placement {
        v.iter().find(|p| p.src.file_name().unwrap().to_str().unwrap() == name).expect("placement")
    }

    #[test]
    fn structured_logicmods_and_mods_folders_route_by_structure() {
        let base = std::env::temp_dir().join("pmm_cls_struct");
        let _ = fs::remove_dir_all(&base);
        touch(&base.join("Content/Paks/LogicMods/Foo_P.pak"));
        touch(&base.join("Content/Paks/~mods/Skin.pak"));
        let p = plan_placements(&base, "m");
        assert_eq!(find(&p, "Foo_P.pak").bucket, Bucket::LogicMods);
        assert_eq!(find(&p, "Foo_P.pak").dst_rel, PathBuf::from("Foo_P.pak")); // 평평
        assert_eq!(find(&p, "Skin.pak").bucket, Bucket::Mods);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn hybrid_no_marker_pak_to_logicmods_lua_to_luamods() {
        let base = std::env::temp_dir().join("pmm_cls_hybrid");
        let _ = fs::remove_dir_all(&base);
        touch(&base.join("Paks/AutoHatch.pak"));
        touch(&base.join("Scripts/main.lua"));
        let p = plan_placements(&base, "AutoHatch");
        assert_eq!(find(&p, "AutoHatch.pak").bucket, Bucket::LogicMods, "하이브리드 pak은 LogicMods");
        let lua = find(&p, "main.lua");
        assert_eq!(lua.bucket, Bucket::LuaMods);
        assert_eq!(lua.dst_rel, PathBuf::from("AutoHatch/Scripts/main.lua"), "Lua는 모드별 폴더");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn lua_under_embedded_mods_tree_flattens_to_mod_name() {
        // 원본이 자기 안에 Binaries/<변형>/Mods/<이름>/ 트리를 동봉한 부류(AutoHatch_GamePass).
        // id 접두사·Binaries 껍데기를 벗기고 <이름>/... 로 평탄화해야 로더가 Mods/<이름>/ 밑에서 찾는다.
        let base = std::env::temp_dir().join("pmm_cls_embed");
        let _ = fs::remove_dir_all(&base);
        touch(&base.join("Binaries/WinGDK/Mods/AutoHatch/Scripts/main.lua"));
        touch(&base.join("Binaries/WinGDK/ue4ss/Mods/AutoHatch/Scripts/utils.lua"));
        touch(&base.join("Content/Paks/LogicMods/AutoHatch.pak"));
        let p = plan_placements(&base, "AutoHatch_GamePass");
        let lua = find(&p, "main.lua");
        assert_eq!(lua.bucket, Bucket::LuaMods);
        assert_eq!(lua.dst_rel, PathBuf::from("AutoHatch/Scripts/main.lua"),
            "동봉된 Mods/<이름> 트리는 <이름>/... 로 평탄화(id·Binaries 제거)");
        // 두 번째(ue4ss 트리) 파일도 같은 목적지 규칙
        assert_eq!(find(&p, "utils.lua").dst_rel, PathBuf::from("AutoHatch/Scripts/utils.lua"));
        assert_eq!(find(&p, "AutoHatch.pak").bucket, Bucket::LogicMods);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn duplicate_dst_from_two_mods_trees_is_deduped() {
        // AutoHatch_GamePass는 Mods/AutoHatch/ 와 ue4ss/Mods/AutoHatch/ 두 트리에
        // 동일 파일을 담아 같은 목적지로 충돌한다 → 목적지 기준 중복 제거.
        let base = std::env::temp_dir().join("pmm_cls_dupe");
        let _ = fs::remove_dir_all(&base);
        touch(&base.join("Binaries/WinGDK/Mods/AutoHatch/Scripts/main.lua"));
        touch(&base.join("Binaries/WinGDK/ue4ss/Mods/AutoHatch/Scripts/main.lua"));
        let p = plan_placements(&base, "AutoHatch_GamePass");
        let hits: Vec<_> = p.iter()
            .filter(|x| x.bucket == Bucket::LuaMods
                && x.dst_rel == PathBuf::from("AutoHatch/Scripts/main.lua"))
            .collect();
        assert_eq!(hits.len(), 1, "동일 (버킷,목적지)는 하나만");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn bare_pak_defaults_to_mods_ignoring_p_suffix() {
        let base = std::env::temp_dir().join("pmm_cls_bare");
        let _ = fs::remove_dir_all(&base);
        touch(&base.join("Something_P.pak")); // _P 있어도 마커·Lua 없으면 ~mods
        let p = plan_placements(&base, "m");
        assert_eq!(find(&p, "Something_P.pak").bucket, Bucket::Mods, "_P는 신호 아님");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn modconfig_goes_to_logicmods_and_manifest_excluded() {
        let base = std::env::temp_dir().join("pmm_cls_cfg");
        let _ = fs::remove_dir_all(&base);
        touch(&base.join("Foo_P.modconfig.json"));
        touch(&base.join("manifest.json"));
        let p = plan_placements(&base, "m");
        assert_eq!(find(&p, "Foo_P.modconfig.json").bucket, Bucket::LogicMods);
        assert!(!p.iter().any(|x| x.src.file_name().unwrap() == "manifest.json"), "manifest 배포 제외");
        // modconfig는 심링크 연산, pak은 복사 연산으로 표시돼야 한다.
        assert_eq!(find(&p, "Foo_P.modconfig.json").op, PlaceOp::LinkModConfig);
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn lua_mod_modconfig_routes_to_lua_folder_as_symlink() {
        // Lua 모드가 자기 Mods/<이름>/ 트리에 modconfig를 동봉하면
        // LogicMods 평평이 아니라 Lua 폴더(<이름>/<파일>)로 가야 하고, 연산은 심링크다.
        let base = std::env::temp_dir().join("pmm_cls_lua_cfg");
        let _ = fs::remove_dir_all(&base);
        touch(&base.join("Binaries/Win64/Mods/DekMinimap/Scripts/main.lua"));
        touch(&base.join("Binaries/Win64/Mods/DekMinimap/DekMinimap.modconfig.json"));
        let p = plan_placements(&base, "DekMinimap_Nexus");
        let cfg = find(&p, "DekMinimap.modconfig.json");
        assert_eq!(cfg.bucket, Bucket::LuaMods, "Lua 모드 modconfig는 Lua 버킷");
        assert_eq!(cfg.dst_rel, PathBuf::from("DekMinimap/DekMinimap.modconfig.json"),
            "모드명 폴더 top-level로 평탄화(id·Binaries 제거)");
        assert_eq!(cfg.op, PlaceOp::LinkModConfig, "modconfig는 심링크 연산");
        // 일반 lua 파일은 복사 연산
        assert_eq!(find(&p, "main.lua").op, PlaceOp::Copy, "일반 파일은 복사");
        let _ = fs::remove_dir_all(&base);
    }
}
