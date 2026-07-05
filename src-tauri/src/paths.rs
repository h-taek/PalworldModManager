use std::path::{Path, PathBuf};

pub fn game_binary() -> PathBuf {
    PathBuf::from("/Applications/Palworld.app/Contents/MacOS/Palworld")
}

pub fn manual_game_binary_txt(home: &Path) -> PathBuf {
    state_dir(home).join("game-binary.txt")
}

pub fn configured_game_binary(home: &Path) -> PathBuf {
    match std::fs::read_to_string(manual_game_binary_txt(home)) {
        Ok(s) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                game_binary()
            } else {
                PathBuf::from(trimmed)
            }
        }
        Err(_) => game_binary(),
    }
}

pub fn write_manual_game_binary(home: &Path, path: &Path) -> Result<(), String> {
    std::fs::create_dir_all(state_dir(home)).map_err(|e| e.to_string())?;
    std::fs::write(
        manual_game_binary_txt(home),
        path.to_string_lossy().as_bytes(),
    )
    .map_err(|e| e.to_string())
}

pub fn container_root(home: &Path) -> PathBuf {
    home.join("Library/Containers/com.pocketpair.palworld.mac/Data")
}

pub fn container_ue4ss_dir(home: &Path) -> PathBuf {
    container_root(home).join("UE4SS")
}

/// ModConfigMenu 설정 원본 격리 폴더(쓰기가능 컨테이너). 번들 심링크의 대상.
pub fn container_modconfigs_dir(home: &Path) -> PathBuf {
    container_ue4ss_dir(home).join("ModConfigs")
}

/// 매니저 라이브러리(원본 모드 보관) 루트. M2에서 사용 — M1 미사용.
pub fn app_support_dir(home: &Path) -> PathBuf {
    home.join("Library/Application Support/PalworldModManager")
}

pub fn library_mods_dir(home: &Path) -> PathBuf {
    app_support_dir(home).join("mods")
}

pub fn library_mod_dir(home: &Path, id: &str) -> PathBuf {
    library_mods_dir(home).join(id)
}

pub fn state_dir(home: &Path) -> PathBuf {
    app_support_dir(home).join("state")
}

/// 다운로드한 UE4SS 런타임 보관(libUE4SS.dylib + version.txt). release 자동 업데이트본.
pub fn ue4ss_runtime_dir(home: &Path) -> PathBuf {
    app_support_dir(home).join("ue4ss")
}

pub fn active_json(home: &Path) -> PathBuf {
    state_dir(home).join("active.json")
}

pub fn deployed_json(home: &Path) -> PathBuf {
    state_dir(home).join("deployed.json")
}

pub fn profiles_json(home: &Path) -> PathBuf {
    state_dir(home).join("profiles.json")
}

pub fn container_log(home: &Path) -> PathBuf {
    container_ue4ss_dir(home).join("UE4SS.log")
}

/// 게임 실행 시 stdout/stderr 리다이렉트 대상. 터미널 런처(launch-ue4ss.sh)와 동일 위치라
/// 앱/터미널 어느 쪽으로 띄워도 진단 로그가 한곳에 모인다. 실행마다 truncate.
/// Caches라 OS가 비워도 안전(재생성됨).
pub fn launch_log(home: &Path) -> PathBuf {
    home.join("Library/Caches/ue4ss-mac/palworld-launch.log")
}

pub fn real_home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").expect("HOME must be set"))
}

/// game_binary(<app>/Contents/MacOS/Palworld) → 프로젝트 디렉터리(<app>/Contents/UE/<proj>).
/// <proj> = Contents/UE 하위 "Engine" 아닌 첫 디렉터리(하드코딩 회피, 로더 bundle_paths.cpp 규칙).
pub fn derive_project_dir(game_binary: &Path) -> Result<PathBuf, String> {
    let contents = game_binary
        .parent()
        .and_then(|p| p.parent())
        .ok_or("cannot derive Contents from game binary")?;
    let ue = contents.join("UE");
    let mut candidates: Vec<PathBuf> = std::fs::read_dir(&ue)
        .map_err(|e| format!("read {}: {e}", ue.display()))?
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.is_dir())
        .filter(|p| {
            p.file_name()
                .and_then(|n| n.to_str())
                .map(|n| n != "Engine")
                .unwrap_or(false)
        })
        .collect();
    candidates.sort();
    candidates
        .into_iter()
        .next()
        .ok_or_else(|| format!("no project dir under {}", ue.display()))
}

pub fn bundle_mods_paks(game_binary: &Path) -> Result<PathBuf, String> {
    Ok(derive_project_dir(game_binary)?.join("Content/Paks/~mods"))
}

pub fn bundle_logicmods(game_binary: &Path) -> Result<PathBuf, String> {
    Ok(derive_project_dir(game_binary)?.join("Content/Paks/LogicMods"))
}

pub fn bundle_lua_mods(game_binary: &Path) -> Result<PathBuf, String> {
    Ok(derive_project_dir(game_binary)?.join("Binaries/Win64/Mods"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn paths_compose_from_home() {
        let home = Path::new("/Users/test");
        assert_eq!(
            container_ue4ss_dir(home),
            PathBuf::from(
                "/Users/test/Library/Containers/com.pocketpair.palworld.mac/Data/UE4SS"
            )
        );
        assert_eq!(
            app_support_dir(home),
            PathBuf::from("/Users/test/Library/Application Support/PalworldModManager")
        );
        assert_eq!(
            game_binary(),
            PathBuf::from("/Applications/Palworld.app/Contents/MacOS/Palworld")
        );
        assert_eq!(
            manual_game_binary_txt(home),
            PathBuf::from(
                "/Users/test/Library/Application Support/PalworldModManager/state/game-binary.txt"
            )
        );
        assert_eq!(
            library_mod_dir(home, "autohatch"),
            PathBuf::from(
                "/Users/test/Library/Application Support/PalworldModManager/mods/autohatch"
            )
        );
        assert_eq!(
            active_json(home),
            PathBuf::from(
                "/Users/test/Library/Application Support/PalworldModManager/state/active.json"
            )
        );
        assert_eq!(
            deployed_json(home),
            PathBuf::from(
                "/Users/test/Library/Application Support/PalworldModManager/state/deployed.json"
            )
        );
        assert_eq!(
            profiles_json(home),
            PathBuf::from(
                "/Users/test/Library/Application Support/PalworldModManager/state/profiles.json"
            )
        );
        assert_eq!(
            container_log(home),
            PathBuf::from(
                "/Users/test/Library/Containers/com.pocketpair.palworld.mac/Data/UE4SS/UE4SS.log"
            )
        );
        assert_eq!(
            launch_log(home),
            PathBuf::from("/Users/test/Library/Caches/ue4ss-mac/palworld-launch.log")
        );
    }

    #[test]
    fn derives_bundle_dirs_from_game_binary() {
        let base = std::env::temp_dir().join("pmm_bundle_paths");
        let _ = std::fs::remove_dir_all(&base);
        // <app>/Contents/{MacOS/Palworld, UE/{Engine, Pal}}
        let contents = base.join("Palworld.app/Contents");
        std::fs::create_dir_all(contents.join("MacOS")).unwrap();
        std::fs::create_dir_all(contents.join("UE/Engine")).unwrap();
        std::fs::create_dir_all(contents.join("UE/Pal")).unwrap();
        let game = contents.join("MacOS/Palworld");
        std::fs::write(&game, b"").unwrap();

        let proj = derive_project_dir(&game).unwrap();
        assert_eq!(proj, contents.join("UE/Pal"));
        assert_eq!(bundle_mods_paks(&game).unwrap(), contents.join("UE/Pal/Content/Paks/~mods"));
        assert_eq!(bundle_logicmods(&game).unwrap(), contents.join("UE/Pal/Content/Paks/LogicMods"));
        assert_eq!(bundle_lua_mods(&game).unwrap(), contents.join("UE/Pal/Binaries/Win64/Mods"));
        let _ = std::fs::remove_dir_all(&base);
    }
}
