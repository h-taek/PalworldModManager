use crate::detection::DetectionResult;
use crate::manifest::{self, Manifest, ModType};
use crate::{
    detection, injection, library, logtail, modstore, pak_convert, paths, profiles, retoc,
    staging, ue4ss, updater,
};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::process::{Child, Command};
use std::sync::{Mutex, OnceLock};
use tauri::Emitter;

static GAME_CHILD: OnceLock<Mutex<Option<Child>>> = OnceLock::new();
const PALWORLD_BUNDLE_ID: &str = "com.pocketpair.palworld.mac";

fn game_child() -> &'static Mutex<Option<Child>> {
    GAME_CHILD.get_or_init(|| Mutex::new(None))
}

fn child_is_running(child: &mut Option<Child>, pid: u32) -> Result<bool, String> {
    let Some(ch) = child.as_mut() else {
        return Ok(false);
    };
    if ch.id() != pid {
        return Ok(false);
    }
    match ch.try_wait().map_err(|e| e.to_string())? {
        Some(_) => {
            *child = None;
            Ok(false)
        }
        None => Ok(true),
    }
}

/// 프런트로 보내는 모드 1행.
#[derive(Serialize)]
pub struct ModView {
    pub id: String,
    pub name: String,
    pub version: String,
    pub mod_type: ModType,
    pub enabled: bool,
    pub deployable: bool,
    pub status: String,
    pub has_update_url: bool,
    pub removed: Vec<String>,
}

impl ModView {
    fn build(m: Manifest, enabled: bool) -> Self {
        let status = match m.mod_type {
            ModType::Lua => String::new(),
            ModType::Pak | ModType::Hybrid => String::new(),
            ModType::Unknown => "Unknown mod".to_string(),
        };
        let deployable = m.mod_type != ModType::Unknown;
        let has_update_url = m.update_url.is_some();
        ModView {
            id: m.id,
            name: m.name,
            version: m.version,
            mod_type: m.mod_type,
            enabled,
            deployable,
            status,
            has_update_url,
            removed: Vec::new(),
        }
    }
}

/// 프로필 스토어 로드(옛 active.json을 마이그레이션 시드로 전달).
fn load_store(home: &std::path::Path) -> profiles::ProfileStore {
    let legacy = modstore::read_id_list(&paths::active_json(home));
    profiles::ProfileStore::load(&paths::profiles_json(home), &legacy)
}

#[tauri::command]
pub fn detect_game() -> DetectionResult {
    let home = paths::real_home();
    detection::detect(&home, &paths::configured_game_binary(&home))
}

fn normalize_game_binary_path(path: PathBuf) -> Result<PathBuf, String> {
    let app_binary = path.join("Contents/MacOS/Palworld");
    let nested_binary = path.join("Palworld");
    let candidate = if path.is_file() {
        path
    } else if app_binary.exists() {
        app_binary
    } else if nested_binary.exists() {
        nested_binary
    } else {
        path
    };
    if !candidate.exists() {
        return Err("Could not find the Palworld executable at the selected location.".into());
    }
    if candidate.file_name().and_then(|s| s.to_str()) != Some("Palworld") {
        return Err("Select the Palworld executable or Palworld.app.".into());
    }
    Ok(candidate)
}

#[tauri::command]
pub async fn pick_game_binary(app: tauri::AppHandle) -> Result<Option<DetectionResult>, String> {
    use tauri_plugin_dialog::DialogExt;
    match app
        .dialog()
        .file()
        .add_filter("Palworld app", &["app"])
        .blocking_pick_file()
    {
        Some(fp) => {
            let picked = fp.into_path().map_err(|e| e.to_string())?;
            let binary = normalize_game_binary_path(picked)?;
            let home = paths::real_home();
            paths::write_manual_game_binary(&home, &binary)?;
            Ok(Some(detection::detect(&home, &binary)))
        }
        None => Ok(None),
    }
}

#[tauri::command]
pub fn list_mods() -> Result<Vec<ModView>, String> {
    let home = paths::real_home();
    let store = load_store(&home);
    let active = store.active_mods().to_vec();
    let manifests = library::list(&paths::library_mods_dir(&home))?;
    Ok(manifests
        .into_iter()
        .map(|m| {
            let enabled = active.contains(&m.id);
            ModView::build(m, enabled)
        })
        .collect())
}

/// 경로 기반 가져오기(폴더/zip 자동 판별). 드래그앤드롭(Task 8)도 이 커맨드를 쓴다.
#[tauri::command]
pub fn import_mod(app: tauri::AppHandle, path: String) -> Result<ModView, String> {
    let home = paths::real_home();
    let mods_dir = paths::library_mods_dir(&home);
    let p = PathBuf::from(&path);
    let manifest = if p.is_dir() {
        library::import_folder(&mods_dir, &p)?
    } else if p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("zip"))
        .unwrap_or(false)
    {
        library::import_zip(&mods_dir, &p)?
    } else if p
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| e.eq_ignore_ascii_case("pak"))
        .unwrap_or(false)
    {
        library::import_pak_file(&mods_dir, &p)?
    } else {
        return Err("Only folders, .zip, or .pak files can be imported.".into());
    };
    let enabled = manifest.mod_type == ModType::Lua;
    // pak 변환: 라이브러리에 들어온 모드가 단일 pak이면 3종으로 변환(import 시 1회).
    let lib_mod = paths::library_mod_dir(&home, &manifest.id);
    let mut conv_removed: Vec<String> = Vec::new();
    if manifest.mod_type == ModType::Pak && manifest::pak_needs_conversion(&lib_mod) {
        let bin = retoc::retoc_bin(&app)?;
        match library::convert_pak_in_place(&lib_mod, &bin, &manifest.id)? {
            pak_convert::ConvertResult::Converted { removed, .. } => { conv_removed = removed; }
            pak_convert::ConvertResult::NeedsUserDecision { removed, stderr } => {
                let _ = library::remove(&mods_dir, &manifest.id); // 부분 상태 금지; remove 실패가 원래 에러를 가리면 안 됨
                return Err(format!(
                    "PAK_CONVERT_NEEDS_DECISION:{}",
                    serde_json::to_string(&serde_json::json!({
                        "removed": removed,
                        "error": stderr.trim()
                    }))
                    .unwrap_or_default()
                ));
            }
        }
    }
    if enabled {
        let mut store = load_store(&home);
        store.set_enabled_in_active(&manifest.id, true);
        store.save(&paths::profiles_json(&home))?;
        reconcile_now(&app, &home, store.active_mods())?;
    }
    let mut view = ModView::build(manifest, enabled);
    view.removed = conv_removed;
    Ok(view)
}

/// macOS NSOpenPanel을 직접 띄워 파일(zip/pak) 또는 폴더를 하나 선택받는다.
/// tauri 다이얼로그 플러그인(rfd)은 파일/폴더를 한 창에서 동시에 못 고르므로 네이티브 직접 호출.
/// 패널은 메인스레드에서만 뜰 수 있어 run_on_main_thread + 채널로 경로만 받아오고,
/// 실제 import(파일 IO)는 이 워커에서 수행한다.
#[tauri::command]
pub async fn pick_mod_path(app: tauri::AppHandle) -> Result<Option<ModView>, String> {
    let app2 = app.clone();
    let picked: Option<String> =
        tauri::async_runtime::spawn_blocking(move || pick_path_native(&app2))
            .await
            .map_err(|e| e.to_string())??;
    match picked {
        Some(path) => Ok(Some(import_mod(app, path)?)),
        None => Ok(None),
    }
}

/// 메인스레드에서 NSOpenPanel 실행 → 선택 경로(Option<String>) 반환. macOS 전용.
#[cfg(target_os = "macos")]
fn pick_path_native(app: &tauri::AppHandle) -> Result<Option<String>, String> {
    use std::sync::mpsc;
    let (tx, rx) = mpsc::channel::<Option<String>>();
    app.run_on_main_thread(move || {
        // 반드시 메인스레드 — MainThreadMarker 확보
        let mtm = objc2_foundation::MainThreadMarker::new()
            .expect("run_on_main_thread guarantees main thread");
        let result = {
            use objc2_app_kit::{NSModalResponseOK, NSOpenPanel};
            let panel = NSOpenPanel::openPanel(mtm);
            panel.setCanChooseFiles(true);
            panel.setCanChooseDirectories(true);
            panel.setAllowsMultipleSelection(false);
            panel.setResolvesAliases(true);
            // 파일 필터: zip/pak만 (폴더는 필터와 무관하게 선택 가능).
            {
                use objc2_foundation::{NSArray, NSString};
                let types = NSArray::from_retained_slice(&[
                    NSString::from_str("zip"),
                    NSString::from_str("pak"),
                ]);
                #[allow(deprecated)]
                panel.setAllowedFileTypes(Some(&types));
            }
            let resp = panel.runModal();
            if resp == NSModalResponseOK {
                let urls = panel.URLs();
                urls.firstObject()
                    .and_then(|url| url.path())
                    .map(|p| p.to_string())
            } else {
                None
            }
        };
        let _ = tx.send(result);
    })
    .map_err(|e| e.to_string())?;
    rx.recv().map_err(|e| e.to_string())
}

/// 비-macOS 스텁(컴파일 안전용, 실사용 없음).
#[cfg(not(target_os = "macos"))]
fn pick_path_native(_app: &tauri::AppHandle) -> Result<Option<String>, String> {
    Err("Native file picker is only available on macOS.".into())
}

#[tauri::command]
pub fn set_mod_enabled(app: tauri::AppHandle, id: String, enabled: bool) -> Result<(), String> {
    let home = paths::real_home();
    let mut store = load_store(&home);
    store.set_enabled_in_active(&id, enabled);
    store.save(&paths::profiles_json(&home))?;
    reconcile_now(&app, &home, store.active_mods())?;
    Ok(())
}

#[tauri::command]
pub fn remove_mod(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let home = paths::real_home();
    let mut store = load_store(&home);
    store.remove_from_all(&id);
    store.save(&paths::profiles_json(&home))?;
    reconcile_now(&app, &home, store.active_mods())?; // 컨테이너에서 먼저 회수
    library::remove(&paths::library_mods_dir(&home), &id)?;
    Ok(())
}

#[tauri::command]
pub async fn launch_game(app: tauri::AppHandle) -> Result<u32, String> {
    {
        let mut slot = game_child().lock().map_err(|e| e.to_string())?;
        let existing_pid = slot.as_ref().map(|child| child.id());
        if let Some(pid) = existing_pid {
            if child_is_running(&mut slot, pid)? {
                return Ok(pid);
            }
        }
    }

    // 스테이징(권한·복사)은 시간이 걸리므로 블로킹 스레드에서 수행하고 완료를 await.
    // 주입은 스테이징이 끝난 뒤에만 시작한다.
    let app_bg = app.clone();
    let _ = app.emit("mod-staging", "start");
    tauri::async_runtime::spawn_blocking(move || {
        let home = paths::real_home();
        let active = load_store(&home).active_mods().to_vec();
        reconcile_now(&app_bg, &home, &active)
    })
    .await
    .map_err(|e| e.to_string())??;
    let _ = app.emit("mod-staging", "done");

    let home = paths::real_home();
    // dev=번들 dylib / release=다운로드본(있고 번들 이상이면) 우선.
    let dylib = ue4ss::resolve_dylib(&app, &home)?;
    if !dylib.exists() {
        return Err(format!("loader not found at {}", dylib.display()));
    }
    injection::ensure_adhoc_signed(&dylib)?;
    let game_binary = paths::configured_game_binary(&home);
    let child = injection::build_command(&game_binary, &dylib)
        .spawn()
        .map_err(|e| format!("spawn failed: {e}"))?;
    let pid = child.id();
    let mut slot = game_child().lock().map_err(|e| e.to_string())?;
    *slot = Some(child);
    Ok(pid)
}

fn signal_process(pid: u32, signal: &str) -> Result<(), String> {
    let status = Command::new("kill")
        .arg(signal)
        .arg(pid.to_string())
        .status()
        .map_err(|e| format!("Failed to run kill: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("kill {signal} failed for pid {pid}"))
    }
}

fn request_game_quit() -> Result<(), String> {
    let status = Command::new("osascript")
        .arg("-e")
        .arg(format!(
            "tell application id \"{PALWORLD_BUNDLE_ID}\" to quit"
        ))
        .status()
        .map_err(|e| format!("Failed to request app quit: {e}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("quit request failed for {PALWORLD_BUNDLE_ID}"))
    }
}

#[tauri::command]
pub fn is_game_process_running(pid: u32) -> bool {
    let Ok(mut slot) = game_child().lock() else {
        return false;
    };
    child_is_running(&mut slot, pid).unwrap_or(false)
}

#[tauri::command]
pub fn stop_game(pid: u32) -> Result<(), String> {
    {
        let mut slot = game_child().lock().map_err(|e| e.to_string())?;
        if !child_is_running(&mut slot, pid)? {
            return Ok(());
        }
    }
    request_game_quit().or_else(|_| signal_process(pid, "-TERM"))
}

#[tauri::command]
pub fn force_stop_game(pid: u32) -> Result<(), String> {
    let mut slot = game_child().lock().map_err(|e| e.to_string())?;
    let Some(ch) = slot.as_mut() else {
        return Ok(());
    };
    if ch.id() != pid {
        return Ok(());
    }
    if !child_is_running(&mut slot, pid)? {
        return Ok(());
    }
    if let Some(ch) = slot.as_mut() {
        ch.kill()
            .map_err(|e| format!("Failed to force stop process: {e}"))?;
        let _ = ch.wait();
    }
    *slot = None;
    Ok(())
}

/// UE4SS 업데이트 상태 조회(시작 시/Settings). 메타만 — 다운로드 안 함.
#[tauri::command]
pub async fn ue4ss_status(app: tauri::AppHandle) -> Result<ue4ss::Ue4ssStatus, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let home = paths::real_home();
        ue4ss::status(&app, &home)
    })
    .await
    .map_err(|e| e.to_string())
}

/// UE4SS 최신 릴리즈 zip 다운로드·설치(Settings 버튼). 설치된 버전 반환.
#[tauri::command]
pub async fn ue4ss_install_update() -> Result<String, String> {
    tauri::async_runtime::spawn_blocking(move || {
        let home = paths::real_home();
        ue4ss::install(&home)
    })
    .await
    .map_err(|e| e.to_string())?
}

/// 런타임 보장(컨테이너) + 번들 3버킷 reconcile + deployed.json 갱신.
/// 인프라(BPModLoaderMod·shared)는 번들 Lua 루트에 프로비저닝. settings.ini만 컨테이너.
fn reconcile_now(app: &tauri::AppHandle, home: &Path, active: &[String]) -> Result<(), String> {
    // UE4SS 런타임 소스 폴더(dylib·loader-mods·settings 한 덩어리). release=다운로드본(≥번들)
    // 우선, dev=번들 resources. dylib 주입(resolve_dylib)과 동일 소스라 버전 정합 보장.
    let runtime_src = ue4ss::runtime_source_dir(app, home)?;

    // 1) 컨테이너 런타임 데이터(settings.ini)만 보장.
    let ue4ss_dir = paths::container_ue4ss_dir(home);
    let settings_src = runtime_src.join("UE4SS-settings.ini");
    staging::ensure_runtime(&ue4ss_dir, &settings_src).map_err(|e| e.to_string())?;

    // 2) 번들 3폴더 경로 유도 + 권한 보장(필요 시 관리자 1회).
    let game_binary = paths::configured_game_binary(home);
    let mods_dir = paths::bundle_mods_paks(&game_binary)?;
    let logicmods_dir = paths::bundle_logicmods(&game_binary)?;
    let lua_dir = paths::bundle_lua_mods(&game_binary)?;
    staging::ensure_bundle_writable(&mods_dir, &logicmods_dir, &lua_dir)?;

    // 3) 인프라(BPModLoaderMod·shared) → 번들 Lua 루트.
    let loader_src = runtime_src.join("loader-mods");
    staging::provision_loader_mods(&lua_dir, &loader_src).map_err(|e| e.to_string())?;

    // 4) 분류 배포. mods.txt 이름은 실제 배치된 Lua 폴더명에서 유도(reconcile 내부).
    let library_mods = paths::library_mods_dir(home);
    let modconfigs_dir = paths::container_modconfigs_dir(home);
    let prev = modstore::read_deployed(&paths::deployed_json(home));
    let new_deployed = modstore::reconcile_bundle(
        &library_mods,
        &mods_dir,
        &logicmods_dir,
        &lua_dir,
        &modconfigs_dir,
        active,
        &prev,
    )?;
    modstore::write_deployed(&paths::deployed_json(home), &new_deployed)?;
    Ok(())
}

/// 프런트로 보내는 프로필 1행.
#[derive(Serialize)]
pub struct ProfileView {
    pub id: String,
    pub name: String,
    pub mod_count: usize,
    pub active: bool,
}

#[tauri::command]
pub fn list_profiles() -> Result<Vec<ProfileView>, String> {
    let home = paths::real_home();
    let store = load_store(&home);
    Ok(store
        .profiles
        .iter()
        .map(|p| ProfileView {
            id: p.id.clone(),
            name: p.name.clone(),
            mod_count: p.mods.len(),
            active: p.id == store.active,
        })
        .collect())
}

#[tauri::command]
pub fn create_profile(name: String) -> Result<ProfileView, String> {
    let home = paths::real_home();
    let mut store = load_store(&home);
    let p = store.create(&name)?;
    store.save(&paths::profiles_json(&home))?;
    Ok(ProfileView {
        id: p.id,
        name: p.name,
        mod_count: p.mods.len(),
        active: false,
    })
}

#[tauri::command]
pub fn duplicate_profile(src_id: String, name: String) -> Result<ProfileView, String> {
    let home = paths::real_home();
    let mut store = load_store(&home);
    let p = store.duplicate(&src_id, &name)?;
    store.save(&paths::profiles_json(&home))?;
    Ok(ProfileView {
        id: p.id,
        name: p.name,
        mod_count: p.mods.len(),
        active: false,
    })
}

#[tauri::command]
pub fn switch_profile(app: tauri::AppHandle, id: String) -> Result<(), String> {
    let home = paths::real_home();
    let mut store = load_store(&home);
    store.set_active(&id)?;
    // 활성 mods를 라이브러리 실재 id로 가지치기(불변식)
    let known: Vec<String> = library::list(&paths::library_mods_dir(&home))?
        .into_iter()
        .map(|m| m.id)
        .collect();
    let active = store.active.clone();
    if let Some(p) = store.profiles.iter_mut().find(|p| p.id == active) {
        p.mods = profiles::prune_missing(&p.mods, &known);
    }
    store.save(&paths::profiles_json(&home))?;
    reconcile_now(&app, &home, store.active_mods())?;
    Ok(())
}

#[tauri::command]
pub fn rename_profile(id: String, name: String) -> Result<(), String> {
    let home = paths::real_home();
    let mut store = load_store(&home);
    store.rename(&id, &name)?;
    store.save(&paths::profiles_json(&home))
}

#[tauri::command]
pub fn delete_profile(id: String) -> Result<(), String> {
    let home = paths::real_home();
    let mut store = load_store(&home);
    store.delete(&id)?;
    store.save(&paths::profiles_json(&home))
}

/// 프런트로 보내는 모드별 업데이트 상태.
#[derive(Serialize)]
pub struct UpdateStatus {
    pub id: String,
    pub name: String,
    pub current: String,
    pub latest: Option<String>,
    pub has_update: bool,
    pub url: Option<String>,
    pub error: Option<String>,
}

/// updateURL 있는 모든 모드의 원격 버전을 확인(네트워크 — 워커 스레드). 모드별 오류는
/// 전체를 실패시키지 않고 error 필드에 담는다.
#[tauri::command]
pub async fn check_updates() -> Result<Vec<UpdateStatus>, String> {
    tauri::async_runtime::spawn_blocking(check_updates_blocking)
        .await
        .map_err(|e| e.to_string())?
}

fn check_updates_blocking() -> Result<Vec<UpdateStatus>, String> {
    let home = paths::real_home();
    let mods = library::list(&paths::library_mods_dir(&home))?;
    let mut out = Vec::new();
    for m in mods {
        let url = match m.update_url {
            Some(u) => u,
            None => continue, // updateURL 없는 모드는 확인 대상 아님
        };
        let mut st = UpdateStatus {
            id: m.id.clone(),
            name: m.name.clone(),
            current: m.version.clone(),
            latest: None,
            has_update: false,
            url: None,
            error: None,
        };
        match updater::fetch_text(&url).and_then(|j| updater::parse_remote(&j)) {
            Ok(remote) => {
                st.has_update = updater::version_gt(&remote.version, &m.version);
                st.latest = Some(remote.version);
                st.url = remote.url;
            }
            Err(e) => st.error = Some(e),
        }
        out.push(st);
    }
    Ok(out)
}

/// FIX 1 helper: 다운로드된 zip에서 얻은 manifest에 remote version과 이전 updateURL을 전파.
/// 순수 함수 — 단위 테스트 가능.
fn merge_updated(prev: &Manifest, mut replaced: Manifest, remote_version: &str) -> Manifest {
    replaced.version = remote_version.to_string();
    if replaced.update_url.is_none() {
        replaced.update_url = prev.update_url.clone();
    }
    replaced
}

/// 모드 1개 업데이트: 원격 매니페스트의 url로 zip 다운로드 → 라이브러리 기존 id로 원자적 교체
/// → 활성 상태면 reconcile. lua 전용(pak v1 게이트).
#[tauri::command]
pub async fn update_mod(app: tauri::AppHandle, id: String) -> Result<ModView, String> {
    tauri::async_runtime::spawn_blocking(move || update_mod_blocking(&app, &id))
        .await
        .map_err(|e| e.to_string())?
}

fn update_mod_blocking(app: &tauri::AppHandle, id: &str) -> Result<ModView, String> {
    let id = manifest::sanitize_id(id); // FIX 4: sanitize frontend id at filesystem boundary
    let home = paths::real_home();
    let mods_dir = paths::library_mods_dir(&home);
    let current = manifest::load_or_synthesize(&mods_dir.join(&id), &id)?;
    let url = current
        .update_url
        .clone()
        .ok_or_else(|| "This mod does not have an updateURL.".to_string())?;
    let remote = updater::parse_remote(&updater::fetch_text(&url)?)?;
    let dl = remote
        .url
        .ok_or_else(|| "The remote manifest does not include a download URL.".to_string())?;

    let tmp_zip = paths::state_dir(&home).join(format!("update-{id}.zip"));
    updater::download(&dl, &tmp_zip)?;
    let replaced = library::replace_from_zip(&mods_dir, &id, &tmp_zip);
    let _ = std::fs::remove_file(&tmp_zip);
    let replaced = replaced?;

    // FIX 1: remote version + 이전 updateURL 전파 후 manifest.json 갱신
    let updated = merge_updated(&current, replaced, &remote.version);
    let manifest_path = mods_dir.join(&updated.id).join("manifest.json");
    std::fs::write(
        &manifest_path,
        serde_json::to_string_pretty(&updated).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;

    // pak 변환: 업데이트된 모드가 단일 pak이면 재변환(import와 동일 패턴).
    let lib_mod = paths::library_mod_dir(&home, &updated.id);
    let mut conv_removed: Vec<String> = Vec::new();
    if updated.mod_type == ModType::Pak && manifest::pak_needs_conversion(&lib_mod) {
        let bin = retoc::retoc_bin(app)?;
        match library::convert_pak_in_place(&lib_mod, &bin, &updated.id)? {
            pak_convert::ConvertResult::Converted { removed, .. } => { conv_removed = removed; }
            pak_convert::ConvertResult::NeedsUserDecision { removed, stderr } => {
                let _ = library::remove(&mods_dir, &updated.id); // 부분 상태 금지; remove 실패가 원래 에러를 가리면 안 됨
                return Err(format!(
                    "PAK_CONVERT_NEEDS_DECISION:{}",
                    serde_json::to_string(&serde_json::json!({
                        "removed": removed,
                        "error": stderr.trim()
                    }))
                    .unwrap_or_default()
                ));
            }
        }
    }

    // FIX 3: enabled 한 번만 계산해 재배포 조건 + ModView::build 양쪽에 재사용
    let active = load_store(&home).active_mods().to_vec();
    let enabled = active.contains(&updated.id);
    if enabled {
        reconcile_now(app, &home, &active)?;
    }
    let mut view = ModView::build(updated, enabled);
    view.removed = conv_removed;
    Ok(view)
}

#[cfg(test)]
mod tests {
    use super::{merge_updated, ModView};
    use crate::manifest::{Manifest, ModType};

    #[test]
    fn modview_pak_status_not_phase0() {
        let m = Manifest {
            id: "b".into(),
            name: "B".into(),
            version: "1".into(),
            mod_type: ModType::Pak,
            update_url: None,
            entry: None,
        };
        let v = ModView::build(m, true);
        assert!(!v.status.contains("Phase 0"));
    }

    #[test]
    fn merge_updated_carries_forward_update_url_and_sets_version() {
        let prev = Manifest {
            id: "mod1".into(),
            name: "Mod 1".into(),
            version: "1.0.0".into(),
            mod_type: ModType::Lua,
            update_url: Some("https://example.com/mod1.json".into()),
            entry: None,
        };
        // zip에서 얻은 manifest: update_url 없음, version은 기본값
        let replaced = Manifest {
            id: "mod1".into(),
            name: "Mod 1".into(),
            version: "0.0.0".into(),
            mod_type: ModType::Lua,
            update_url: None,
            entry: None,
        };
        let result = merge_updated(&prev, replaced, "2.0.0");
        assert_eq!(result.version, "2.0.0", "remote version이 반영돼야 함");
        assert_eq!(
            result.update_url.as_deref(),
            Some("https://example.com/mod1.json"),
            "이전 updateURL이 전파돼야 함"
        );
    }
}

/// 컨테이너 UE4SS.log의 마지막 64KB를 반환(없으면 안내문).
#[tauri::command]
pub fn read_log() -> Result<String, String> {
    let home = paths::real_home();
    logtail::read_tail(&paths::container_log(&home), 64 * 1024)
}
