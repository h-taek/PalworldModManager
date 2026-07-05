//! UE4SS 런타임(dylib) 갱신: GitHub 릴리즈 조회 → zip 다운로드/추출 → 주입 dylib 해석.
//! 모드 업데이트(updater.rs)의 프리미티브(download/version_gt)를 재사용한다.

use std::path::{Path, PathBuf};
use tauri::Manager;

/// 업데이트 출처 레포(GitHub). 자산 이름(UE4SS_mac.zip)은 레포명과 무관하게 고정.
const REPO: &str = "h-taek/UE4SS-Palworld-macOS";
const DYLIB_FILE: &str = "libUE4SS.dylib";
const VERSION_FILE: &str = "version.txt";

/// GitHub 릴리즈에서 추출한 UE4SS 배포 정보.
pub struct ReleaseInfo {
    /// 릴리즈 태그(예: "v0.2.0"). version_gt가 선행 v를 처리한다.
    pub version: String,
    /// UE4SS_mac.zip 에셋의 직다운 URL.
    pub asset_url: String,
}

/// 매니저가 찾는 릴리즈 에셋 이름.
const ASSET_NAME: &str = "UE4SS_mac.zip";

#[derive(serde::Deserialize)]
struct GhRelease {
    tag_name: String,
    #[serde(default)]
    assets: Vec<GhAsset>,
}

#[derive(serde::Deserialize)]
struct GhAsset {
    name: String,
    browser_download_url: String,
}

/// releases/latest JSON에서 태그 + UE4SS_mac.zip 에셋 URL을 뽑는다.
/// 해당 에셋이 없으면 에러.
pub fn parse_release(json: &str) -> Result<ReleaseInfo, String> {
    let rel: GhRelease =
        serde_json::from_str(json).map_err(|e| format!("Failed to parse release JSON: {e}"))?;
    let asset = rel
        .assets
        .into_iter()
        .find(|a| a.name == ASSET_NAME)
        .ok_or_else(|| format!("Release has no asset named {ASSET_NAME}"))?;
    Ok(ReleaseInfo {
        version: rel.tag_name,
        asset_url: asset.browser_download_url,
    })
}

/// UE4SS 런타임 소스 폴더 선택(dylib + loader-mods + settings 한 덩어리).
/// - dev: 무조건 번들 resources(로컬 빌드 테스트용).
/// - release: 다운로드본이 번들 이상 버전이면 다운로드 폴더, 아니면 번들 resources 폴백.
/// dylib 하나가 아니라 런타임 전체를 한 버전 단위로 함께 전환한다.
pub fn pick_runtime_source<'a>(
    is_dev: bool,
    resources_dir: &'a Path,
    bundled_version: &str,
    downloaded: Option<(&'a Path, &str)>,
) -> &'a Path {
    if is_dev {
        return resources_dir;
    }
    match downloaded {
        // 다운로드본 >= 번들(= 번들이 더 새 게 아님)일 때만 다운로드 폴더 사용.
        Some((dir, ver)) if !crate::updater::version_gt(bundled_version, ver) => dir,
        _ => resources_dir,
    }
}

/// 릴리즈 zip에서 UE4SS 런타임 전체를 매니저 레이아웃으로 추출.
/// 매핑(zip 최상위 폴더 제거 후):
///   libUE4SS.dylib              → dest/libUE4SS.dylib
///   UE4SS/UE4SS-settings.ini    → dest/UE4SS-settings.ini
///   UE4SS/Mods/BPModLoaderMod/* → dest/loader-mods/BPModLoaderMod/*
///   UE4SS/Mods/shared/*         → dest/loader-mods/shared/*
/// mods.txt(매니저 소유)·launch-palworld.command·version.txt는 제외(install이 별도 처리).
pub fn extract_runtime(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| format!("Failed to open zip: {e}"))?;
    let mut found_dylib = false;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        if !entry.is_file() {
            continue;
        }
        let name = entry.name().to_string();
        // 최상위 폴더(UE4SS_mac/) 제거한 나머지 경로.
        let rest = match name.split_once('/') {
            Some((_, r)) => r,
            None => name.as_str(),
        };
        let rel: Option<PathBuf> = if rest == "libUE4SS.dylib" {
            Some(PathBuf::from("libUE4SS.dylib"))
        } else if rest == "UE4SS/UE4SS-settings.ini" {
            Some(PathBuf::from("UE4SS-settings.ini"))
        } else if let Some(sub) = rest.strip_prefix("UE4SS/Mods/BPModLoaderMod/") {
            Some(Path::new("loader-mods/BPModLoaderMod").join(sub))
        } else if let Some(sub) = rest.strip_prefix("UE4SS/Mods/shared/") {
            Some(Path::new("loader-mods/shared").join(sub))
        } else {
            None // 그 외(mods.txt·launcher·version.txt 등)는 제외.
        };
        let Some(rel) = rel else { continue };
        if rel == Path::new("libUE4SS.dylib") {
            found_dylib = true;
        }
        let out_path = dest.join(&rel);
        if let Some(parent) = out_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let mut out = std::fs::File::create(&out_path).map_err(|e| e.to_string())?;
        std::io::copy(&mut entry, &mut out).map_err(|e| e.to_string())?;
    }
    if !found_dylib {
        return Err("Release zip has no libUE4SS.dylib".into());
    }
    Ok(())
}

/// 프런트로 보내는 UE4SS 업데이트 상태.
#[derive(serde::Serialize)]
pub struct Ue4ssStatus {
    /// 현재 주입에 쓰이는 UE4SS 버전.
    pub current: String,
    /// 최신 릴리즈 태그(조회 성공 시).
    pub latest: Option<String>,
    pub update_available: bool,
    /// 다운로드할 zip 에셋 URL(업데이트 있을 때).
    pub asset_url: Option<String>,
    /// 비치명적 조회 실패 메시지(오프라인 등).
    pub error: Option<String>,
}

/// 번들 resources 폴더(dylib·loader-mods·settings·버전 파일 보관).
fn resources_dir(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .resolve("resources", tauri::path::BaseDirectory::Resource)
        .map_err(|e| e.to_string())
}

/// 번들 버전(resources/ue4ss-version.txt). 없으면 "0.0.0".
fn bundled_version(app: &tauri::AppHandle) -> String {
    resources_dir(app)
        .ok()
        .and_then(|d| std::fs::read_to_string(d.join("ue4ss-version.txt")).ok())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "0.0.0".to_string())
}

/// app-data에 설치된 다운로드본 런타임 폴더 + 버전(dylib·버전 둘 다 있을 때만).
fn downloaded_runtime(home: &Path) -> Option<(PathBuf, String)> {
    let dir = crate::paths::ue4ss_runtime_dir(home);
    let ver = std::fs::read_to_string(dir.join(VERSION_FILE)).ok()?;
    let ver = ver.trim().to_string();
    if dir.join(DYLIB_FILE).is_file() && !ver.is_empty() {
        Some((dir, ver))
    } else {
        None
    }
}

/// 주입/프로비저닝에 실제 쓰일 UE4SS 런타임 소스 폴더.
/// dev=번들 resources, release=다운로드본(≥번들) 우선. dylib·loader-mods·settings를
/// 여기 한 폴더에서 함께 읽어 버전 스큐(엔진만 갱신되고 Lua가 고정되는 문제)를 없앤다.
pub fn runtime_source_dir(app: &tauri::AppHandle, home: &Path) -> Result<PathBuf, String> {
    let resources = resources_dir(app)?;
    let is_dev = cfg!(debug_assertions);
    if is_dev {
        return Ok(resources);
    }
    let bver = bundled_version(app);
    let dl = downloaded_runtime(home);
    let chosen = pick_runtime_source(
        is_dev,
        &resources,
        &bver,
        dl.as_ref().map(|(p, v)| (p.as_path(), v.as_str())),
    );
    Ok(chosen.to_path_buf())
}

/// 현재 주입에 실제 쓰일 UE4SS 버전(release에서 다운로드본이 번들 이상이면 그것).
pub fn current_version(app: &tauri::AppHandle, home: &Path) -> String {
    let bundled_ver = bundled_version(app);
    match downloaded_runtime(home) {
        Some((_, dl)) if !crate::updater::version_gt(&bundled_ver, &dl) => dl,
        _ => bundled_ver,
    }
}

/// 주입할 dylib 경로 해석 = 런타임 소스 폴더의 libUE4SS.dylib.
pub fn resolve_dylib(app: &tauri::AppHandle, home: &Path) -> Result<PathBuf, String> {
    Ok(runtime_source_dir(app, home)?.join(DYLIB_FILE))
}

/// GitHub releases/latest 조회 → ReleaseInfo. (네트워크 — 수동 스모크)
fn fetch_latest() -> Result<ReleaseInfo, String> {
    let url = format!("https://api.github.com/repos/{REPO}/releases/latest");
    let json = ureq::get(&url)
        .set("User-Agent", "PalworldModManager")
        .set("Accept", "application/vnd.github+json")
        .call()
        .map_err(|e| format!("Request failed: {e}"))?
        .into_string()
        .map_err(|e| format!("Failed to read response body: {e}"))?;
    parse_release(&json)
}

/// 시작 시/Settings에서 호출: 메타만 조회(다운로드 안 함). 실패는 error 필드로.
pub fn status(app: &tauri::AppHandle, home: &Path) -> Ue4ssStatus {
    let current = current_version(app, home);
    let mut st = Ue4ssStatus {
        current: current.clone(),
        latest: None,
        update_available: false,
        asset_url: None,
        error: None,
    };
    match fetch_latest() {
        Ok(rel) => {
            st.update_available = crate::updater::version_gt(&rel.version, &current);
            st.latest = Some(rel.version);
            st.asset_url = Some(rel.asset_url);
        }
        Err(e) => st.error = Some(e),
    }
    st
}

/// Settings 버튼: 릴리즈 zip 다운로드 → 런타임 전체(dylib+loader-mods+settings) 추출 →
/// 임시 폴더에서 서명·버전기록 후 런타임 폴더를 통째로 원자 교체(temp→rename).
/// 설치된 버전 문자열 반환. (네트워크 — 수동 스모크)
pub fn install(home: &Path) -> Result<String, String> {
    let rel = fetch_latest()?;
    let dir = crate::paths::ue4ss_runtime_dir(home);
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }

    // 임시 폴더에 전체 추출 → 완성 후에만 실폴더로 교체(부분 상태 노출 방지).
    let tmp_dir = dir.with_extension("update.tmp");
    let _ = std::fs::remove_dir_all(&tmp_dir);
    let tmp_zip = crate::paths::state_dir(home).join("ue4ss-update.zip");
    crate::updater::download(&rel.asset_url, &tmp_zip)?;
    let extracted = extract_runtime(&tmp_zip, &tmp_dir);
    let _ = std::fs::remove_file(&tmp_zip);
    if let Err(e) = extracted {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(e);
    }

    // 인터넷에서 받은 dylib: quarantine 제거 후 ad-hoc 재서명(주입 가능 상태로).
    let dylib_tmp = tmp_dir.join(DYLIB_FILE);
    let _ = std::process::Command::new("xattr")
        .args(["-d", "com.apple.quarantine"])
        .arg(&dylib_tmp)
        .status();
    if let Err(e) = crate::injection::ensure_adhoc_signed(&dylib_tmp) {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(e);
    }

    if let Err(e) = std::fs::write(tmp_dir.join(VERSION_FILE), &rel.version) {
        let _ = std::fs::remove_dir_all(&tmp_dir);
        return Err(e.to_string());
    }

    // 원자적 통째 교체: 기존 런타임 제거 후 임시 폴더를 실폴더로 rename.
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::rename(&tmp_dir, &dir).map_err(|e| e.to_string())?;
    Ok(rel.version)
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = r#"{
        "tag_name": "v0.2.0",
        "name": "UE4SS mac 0.2.0",
        "assets": [
            { "name": "source.txt", "browser_download_url": "https://example.com/source.txt" },
            { "name": "UE4SS_mac.zip", "browser_download_url": "https://github.com/h-taek/UE4SS-Palworld-macOS/releases/download/v0.2.0/UE4SS_mac.zip" }
        ]
    }"#;

    #[test]
    fn parse_release_extracts_tag_and_asset_url() {
        let r = parse_release(FIXTURE).expect("should parse");
        assert_eq!(r.version, "v0.2.0");
        assert_eq!(
            r.asset_url,
            "https://github.com/h-taek/UE4SS-Palworld-macOS/releases/download/v0.2.0/UE4SS_mac.zip"
        );
    }

    #[test]
    fn parse_release_errors_when_asset_missing() {
        let json = r#"{ "tag_name": "v0.2.0", "assets": [ { "name": "other.zip", "browser_download_url": "https://x/other.zip" } ] }"#;
        assert!(parse_release(json).is_err());
    }

    // 런타임 소스 폴더 선택(dylib+loader-mods+settings 한 덩어리). 폴더 단위로 고른다.
    #[test]
    fn pick_runtime_source_dev_always_resources() {
        let res = Path::new("/app/resources");
        let dl = Path::new("/data/ue4ss");
        // dev면 다운로드본이 더 새 버전이어도 번들 resources.
        let chosen = pick_runtime_source(true, res, "0.1.0", Some((dl, "0.9.0")));
        assert_eq!(chosen, res);
    }

    #[test]
    fn pick_runtime_source_release_prefers_newer_download() {
        let res = Path::new("/app/resources");
        let dl = Path::new("/data/ue4ss");
        let chosen = pick_runtime_source(false, res, "0.1.0", Some((dl, "0.2.0")));
        assert_eq!(chosen, dl);
    }

    #[test]
    fn pick_runtime_source_release_uses_download_when_equal() {
        let res = Path::new("/app/resources");
        let dl = Path::new("/data/ue4ss");
        let chosen = pick_runtime_source(false, res, "0.2.0", Some((dl, "0.2.0")));
        assert_eq!(chosen, dl);
    }

    #[test]
    fn pick_runtime_source_release_falls_back_when_download_older() {
        let res = Path::new("/app/resources");
        let dl = Path::new("/data/ue4ss");
        let chosen = pick_runtime_source(false, res, "0.3.0", Some((dl, "0.2.0")));
        assert_eq!(chosen, res);
    }

    #[test]
    fn pick_runtime_source_release_resources_when_no_download() {
        let res = Path::new("/app/resources");
        let chosen = pick_runtime_source(false, res, "0.1.0", None);
        assert_eq!(chosen, res);
    }

    #[test]
    fn extract_runtime_pulls_dylib_loader_mods_and_settings() {
        use std::fs;
        use std::io::Write as _;
        let base = std::env::temp_dir().join("pmm_ue4ss_extract_runtime");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();

        // 실제 릴리즈 zip 레이아웃(UE4SS_mac/ 최상위) 재현.
        let zip_path = base.join("UE4SS_mac.zip");
        let f = fs::File::create(&zip_path).unwrap();
        let mut zw = zip::ZipWriter::new(f);
        let o: zip::write::FileOptions<()> = zip::write::FileOptions::default();
        let put = |zw: &mut zip::ZipWriter<fs::File>, name: &str, body: &[u8]| {
            zw.start_file(name, o).unwrap();
            zw.write_all(body).unwrap();
        };
        put(&mut zw, "UE4SS_mac/libUE4SS.dylib", b"DYLIB");
        put(&mut zw, "UE4SS_mac/version.txt", b"0.2.0");
        put(&mut zw, "UE4SS_mac/launch-palworld.command", b"launcher");
        put(&mut zw, "UE4SS_mac/UE4SS/UE4SS-settings.ini", b"[General]\n");
        put(&mut zw, "UE4SS_mac/UE4SS/Mods/mods.txt", b"BPModLoaderMod:1\n");
        put(&mut zw, "UE4SS_mac/UE4SS/Mods/BPModLoaderMod/enabled.txt", b"");
        put(&mut zw, "UE4SS_mac/UE4SS/Mods/BPModLoaderMod/Scripts/main.lua", b"loader");
        put(&mut zw, "UE4SS_mac/UE4SS/Mods/shared/UEHelpers/UEHelpers.lua", b"helpers");
        zw.finish().unwrap();

        let dest = base.join("runtime");
        extract_runtime(&zip_path, &dest).expect("추출 성공");

        // 매니저 런타임 레이아웃으로 매핑됐는지
        assert_eq!(fs::read(dest.join("libUE4SS.dylib")).unwrap(), b"DYLIB");
        assert_eq!(fs::read(dest.join("UE4SS-settings.ini")).unwrap(), b"[General]\n");
        assert!(dest.join("loader-mods/BPModLoaderMod/enabled.txt").is_file());
        assert_eq!(fs::read(dest.join("loader-mods/BPModLoaderMod/Scripts/main.lua")).unwrap(), b"loader");
        assert_eq!(fs::read(dest.join("loader-mods/shared/UEHelpers/UEHelpers.lua")).unwrap(), b"helpers");
        // 매니저 소유(mods.txt)·런처는 제외
        assert!(!dest.join("loader-mods/mods.txt").exists(), "mods.txt는 매니저 소유라 제외");
        assert!(!dest.join("launch-palworld.command").exists(), "런처 스크립트 제외");
        let _ = fs::remove_dir_all(&base);
    }

    /// 라이브 스모크(네트워크+codesign, 외부 의존): 실제 프로덕션 install()을 라이브
    /// GitHub 릴리즈에 태워 다운로드→추출→dequarantine→ad-hoc 서명→version.txt까지 검증.
    /// 기본 스위트 오염 방지 위해 #[ignore] — 명시 실행:
    /// `cargo test --lib ue4ss::tests::live_install_smoke -- --ignored --nocapture`
    #[test]
    #[ignore]
    fn live_install_smoke() {
        use std::process::Command;
        let home = std::env::temp_dir().join("pmm_ue4ss_live_install");
        let _ = std::fs::remove_dir_all(&home);

        // 1) 실제 install() — fetch_latest→download→extract→xattr→codesign→version.txt
        let ver = install(&home).expect("install()이 성공해야");
        println!("install returned version = {ver}");
        assert_eq!(ver, "v0.2.1", "라이브 최신 릴리즈 태그");

        // 2) 런타임 전체(dylib+loader-mods+settings)가 폴더에 설치되고 version.txt 정합
        let dir = crate::paths::ue4ss_runtime_dir(&home);
        let dylib = dir.join(DYLIB_FILE);
        assert!(dylib.is_file(), "libUE4SS.dylib 설치됨");
        let sz = std::fs::metadata(&dylib).unwrap().len();
        println!("dylib size = {sz} bytes");
        assert!(sz > 100_000, "dylib이 실제 바이너리 크기여야: {sz}");
        assert_eq!(std::fs::read_to_string(dir.join(VERSION_FILE)).unwrap().trim(), "v0.2.1");
        // Lua 인프라·settings까지 통째로 들어왔는지(핵심: dylib만 아님)
        assert!(dir.join("loader-mods/BPModLoaderMod/Scripts/main.lua").is_file(),
            "BPModLoaderMod 함께 설치");
        assert!(dir.join("loader-mods/shared/UEHelpers/UEHelpers.lua").is_file(),
            "shared/UEHelpers 함께 설치");
        assert!(dir.join("UE4SS-settings.ini").is_file(), "settings 함께 설치");

        // 3) 다운로드본이 arm64 mach-o + ad-hoc 서명 유효(= 주입 가능 상태)
        let file_out = Command::new("file").arg(&dylib).output().unwrap();
        let file_desc = String::from_utf8_lossy(&file_out.stdout);
        println!("file: {}", file_desc.trim());
        assert!(file_desc.contains("Mach-O") && file_desc.contains("arm64"),
            "arm64 mach-o여야: {file_desc}");
        let cs = Command::new("codesign").args(["-v", "--verbose=2"]).arg(&dylib).output().unwrap();
        assert!(cs.status.success(),
            "codesign 검증 통과(주입 가능): {}", String::from_utf8_lossy(&cs.stderr));

        // 4) detection: 라이브 태그(v0.2.0) > 번들(0.1.0) → update_available 발화
        assert!(crate::updater::version_gt(&ver, "0.1.0"), "0.1.0 대비 업데이트로 감지");

        // 5) release 소스 선택: 번들 0.1.0 vs 다운로드 v0.2.0 → 다운로드 런타임 폴더 선택
        //    (= release 빌드에서 dylib+loader-mods+settings 통째로 새 버전 사용).
        let resources = Path::new("/app/resources");
        let chosen = pick_runtime_source(false, resources, "0.1.0", Some((dir.as_path(), &ver)));
        assert_eq!(chosen, dir.as_path(), "release라면 다운로드 런타임 폴더를 써야");

        let _ = std::fs::remove_dir_all(&home);
    }

    #[test]
    fn extract_runtime_errors_when_no_dylib() {
        use std::fs;
        use std::io::Write as _;
        let base = std::env::temp_dir().join("pmm_ue4ss_extract_none");
        let _ = fs::remove_dir_all(&base);
        fs::create_dir_all(&base).unwrap();
        let zip_path = base.join("nope.zip");
        let f = fs::File::create(&zip_path).unwrap();
        let mut zw = zip::ZipWriter::new(f);
        let opts: zip::write::FileOptions<()> = zip::write::FileOptions::default();
        zw.start_file("UE4SS_mac/readme.txt", opts).unwrap();
        zw.write_all(b"hi").unwrap();
        zw.finish().unwrap();
        assert!(extract_runtime(&zip_path, &base.join("out")).is_err());
        let _ = fs::remove_dir_all(&base);
    }
}
