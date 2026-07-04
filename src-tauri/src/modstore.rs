use crate::classify::{self, Bucket};
use crate::staging;
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::{Path, PathBuf};

/// JSON 배열 파일 읽기(없거나 깨졌으면 빈 벡터).
pub fn read_id_list(path: &Path) -> Vec<String> {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<Vec<String>>(&s).ok())
        .unwrap_or_default()
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct DeployedFile {
    pub bucket: String,
    pub path: String,
}

pub type DeployedMap = std::collections::BTreeMap<String, Vec<DeployedFile>>;

/// deployed.json 읽기. 없거나 구형(Vec<String>)/손상 시 빈 맵.
pub fn read_deployed(path: &Path) -> DeployedMap {
    fs::read_to_string(path)
        .ok()
        .and_then(|s| serde_json::from_str::<DeployedMap>(&s).ok())
        .unwrap_or_default()
}

/// deployed.json 원자적 쓰기(부모 보장).
pub fn write_deployed(path: &Path, map: &DeployedMap) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let json = serde_json::to_string_pretty(map).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, json).map_err(|e| e.to_string())?;
    fs::rename(&tmp, path).map_err(|e| e.to_string())?;
    Ok(())
}

/// active 집합 갱신: 활성화면 없을 때만 끝에 추가(순서=로드 순서), 비활성화면 제거.
pub fn set_enabled(active: &mut Vec<String>, id: &str, enabled: bool) {
    let pos = active.iter().position(|x| x == id);
    match (enabled, pos) {
        (true, None) => active.push(id.to_string()),
        (false, Some(i)) => {
            active.remove(i);
        }
        _ => {}
    }
}

fn bucket_str(b: Bucket) -> &'static str {
    match b { Bucket::Mods => "mods", Bucket::LogicMods => "logicmods", Bucket::LuaMods => "lua" }
}

fn bucket_dir<'a>(s: &str, mods: &'a Path, logic: &'a Path, lua: &'a Path) -> Option<&'a Path> {
    match s { "mods" => Some(mods), "logicmods" => Some(logic), "lua" => Some(lua), _ => None }
}

/// 컨테이너 ModConfigs 원본(모드별 폴더 격리) 경로.
fn modconfig_original(modconfigs_dir: &Path, id: &str, dst_rel: &Path) -> PathBuf {
    let file = dst_rel.file_name().unwrap_or(dst_rel.as_os_str());
    modconfigs_dir.join(id).join(file)
}

/// ModConfigMenu 설정 배치: 컨테이너에 원본을 seed(없을 때만; 있으면 사용자 저장값 보존)하고
/// 번들 발견 위치에는 그 원본을 가리키는 심링크만 남긴다(읽기전용 번들의 저장 실패 우회).
fn place_modconfig(src: &Path, bundle_dst: &Path, original: &Path) -> Result<(), String> {
    if let Some(parent) = original.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    if !original.exists() {
        fs::copy(src, original).map_err(|e| format!("seed modconfig {}: {e}", original.display()))?;
    }
    if let Some(parent) = bundle_dst.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    // 기존 심링크/파일 제거 후 재링크(멱등).
    let _ = fs::remove_file(bundle_dst);
    std::os::unix::fs::symlink(original, bundle_dst)
        .map_err(|e| format!("symlink {} → {}: {e}", bundle_dst.display(), original.display()))?;
    Ok(())
}

/// 라이브러리 → 번들 3버킷 동기화. prev 소유 파일만 회수(외부 파일 불가침).
/// `modconfigs_dir` = 컨테이너 ModConfigs 베이스(설정 원본 격리·심링크 대상).
pub fn reconcile_bundle(
    library_mods: &Path,
    mods_dir: &Path,
    logicmods_dir: &Path,
    lua_dir: &Path,
    modconfigs_dir: &Path,
    active: &[String],
    prev: &DeployedMap,
) -> Result<DeployedMap, String> {
    for id in active {
        if !library_mods.join(id).is_dir() {
            return Err(format!("Mod not found in library: {id}"));
        }
    }
    for dir in [mods_dir, logicmods_dir, lua_dir] {
        fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    }

    // 회수: prev 전 항목의 기록 파일만 삭제(재배포도 깨끗하게 하려 active 포함 전부 제거 후 재배포).
    for (_id, files) in prev {
        // 모드별 Lua 폴더는 통째 제거(우리 소유). 폴더명=기록 경로 첫 세그먼트
        // (신형=모드명, 구형=id). id 가정 대신 실제 배치 경로에서 유도해 구형·신형 모두 정리.
        let mut lua_roots: std::collections::BTreeSet<PathBuf> = Default::default();
        for f in files {
            if f.bucket == "lua" {
                if let Some(first) = Path::new(&f.path).components().next() {
                    lua_roots.insert(PathBuf::from(first.as_os_str()));
                }
            }
            if let Some(base) = bucket_dir(&f.bucket, mods_dir, logicmods_dir, lua_dir) {
                let _ = fs::remove_file(base.join(&f.path));
            }
        }
        for root in lua_roots {
            let _ = fs::remove_dir_all(lua_dir.join(root));
        }
    }

    // 배포: active를 분류대로 복사·기록.
    let mut new_map: DeployedMap = Default::default();
    let mut lua_mod_names: Vec<String> = Vec::new(); // active 순서, 폴더명 기준 중복 제거
    for id in active {
        let placements = classify::plan_placements(&library_mods.join(id), id);
        let mut recorded = Vec::new();
        for p in placements {
            let base = match p.bucket { Bucket::Mods => mods_dir, Bucket::LogicMods => logicmods_dir, Bucket::LuaMods => lua_dir };
            let dst = base.join(&p.dst_rel);
            if let Some(parent) = dst.parent() {
                fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            match p.op {
                classify::PlaceOp::Copy => {
                    fs::copy(&p.src, &dst).map_err(|e| format!("copy {} → {}: {e}", p.src.display(), dst.display()))?;
                }
                classify::PlaceOp::LinkModConfig => {
                    // 원본은 컨테이너에 seed(보존), 번들엔 심링크만. deployed.json엔 번들 심링크만 기록.
                    let original = modconfig_original(modconfigs_dir, id, &p.dst_rel);
                    place_modconfig(&p.src, &dst, &original)?;
                }
            }
            // mods.txt 이름 = Lua 목적지의 첫 세그먼트(실제 폴더명). id가 아님.
            if p.bucket == Bucket::LuaMods {
                if let Some(first) = p.dst_rel.components().next() {
                    let name = first.as_os_str().to_string_lossy().to_string();
                    if !lua_mod_names.contains(&name) { lua_mod_names.push(name); }
                }
            }
            recorded.push(DeployedFile { bucket: bucket_str(p.bucket).to_string(), path: p.dst_rel.to_string_lossy().to_string() });
        }
        new_map.insert(id.clone(), recorded);
    }

    // mods.txt = 실제 배치된 Lua 모드 폴더명(로드 순서=active 순서).
    let entries: Vec<(String, bool)> = lua_mod_names.into_iter().map(|n| (n, true)).collect();
    staging::write_mods_txt(lua_dir, &entries).map_err(|e| e.to_string())?;

    Ok(new_map)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, body: &[u8]) {
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).unwrap();
        }
        fs::write(path, body).unwrap();
    }

    #[test]
    fn set_enabled_appends_and_removes_in_order() {
        let mut active: Vec<String> = vec![];
        set_enabled(&mut active, "a", true);
        set_enabled(&mut active, "b", true);
        set_enabled(&mut active, "a", true); // 중복 추가 안 함
        assert_eq!(active, vec!["a".to_string(), "b".to_string()]);
        set_enabled(&mut active, "a", false);
        assert_eq!(active, vec!["b".to_string()]);
    }

    #[test]
    fn deployed_map_roundtrip_and_legacy_migration() {
        use super::{DeployedFile, DeployedMap, read_deployed, write_deployed};
        let base = std::env::temp_dir().join("pmm_deployed_map");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let path = base.join("deployed.json");

        let mut map: DeployedMap = Default::default();
        map.insert("Skin".into(), vec![DeployedFile { bucket: "mods".into(), path: "Skin.pak".into() }]);
        write_deployed(&path, &map).unwrap();
        assert_eq!(read_deployed(&path), map);

        // 구형 Vec<String> 형식 → 빈 맵으로 흡수(마이그레이션)
        fs::write(&path, r#"["A","B"]"#).unwrap();
        assert!(read_deployed(&path).is_empty());

        // 파일 없음 → 빈 맵
        assert!(read_deployed(&base.join("missing.json")).is_empty());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn reconcile_bundle_routes_by_type_and_retracts_only_owned() {
        use super::{reconcile_bundle, DeployedMap};
        let base = std::env::temp_dir().join("pmm_reconcile_bundle");
        let _ = fs::remove_dir_all(&base);
        let lib = base.join("library");
        let mods = base.join("~mods");
        let logic = base.join("LogicMods");
        let lua = base.join("LuaMods");
        let cfgs = base.join("ModConfigs");

        // 라이브러리: 스킨(pak) / 하이브리드(pak+lua)
        write(&lib.join("Skin/Skin.pak"), b"skin");
        write(&lib.join("Hatch/Paks/Hatch.pak"), b"pak");
        write(&lib.join("Hatch/Scripts/main.lua"), b"lua");

        // 사용자가 손으로 둔 외부 스킨(매니저 소유 아님) — 보존돼야 함
        write(&mods.join("External.pak"), b"ext");

        let prev: DeployedMap = Default::default();
        let dep = reconcile_bundle(&lib, &mods, &logic, &lua, &cfgs,
            &["Skin".to_string(), "Hatch".to_string()], &prev).unwrap();

        assert!(mods.join("Skin.pak").is_file(), "스킨은 ~mods 평평");
        assert!(logic.join("Hatch.pak").is_file(), "하이브리드 pak은 LogicMods");
        assert!(lua.join("Hatch/Scripts/main.lua").is_file(), "Lua는 모드별 폴더");
        assert!(mods.join("External.pak").is_file(), "외부 파일 보존");
        assert_eq!(fs::read_to_string(lua.join("mods.txt")).unwrap(), "Hatch:1\n");

        // Skin 회수 → Skin.pak만 삭제, External·Hatch 보존
        let dep2 = reconcile_bundle(&lib, &mods, &logic, &lua, &cfgs,
            &["Hatch".to_string()], &dep).unwrap();
        assert!(!mods.join("Skin.pak").exists(), "Skin 회수");
        assert!(mods.join("External.pak").is_file(), "외부 파일 여전히 보존");
        assert!(logic.join("Hatch.pak").is_file(), "Hatch 유지");
        assert!(dep2.contains_key("Hatch") && !dep2.contains_key("Skin"));
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn reconcile_modconfig_seeds_container_and_symlinks_bundle() {
        use super::{reconcile_bundle, DeployedMap};
        let base = std::env::temp_dir().join("pmm_reconcile_modcfg_seed");
        let _ = fs::remove_dir_all(&base);
        let lib = base.join("library");
        let mods = base.join("~mods");
        let logic = base.join("LogicMods");
        let lua = base.join("LuaMods");
        let cfgs = base.join("ModConfigs"); // 컨테이너 원본 베이스

        // BP 모드: pak + 형제 modconfig(기본값)
        write(&lib.join("Mini/Content/Paks/LogicMods/Mini_P.pak"), b"pak");
        write(&lib.join("Mini/Content/Paks/LogicMods/Mini.modconfig.json"), b"DEFAULT");

        let prev: DeployedMap = Default::default();
        let dep = reconcile_bundle(&lib, &mods, &logic, &lua, &cfgs,
            &["Mini".to_string()], &prev).unwrap();

        // 1) 컨테이너에 원본이 seed(모드별 폴더 격리)
        let orig = cfgs.join("Mini/Mini.modconfig.json");
        assert!(orig.is_file(), "컨테이너에 원본 seed");
        assert_eq!(fs::read_to_string(&orig).unwrap(), "DEFAULT", "기본값 이관");

        // 2) 번들엔 심링크만(원본을 가리킴)
        let bundle_cfg = logic.join("Mini.modconfig.json");
        assert!(fs::symlink_metadata(&bundle_cfg).unwrap().file_type().is_symlink(),
            "번들 modconfig는 심링크");
        assert_eq!(fs::read_link(&bundle_cfg).unwrap(), orig, "심링크는 컨테이너 원본을 가리킴");

        // 3) deployed.json엔 번들 심링크만 기록(컨테이너 원본은 기록 안 함 → 회수 대상 아님)
        let recorded = &dep["Mini"];
        assert!(recorded.iter().any(|f| f.path == "Mini.modconfig.json" && f.bucket == "logicmods"),
            "번들 심링크 기록");
        assert!(!recorded.iter().any(|f| f.path.contains("ModConfigs")),
            "컨테이너 원본 경로는 기록하지 않음");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn reconcile_preserves_user_saved_modconfig_on_redeploy() {
        use super::{reconcile_bundle, DeployedMap};
        let base = std::env::temp_dir().join("pmm_reconcile_modcfg_preserve");
        let _ = fs::remove_dir_all(&base);
        let lib = base.join("library");
        let (mods, logic, lua, cfgs) =
            (base.join("~mods"), base.join("LogicMods"), base.join("LuaMods"), base.join("ModConfigs"));
        write(&lib.join("Mini/Content/Paks/LogicMods/Mini.modconfig.json"), b"DEFAULT");

        // 사용자가 이미 인게임에서 저장한 값이 컨테이너에 있음
        write(&cfgs.join("Mini/Mini.modconfig.json"), b"USER_SAVED");

        let prev: DeployedMap = Default::default();
        reconcile_bundle(&lib, &mods, &logic, &lua, &cfgs,
            &["Mini".to_string()], &prev).unwrap();

        // 재배포해도 사용자 저장값 보존(라이브러리 기본값으로 되돌리지 않음)
        assert_eq!(fs::read_to_string(cfgs.join("Mini/Mini.modconfig.json")).unwrap(),
            "USER_SAVED", "기존 사용자 저장값 보존");
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn reconcile_retract_removes_symlink_preserves_container_original() {
        use super::{reconcile_bundle, DeployedMap};
        let base = std::env::temp_dir().join("pmm_reconcile_modcfg_retract");
        let _ = fs::remove_dir_all(&base);
        let lib = base.join("library");
        let (mods, logic, lua, cfgs) =
            (base.join("~mods"), base.join("LogicMods"), base.join("LuaMods"), base.join("ModConfigs"));
        write(&lib.join("Mini/Content/Paks/LogicMods/Mini.modconfig.json"), b"DEFAULT");

        let prev: DeployedMap = Default::default();
        let dep = reconcile_bundle(&lib, &mods, &logic, &lua, &cfgs,
            &["Mini".to_string()], &prev).unwrap();
        // 사용자가 저장(컨테이너 원본 변경)
        fs::write(cfgs.join("Mini/Mini.modconfig.json"), b"USER_SAVED").unwrap();

        // 회수(비활성) → 번들 심링크는 사라지고, 컨테이너 원본은 보존
        let dep2 = reconcile_bundle(&lib, &mods, &logic, &lua, &cfgs, &[], &dep).unwrap();
        assert!(!logic.join("Mini.modconfig.json").exists(), "번들 심링크 회수");
        assert_eq!(fs::read_to_string(cfgs.join("Mini/Mini.modconfig.json")).unwrap(),
            "USER_SAVED", "컨테이너 원본(사용자 저장값)은 회수해도 보존");
        assert!(dep2.is_empty());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn reconcile_uses_mod_name_not_id_for_embedded_lua_tree_and_retracts_cleanly() {
        use super::{reconcile_bundle, DeployedMap};
        let base = std::env::temp_dir().join("pmm_reconcile_embed");
        let _ = fs::remove_dir_all(&base);
        let lib = base.join("library");
        let mods = base.join("~mods");
        let logic = base.join("LogicMods");
        let lua = base.join("LuaMods");
        let cfgs = base.join("ModConfigs");

        // 라이브러리 id=AutoHatch_GamePass 인데 내부는 Mods/AutoHatch/ 트리 동봉(두 트리 중복)
        write(&lib.join("AutoHatch_GamePass/Binaries/WinGDK/Mods/AutoHatch/Scripts/main.lua"), b"lua");
        write(&lib.join("AutoHatch_GamePass/Binaries/WinGDK/ue4ss/Mods/AutoHatch/Scripts/main.lua"), b"lua");
        write(&lib.join("AutoHatch_GamePass/Content/Paks/LogicMods/AutoHatch.pak"), b"pak");

        let prev: DeployedMap = Default::default();
        let dep = reconcile_bundle(&lib, &mods, &logic, &lua, &cfgs,
            &["AutoHatch_GamePass".to_string()], &prev).unwrap();

        // 폴더·mods.txt 모두 모드명(AutoHatch) 기준, id 접두사 없음
        assert!(lua.join("AutoHatch/Scripts/main.lua").is_file(), "모드명 폴더로 평탄화");
        assert!(!lua.join("AutoHatch_GamePass").exists(), "id 접두 폴더 생기지 않음");
        assert_eq!(fs::read_to_string(lua.join("mods.txt")).unwrap(), "AutoHatch:1\n",
            "mods.txt는 모드명 사용");

        // 회수 → 모드명 폴더 통째 정리(빈 폴더 잔재 없음)
        let dep2 = reconcile_bundle(&lib, &mods, &logic, &lua, &cfgs, &[], &dep).unwrap();
        assert!(!lua.join("AutoHatch").exists(), "회수 시 Lua 폴더 통째 제거");
        assert!(dep2.is_empty());
        let _ = fs::remove_dir_all(&base);
    }
}
