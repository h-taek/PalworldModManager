use crate::manifest::{self, Manifest};
use std::fs;
use std::path::{Path, PathBuf};

/// 디렉터리 재귀 복사. skip에 든 파일명(소문자 비교)은 제외. modstore에서도 사용.
pub(crate) fn copy_tree(src: &Path, dst: &Path, skip: &[&str]) -> std::io::Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)? {
        let entry = entry?;
        let name = entry.file_name();
        let from = entry.path();
        let to = dst.join(&name);
        if from.is_dir() {
            copy_tree(&from, &to, skip)?;
        } else {
            let lower = name.to_string_lossy().to_ascii_lowercase();
            if !skip.contains(&lower.as_str()) {
                fs::copy(&from, &to)?;
            }
        }
    }
    Ok(())
}

/// `enabled.txt`(대소문자 무관)를 트리에서 모두 제거.
/// research/03: enabled.txt는 mods.txt와 무관하게 자동 활성화 → 매니저 토글 무력화 방지.
fn strip_enabled_txt(dir: &Path) -> std::io::Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.is_dir() {
            strip_enabled_txt(&path)?;
        } else if path
            .file_name()
            .and_then(|n| n.to_str())
            .map(|n| n.eq_ignore_ascii_case("enabled.txt"))
            .unwrap_or(false)
        {
            fs::remove_file(&path)?;
        }
    }
    Ok(())
}

/// 정규화된 모드 디렉터리를 라이브러리 `mods_dir/<id>/`로 들인다(원자적 교체).
/// - enabled.txt 제거 · manifest.json 보장 · 동일 id면 덮어씀.
/// - force_id=Some이면 manifest의 id를 그것으로 강제(업데이트: 기존 id 유지).
fn ingest(
    mods_dir: &Path,
    src_mod: &Path,
    fallback_id: &str,
    force_id: Option<&str>,
) -> Result<Manifest, String> {
    let mut manifest = manifest::load_or_synthesize(src_mod, fallback_id)?;
    if let Some(fid) = force_id {
        manifest.id = manifest::sanitize_id(fid);
    }
    let id = manifest.id.clone();
    fs::create_dir_all(mods_dir).map_err(|e| e.to_string())?;
    let dest = mods_dir.join(&id);
    let tmp = mods_dir.join(format!(".{id}.tmp"));
    let _ = fs::remove_dir_all(&tmp);

    copy_tree(src_mod, &tmp, &["enabled.txt"]).map_err(|e| e.to_string())?;
    strip_enabled_txt(&tmp).map_err(|e| e.to_string())?;
    let mf_json = serde_json::to_string_pretty(&manifest).map_err(|e| e.to_string())?;
    fs::write(tmp.join("manifest.json"), mf_json).map_err(|e| e.to_string())?;

    let _ = fs::remove_dir_all(&dest);
    fs::rename(&tmp, &dest).map_err(|e| e.to_string())?;
    Ok(manifest)
}

/// 단일 `.pak` 파일 가져오기: 임시 폴더에 pak을 담아 모드로 ingest한다.
/// id 후보 = pak 파일명 stem. 단일 레거시면 이후 `convert_pak_in_place`가 3종으로 변환.
/// (변환 결과는 mod_root/Paks/로 정규화되므로 여기서는 stage 루트에 그대로 둔다.)
pub fn import_pak_file(mods_dir: &Path, pak_path: &Path) -> Result<Manifest, String> {
    if !pak_path.is_file() {
        return Err(format!("Not a file: {}", pak_path.display()));
    }
    let stem = pak_path.file_stem().and_then(|n| n.to_str()).unwrap_or("mod");
    let file_name = pak_path
        .file_name()
        .and_then(|n| n.to_str())
        .ok_or("Invalid pak filename")?;
    fs::create_dir_all(mods_dir).map_err(|e| e.to_string())?;
    let stage = mods_dir.join(".pakimport.tmp");
    let _ = fs::remove_dir_all(&stage);
    fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
    if let Err(e) = fs::copy(pak_path, stage.join(file_name)).map_err(|e| e.to_string()) {
        let _ = fs::remove_dir_all(&stage);
        return Err(e);
    }
    let result = ingest(mods_dir, &stage, stem, None);
    let _ = fs::remove_dir_all(&stage);
    result
}

/// 폴더 가져오기. id 후보 = 폴더명.
pub fn import_folder(mods_dir: &Path, source: &Path) -> Result<Manifest, String> {
    if !source.is_dir() {
        return Err(format!("Not a folder: {}", source.display()));
    }
    let fallback = source.file_name().and_then(|n| n.to_str()).unwrap_or("mod");
    ingest(mods_dir, source, fallback, None)
}

/// 라이브러리의 모든 모드 매니페스트(id 정렬). 임시(`.`) 디렉터리 무시.
pub fn list(mods_dir: &Path) -> Result<Vec<Manifest>, String> {
    let mut out = Vec::new();
    if !mods_dir.exists() {
        return Ok(out);
    }
    for entry in fs::read_dir(mods_dir).map_err(|e| e.to_string())? {
        let entry = entry.map_err(|e| e.to_string())?;
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with('.') {
            continue;
        }
        out.push(manifest::load_or_synthesize(&path, &name_str)?);
    }
    out.sort_by(|a, b| a.id.cmp(&b.id));
    Ok(out)
}

/// 라이브러리에서 모드 원본 제거(없으면 무시).
pub fn remove(mods_dir: &Path, id: &str) -> Result<(), String> {
    let dest = mods_dir.join(id);
    if dest.exists() {
        fs::remove_dir_all(&dest).map_err(|e| e.to_string())?;
    }
    Ok(())
}

/// dir에 항목이 하나뿐이고 그게 디렉터리면 그 경로 반환(zip 루트 정규화용).
/// macOS Finder zip의 `__MACOSX` 및 `.`으로 시작하는 항목은 junk로 무시.
fn single_subdir(dir: &Path) -> Option<std::path::PathBuf> {
    let real: Vec<_> = fs::read_dir(dir)
        .ok()?
        .flatten()
        .filter(|e| {
            let name = e.file_name();
            let s = name.to_string_lossy();
            s != "__MACOSX" && !s.starts_with('.')
        })
        .collect();
    if real.len() == 1 && real[0].path().is_dir() {
        Some(real[0].path())
    } else {
        None
    }
}

/// .zip 가져오기: 임시 해제 → (단일 최상위 폴더면 그걸, 아니면 루트를) 모드로 ingest.
pub fn import_zip(mods_dir: &Path, zip_path: &Path) -> Result<Manifest, String> {
    let file = fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Failed to open zip: {e}"))?;
    fs::create_dir_all(mods_dir).map_err(|e| e.to_string())?;
    let stage = mods_dir.join(".unzip.tmp");
    let _ = fs::remove_dir_all(&stage);
    fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
    if let Err(e) = archive
        .extract(&stage)
        .map_err(|e| format!("Failed to extract zip: {e}"))
    {
        let _ = fs::remove_dir_all(&stage);
        return Err(e);
    }
    // macOS Finder zip이 생성하는 __MACOSX 디렉터리를 즉시 제거.
    // flat-zip 경로(stage == mod_root)에서도 라이브러리로 복사되지 않게 한다.
    let _ = fs::remove_dir_all(stage.join("__MACOSX"));

    let mod_root = single_subdir(&stage).unwrap_or_else(|| stage.clone());

    // 평평한 zip(최상위 단일 폴더 없음)이면 모드 루트 = 스테이징 폴더이므로
    // 폴더명(".unzip.tmp") 대신 zip 파일명을 id 후보로 쓴다.
    let fallback = if mod_root == stage {
        zip_path
            .file_stem()
            .and_then(|n| n.to_str())
            .unwrap_or("mod")
            .to_string()
    } else {
        mod_root
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("mod")
            .to_string()
    };

    let result = ingest(mods_dir, &mod_root, &fallback, None);
    let _ = fs::remove_dir_all(&stage);
    result
}

/// 업데이트용: 다운로드한 zip을 **기존 id로 강제 교체**(원자적). zip 폴더명이 달라도 새 모드를
/// 만들지 않는다. import_zip과 동일하게 단일 최상위 폴더/__MACOSX 정규화 후 ingest(force_id).
pub fn replace_from_zip(mods_dir: &Path, id: &str, zip_path: &Path) -> Result<Manifest, String> {
    let file = fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Failed to open zip: {e}"))?;
    fs::create_dir_all(mods_dir).map_err(|e| e.to_string())?;
    let stage = mods_dir.join(".unzip.tmp");
    let _ = fs::remove_dir_all(&stage);
    fs::create_dir_all(&stage).map_err(|e| e.to_string())?;
    if let Err(e) = archive
        .extract(&stage)
        .map_err(|e| format!("Failed to extract zip: {e}"))
    {
        let _ = fs::remove_dir_all(&stage);
        return Err(e);
    }
    let _ = fs::remove_dir_all(stage.join("__MACOSX"));
    let mod_root = single_subdir(&stage).unwrap_or_else(|| stage.clone());
    let result = ingest(mods_dir, &mod_root, id, Some(id));
    let _ = fs::remove_dir_all(&stage);
    result
}

/// mod_root의 단일 레거시 pak을 3종(IoStore)으로 변환해 mod_root/Paks/에 배치하고 원본 제거.
/// 이미 3종이면 호출측(import)에서 스킵. mod_name = 출력 basename(로더 발견 규약, spec §7-5).
// 가정: 모드당 변환 대상 단일 레거시 pak은 하나(스펙 범위). 여러 개면 첫 번째만 변환됨.
pub fn convert_pak_in_place(
    mod_root: &Path,
    retoc_bin: &Path,
    mod_name: &str,
) -> Result<crate::pak_convert::ConvertResult, String> {
    use crate::pak_convert::{is_non_asset, ConvertResult};
    let input = manifest::find_single_legacy_pak(mod_root).ok_or("변환할 단일 pak을 찾지 못함")?;
    let tmp = mod_root.join(".convert.tmp");
    let _ = fs::remove_dir_all(&tmp);
    fs::create_dir_all(&tmp).map_err(|e| e.to_string())?;
    let out_utoc = tmp.join(format!("{mod_name}.utoc"));

    // 1) 직접 시도(깨끗한 pak).
    let mut removed: Vec<String> = Vec::new();
    let args = crate::retoc::build_to_zen_args(&input, &out_utoc, None);
    let mut run = crate::retoc::run(retoc_bin, &args)?;

    // 2) 실패 시 repak 추출 → 비에셋 + UE4-era 에셋 제거 → to-zen <dir>.
    if !run.ok {
        // 부분 산출물 제거 후 재시도 준비.
        for ext in ["pak", "ucas", "utoc"] {
            let _ = fs::remove_file(tmp.join(format!("{mod_name}.{ext}")));
        }
        let extract = tmp.join("extract");
        fs::create_dir_all(&extract).map_err(|e| e.to_string())?;
        let outcome = extract_pak(&input, &extract)?;
        // junk = 추출된 파일 중 비에셋(txt 등) → 추출 디렉터리에서도 제거
        let junk: Vec<String> = outcome.written.iter().filter(|e| is_non_asset(e)).cloned().collect();
        for r in &junk {
            let _ = fs::remove_file(extract.join(r));
        }
        // removed = junk + UE4-era 스킵 (사용자에게 모두 보고)
        removed = [junk, outcome.ue4_skipped].concat();
        let args = crate::retoc::build_to_zen_args(&extract, &out_utoc, None);
        run = crate::retoc::run(retoc_bin, &args)?;
    }

    if !run.ok {
        let _ = fs::remove_dir_all(&tmp);
        return Ok(ConvertResult::NeedsUserDecision { removed, stderr: run.stderr });
    }

    // 3) 3종을 mod_root/Paks/로 이동(원자적) — 실패 시 이미 이동된 파일 롤백.
    let paks = mod_root.join("Paks");
    fs::create_dir_all(&paks).map_err(|e| e.to_string())?;
    let mut files: Vec<PathBuf> = Vec::new();
    let mut moved_dsts: Vec<PathBuf> = Vec::new();
    for ext in ["pak", "ucas", "utoc"] {
        let src = tmp.join(format!("{mod_name}.{ext}"));
        let dst = paks.join(format!("{mod_name}.{ext}"));
        if let Err(e) = fs::rename(&src, &dst) {
            for d in &moved_dsts { let _ = fs::remove_file(d); }
            let _ = fs::remove_dir_all(&tmp);
            return Err(e.to_string());
        }
        moved_dsts.push(dst.clone());
        files.push(dst);
    }
    // 3종 모두 성공 후에 원본 제거 — 단, 원본 경로가 방금 배치한 출력과 같으면(모드 id == pak stem이고
    // 원본이 이미 Paks/에 있던 경우) 삭제하면 변환 결과가 사라진다. 그 경우 삭제 생략.
    if !moved_dsts.iter().any(|d| d == &input) {
        let _ = fs::remove_file(&input);
    }
    let _ = fs::remove_dir_all(&tmp);
    Ok(ConvertResult::Converted { files, removed })
}

/// extract_pak의 반환: 실제 기록된 파일 목록 + UE4-era로 스킵된 파일 목록.
struct ExtractOutcome {
    written: Vec<String>,
    ue4_skipped: Vec<String>,
}

/// 레거시 pak을 dest 아래로 추출한다.
/// repak 0.2.3 API: PakBuilder::new().reader(&mut reader) → PakReader.
/// files() → Vec<String>, get(path, reader) → Vec<u8>.
///
/// UE4-era .uasset(legacy_version > -8)은 retoc가 처리하지 못하므로 추출에서 제외.
/// 같은 스템의 .uexp/.ubulk도 함께 제외한다. 스킵된 목록은 ue4_skipped에 담아 반환.
fn extract_pak(pak: &Path, dest: &Path) -> Result<ExtractOutcome, String> {
    let bytes = fs::read(pak).map_err(|e| e.to_string())?;
    let mut reader = std::io::Cursor::new(&bytes);
    let pak_rdr = repak::PakBuilder::new()
        .reader(&mut reader)
        .map_err(|e| format!("repak open 실패: {e}"))?;

    // 1단계: 모든 파일 데이터를 읽어 메모리에 보관.
    let mut file_data: Vec<(String, Vec<u8>)> = Vec::new();
    for rel in pak_rdr.files() {
        let data = pak_rdr
            .get(&rel, &mut std::io::Cursor::new(&bytes))
            .map_err(|e| format!("repak get {rel} 실패: {e}"))?;
        file_data.push((rel, data));
    }

    // 2단계: UE4-era .uasset 스템을 스킵 목록에 등록.
    let mut skip_stems: std::collections::HashSet<String> = std::collections::HashSet::new();
    for (rel, data) in &file_data {
        if rel.to_ascii_lowercase().ends_with(".uasset") && is_ue4_legacy_uasset(data) {
            // 확장자 제거한 스템을 스킵 목록에 추가.
            let stem_end = rel.rfind('.').unwrap_or(rel.len());
            skip_stems.insert(rel[..stem_end].to_string());
        }
    }

    // 3단계: 스킵 목록에 없는 파일만 추출. 스킵된 파일은 ue4_skipped에 수집.
    let mut written = Vec::new();
    let mut ue4_skipped = Vec::new();
    for (rel, data) in file_data {
        let stem_end = rel.rfind('.').unwrap_or(rel.len());
        let stem = &rel[..stem_end];
        if skip_stems.contains(stem) {
            ue4_skipped.push(rel); // UE4-era 에셋 쌍(uasset/uexp/ubulk) 제외 — 목록에 기록
            continue;
        }
        let out = dest.join(&rel);
        if let Some(parent) = out.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        fs::write(&out, &data).map_err(|e| e.to_string())?;
        written.push(rel);
    }
    Ok(ExtractOutcome { written, ue4_skipped })
}

/// UE4-era .uasset 판별: magic이 있고 legacy_version > -8이면 구형(retoc 미지원).
/// UE5 에셋은 legacy_version ≤ -8. 크기가 8 미만이면 판독 불가로 false.
fn is_ue4_legacy_uasset(data: &[u8]) -> bool {
    if data.len() < 8 {
        return false;
    }
    let magic = u32::from_le_bytes([data[0], data[1], data[2], data[3]]);
    if magic != 0x9E2A83C1 {
        return false;
    }
    let version = i32::from_le_bytes([data[4], data[5], data[6], data[7]]);
    version > -8
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn write(path: &Path, body: &[u8]) {
        if let Some(p) = path.parent() {
            fs::create_dir_all(p).unwrap();
        }
        fs::write(path, body).unwrap();
    }

    #[test]
    fn import_folder_normalizes_and_strips_enabled() {
        let base = std::env::temp_dir().join("pmm_lib_import");
        let _ = fs::remove_dir_all(&base);
        let mods_dir = base.join("mods");

        // 가짜 원본 모드: Scripts/main.lua + enabled.txt
        let src = base.join("src/AutoHatch");
        write(&src.join("Scripts/main.lua"), b"print('hi')");
        write(&src.join("enabled.txt"), b"");

        let m = import_folder(&mods_dir, &src).unwrap();
        assert_eq!(m.id, "AutoHatch");
        let dest = mods_dir.join("AutoHatch");
        assert!(dest.join("Scripts/main.lua").is_file());
        assert!(dest.join("manifest.json").is_file());
        assert!(
            !dest.join("enabled.txt").exists(),
            "enabled.txt가 남으면 안 됨"
        );

        // 목록에 등장
        let listed = list(&mods_dir).unwrap();
        assert_eq!(listed.len(), 1);
        assert_eq!(listed[0].id, "AutoHatch");

        // 제거
        remove(&mods_dir, "AutoHatch").unwrap();
        assert!(!dest.exists());
        assert!(list(&mods_dir).unwrap().is_empty());

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn reimport_overwrites_same_id() {
        let base = std::env::temp_dir().join("pmm_lib_reimport");
        let _ = fs::remove_dir_all(&base);
        let mods_dir = base.join("mods");

        let src = base.join("src/Mod");
        write(&src.join("Scripts/main.lua"), b"v1");
        import_folder(&mods_dir, &src).unwrap();

        write(&src.join("Scripts/main.lua"), b"v2");
        import_folder(&mods_dir, &src).unwrap();

        let got = fs::read_to_string(mods_dir.join("Mod/Scripts/main.lua")).unwrap();
        assert_eq!(got, "v2");
        assert_eq!(list(&mods_dir).unwrap().len(), 1);

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn import_zip_with_top_folder() {
        let base = std::env::temp_dir().join("pmm_lib_zip");
        let _ = fs::remove_dir_all(&base);
        let mods_dir = base.join("mods");
        fs::create_dir_all(&base).unwrap();

        // MyMod/Scripts/main.lua 를 담은 zip 생성
        let zip_path = base.join("MyMod.zip");
        let f = fs::File::create(&zip_path).unwrap();
        let mut zw = zip::ZipWriter::new(f);
        let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
        zw.start_file("MyMod/Scripts/main.lua", opts).unwrap();
        zw.write_all(b"print('zip')").unwrap();
        zw.finish().unwrap();

        let m = import_zip(&mods_dir, &zip_path).unwrap();
        assert_eq!(m.id, "MyMod");
        assert!(mods_dir.join("MyMod/Scripts/main.lua").is_file());
        assert!(
            !mods_dir.join(".unzip.tmp").exists(),
            "임시 해제 폴더 정리됨"
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn import_zip_ignores_macosx_wrapper() {
        use std::io::Write as _;
        let base = std::env::temp_dir().join("pmm_lib_macosx");
        let _ = fs::remove_dir_all(&base);
        let mods_dir = base.join("mods");
        fs::create_dir_all(&base).unwrap();

        // macOS Finder "Compress" zip: MyMod/ + __MACOSX/MyMod/ 두 개의 최상위 항목
        let zip_path = base.join("MyMod.zip");
        let f = fs::File::create(&zip_path).unwrap();
        let mut zw = zip::ZipWriter::new(f);
        let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
        zw.start_file("MyMod/Scripts/main.lua", opts).unwrap();
        zw.write_all(b"print('lua')").unwrap();
        zw.start_file("__MACOSX/MyMod/._main.lua", opts).unwrap();
        zw.write_all(b"junk").unwrap();
        zw.finish().unwrap();

        let m = import_zip(&mods_dir, &zip_path).unwrap();
        assert_eq!(m.id, "MyMod", "id는 실제 모드 폴더명이어야 함");
        assert!(
            mods_dir.join("MyMod/Scripts/main.lua").is_file(),
            "main.lua가 올바른 위치에 있어야 함"
        );
        assert!(
            !mods_dir.join("MyMod/__MACOSX").exists(),
            "__MACOSX가 라이브러리에 복사되면 안 됨"
        );
        assert!(
            !mods_dir.join("MyMod/MyMod").exists(),
            "한 단계 더 중첩되면 안 됨"
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn import_zip_flat_no_wrapper() {
        let base = std::env::temp_dir().join("pmm_lib_flatzip");
        let _ = fs::remove_dir_all(&base);
        let mods_dir = base.join("mods");
        fs::create_dir_all(&base).unwrap();

        // 최상위 폴더가 없는 평평한 zip: 여러 루트 항목
        // Scripts/main.lua + config.txt 를 루트에 직접
        let zip_path = base.join("FlatMod.zip");
        let f = fs::File::create(&zip_path).unwrap();
        let mut zw = zip::ZipWriter::new(f);
        let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
        zw.start_file("Scripts/main.lua", opts).unwrap();
        zw.write_all(b"print('flat')").unwrap();
        zw.start_file("config.txt", opts).unwrap();
        zw.write_all(b"flat").unwrap();
        zw.finish().unwrap();

        let m = import_zip(&mods_dir, &zip_path).unwrap();
        assert_eq!(m.id, "FlatMod", "평평한 zip의 id는 파일명 stem에서 와야 함");
        assert!(mods_dir.join("FlatMod/Scripts/main.lua").is_file());
        assert!(mods_dir.join("FlatMod/config.txt").is_file());
        assert!(
            !mods_dir.join(".unzip.tmp").exists(),
            "임시 해제 폴더 정리됨"
        );

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn replace_from_zip_forces_existing_id() {
        let base = std::env::temp_dir().join("pmm_lib_replace");
        let _ = fs::remove_dir_all(&base);
        let mods_dir = base.join("mods");
        fs::create_dir_all(&base).unwrap();

        // 기존 라이브러리 모드 "AutoHatch" v1
        let src = base.join("src/AutoHatch");
        write(&src.join("Scripts/main.lua"), b"v1");
        import_folder(&mods_dir, &src).unwrap();

        // 다운로드 zip: 폴더명이 다름("AutoHatch-1.3")
        let zip_path = base.join("update.zip");
        let f = fs::File::create(&zip_path).unwrap();
        let mut zw = zip::ZipWriter::new(f);
        let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
        zw.start_file("AutoHatch-1.3/Scripts/main.lua", opts)
            .unwrap();
        zw.write_all(b"v2").unwrap();
        zw.finish().unwrap();

        let m = replace_from_zip(&mods_dir, "AutoHatch", &zip_path).unwrap();
        assert_eq!(m.id, "AutoHatch", "id는 기존 id로 강제");
        // 새 모드("AutoHatch-1.3")가 생기지 않음
        assert!(!mods_dir.join("AutoHatch-1.3").exists());
        // 내용이 v2로 교체
        assert_eq!(
            fs::read_to_string(mods_dir.join("AutoHatch/Scripts/main.lua")).unwrap(),
            "v2"
        );
        // 목록은 여전히 1개
        assert_eq!(list(&mods_dir).unwrap().len(), 1);

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn import_pak_file_wraps_bare_pak_into_mod() {
        let base = std::env::temp_dir().join("pmm_import_pak_file");
        let _ = fs::remove_dir_all(&base);
        let mods = base.join("mods");
        let src = base.join("src");
        fs::create_dir_all(&src).unwrap();
        // 가짜 단일 pak(내용 무관 — 변환은 import_mod에서, 여기선 래핑/판별만)
        let pak = src.join("CoolMod.pak");
        fs::write(&pak, b"PAKDATA").unwrap();

        let m = import_pak_file(&mods, &pak).unwrap();
        assert_eq!(m.mod_type, crate::manifest::ModType::Pak, "pak으로 판별돼야");
        // 라이브러리에 <id>/ 아래로 pak이 들어가고 manifest.json 생성
        let dir = mods.join(&m.id);
        assert!(dir.join("manifest.json").is_file());
        // pak 파일이 라이브러리 모드 안 어딘가에 존재(루트 배치)
        assert!(dir.join("CoolMod.pak").is_file(), "원본 pak이 라이브러리 모드에 복사돼야");
        // 임시 스테이징 잔재 없음
        assert!(!mods.join(".pakimport.tmp").exists());
        let _ = fs::remove_dir_all(&base);
    }

    // 통합: 실제 pak을 convert_pak_in_place로 3종 변환. PMM_PAK_FIXTURE 미설정이면 SKIP.
    #[test]
    fn convert_real_pak_produces_three_container() {
        let fixture = match std::env::var("PMM_PAK_FIXTURE") {
            Ok(p) => std::path::PathBuf::from(p),
            Err(_) => { eprintln!("SKIP: PMM_PAK_FIXTURE unset"); return; }
        };
        let retoc = match std::env::var("PMM_RETOC_BIN") {
            Ok(p) => std::path::PathBuf::from(p),
            Err(_) => { eprintln!("SKIP: PMM_RETOC_BIN unset"); return; }
        };
        let base = std::env::temp_dir().join("pmm_conv_real");
        let _ = fs::remove_dir_all(&base);
        let paks = base.join("Paks");
        fs::create_dir_all(&paks).unwrap();
        fs::copy(&fixture, paks.join("Mod_P.pak")).unwrap();

        let r = convert_pak_in_place(&base, &retoc, "Mod").unwrap();
        assert!(matches!(&r, crate::pak_convert::ConvertResult::Converted { .. }));
        for ext in ["pak", "ucas", "utoc"] {
            assert!(paks.join(format!("Mod.{ext}")).is_file(), "missing Mod.{ext}");
        }
        assert!(!paks.join("Mod_P.pak").exists(), "원본 단일 pak은 제거돼야");

        // JUNK fixture(ZFrancisLouis SkinMod): removed에 UE4-era 스킵 에셋 포함 확인.
        let fixture_str = std::env::var("PMM_PAK_FIXTURE").unwrap_or_default();
        if fixture_str.to_ascii_lowercase().contains("zfrancislouis") {
            if let crate::pak_convert::ConvertResult::Converted { removed, .. } = r {
                assert!(!removed.is_empty(), "JUNK: removed가 비어 있으면 안 됨; got {:?}", removed);
                let lower: Vec<_> = removed.iter().map(|s| s.to_ascii_lowercase()).collect();
                assert!(
                    lower.iter().any(|s| s.contains("francislouis") || s.contains("made_by")),
                    "JUNK: UE4-era 마커 스템이 removed에 없음; got {:?}", removed
                );
            }
        }

        let _ = fs::remove_dir_all(&base);
    }

    // C1 회귀: 원본 pak stem == mod_name 충돌 — 변환 후 .pak이 삭제되면 안 된다.
    // PMM_PAK_FIXTURE / PMM_RETOC_BIN 미설정이면 SKIP.
    #[test]
    fn convert_pak_collision_stem_equals_mod_name() {
        let fixture = match std::env::var("PMM_PAK_FIXTURE") {
            Ok(p) => std::path::PathBuf::from(p),
            Err(_) => { eprintln!("SKIP: PMM_PAK_FIXTURE unset"); return; }
        };
        let retoc = match std::env::var("PMM_RETOC_BIN") {
            Ok(p) => std::path::PathBuf::from(p),
            Err(_) => { eprintln!("SKIP: PMM_RETOC_BIN unset"); return; }
        };
        // 충돌 케이스: 원본 파일명 stem == mod_name("SomeMod") → 출력 dst도 Paks/SomeMod.pak
        let base = std::env::temp_dir().join("pmm_conv_collision");
        let _ = fs::remove_dir_all(&base);
        let paks = base.join("Paks");
        fs::create_dir_all(&paks).unwrap();
        fs::copy(&fixture, paks.join("SomeMod.pak")).unwrap();

        let r = convert_pak_in_place(&base, &retoc, "SomeMod").unwrap();
        assert!(
            matches!(&r, crate::pak_convert::ConvertResult::Converted { .. }),
            "충돌 케이스도 Converted 반환해야"
        );
        for ext in ["pak", "ucas", "utoc"] {
            assert!(
                paks.join(format!("SomeMod.{ext}")).is_file(),
                "SomeMod.{ext} 이 존재해야 함 — 특히 .pak 가 삭제되면 안 됨"
            );
        }

        let _ = fs::remove_dir_all(&base);
    }
}
