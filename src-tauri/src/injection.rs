use std::path::Path;
use std::process::Command;

/// 검증된 런처 동작 재현: DYLD_INSERT_LIBRARIES=<dylib> 환경으로 게임 실행 준비.
/// (argv 전달 없음 — launch-ue4ss.sh:18과 동일)
pub fn build_command(game: &Path, dylib: &Path) -> Command {
    let mut cmd = Command::new(game);
    cmd.env("DYLD_INSERT_LIBRARIES", dylib);
    cmd
}

/// 주입 직전 dylib ad-hoc 재서명(launch-ue4ss.sh:14 재현).
/// 미서명/서명 불일치 dylib는 DYLD가 게임 프로세스에 올리지 않는다. `--force`라 멱등.
pub fn ensure_adhoc_signed(dylib: &Path) -> Result<(), String> {
    let out = Command::new("codesign")
        .args(["-s", "-", "--force", "--timestamp=none"])
        .arg(dylib)
        .output()
        .map_err(|e| format!("Failed to run codesign: {e}"))?;
    if !out.status.success() {
        return Err(format!(
            "codesign failed: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sets_program_and_dyld_env() {
        let game = Path::new("/Applications/Palworld.app/Contents/MacOS/Palworld");
        let dylib = Path::new("/tmp/libUE4SS.dylib");
        let cmd = build_command(game, dylib);
        assert_eq!(cmd.get_program(), game.as_os_str());
        let env: Vec<_> = cmd.get_envs().collect();
        assert!(env
            .iter()
            .any(|(k, v)| *k == std::ffi::OsStr::new("DYLD_INSERT_LIBRARIES")
                && v.map(|v| v == dylib.as_os_str()).unwrap_or(false)));
        // argv 없음
        assert_eq!(cmd.get_args().count(), 0);
    }
}
