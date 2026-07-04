//! 번들 retoc 서브프로세스 래퍼. 단일 pak(또는 디렉터리) → IoStore 3종 변환.
//! 계약 = docs/spec/04 §7(Task 1 확정): to-zen --version UE5_1 <INPUT> <OUTPUT.utoc>.

use std::path::{Path, PathBuf};
use tauri::Manager;

/// to-zen 인자 조립. filter=Some이면 include 필터(-f). OUTPUT은 .utoc 경로(basename이 3종명).
pub fn build_to_zen_args(input: &Path, out_utoc: &Path, filter: Option<&str>) -> Vec<String> {
    let mut args = vec![
        "to-zen".to_string(),
        "--version".to_string(),
        "UE5_1".to_string(),
    ];
    if let Some(f) = filter {
        args.push("-f".to_string());
        args.push(f.to_string());
    }
    args.push(input.to_string_lossy().to_string());
    args.push(out_utoc.to_string_lossy().to_string());
    args
}

pub struct RetocRun {
    pub ok: bool,
    pub stderr: String,
}

/// retoc 실행. 실행 자체 실패(바이너리 없음/권한)만 Err, 변환 실패는 Ok(ok=false).
pub fn run(bin: &Path, args: &[String]) -> Result<RetocRun, String> {
    let out = std::process::Command::new(bin)
        .args(args)
        .output()
        .map_err(|e| format!("retoc 실행 실패: {e}"))?;
    Ok(RetocRun {
        ok: out.status.success(),
        stderr: String::from_utf8_lossy(&out.stderr).to_string(),
    })
}

/// 번들 retoc 경로(ue4ss.rs bundled() 패턴 재사용).
pub fn retoc_bin(app: &tauri::AppHandle) -> Result<PathBuf, String> {
    app.path()
        .resolve("resources/retoc", tauri::path::BaseDirectory::Resource)
        .map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn to_zen_args_basic() {
        let args = build_to_zen_args(Path::new("/in/Mod.pak"), Path::new("/out/Mod.utoc"), None);
        assert_eq!(args[0], "to-zen");
        assert_eq!(args[1], "--version");
        assert_eq!(args[2], "UE5_1");
        // INPUT then OUTPUT.utoc are the trailing positional args.
        assert_eq!(args[args.len() - 2], "/in/Mod.pak");
        assert_eq!(args[args.len() - 1], "/out/Mod.utoc");
    }

    #[test]
    fn to_zen_args_with_filter() {
        let args = build_to_zen_args(
            Path::new("/in/Mod.pak"),
            Path::new("/out/Mod.utoc"),
            Some("SK_Player"),
        );
        // include 필터는 -f <value>로 붙는다.
        let i = args.iter().position(|a| a == "-f").expect("-f present");
        assert_eq!(args[i + 1], "SK_Player");
    }
}
