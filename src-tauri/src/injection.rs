use std::fs::File;
use std::path::Path;
use std::process::{Command, Stdio};

/// 검증된 런처 동작 재현: DYLD_INSERT_LIBRARIES=<dylib> 환경으로 게임 실행 준비.
/// (argv 전달 없음 — launch-ue4ss.sh:18과 동일)
pub fn build_command(game: &Path, dylib: &Path) -> Command {
    let mut cmd = Command::new(game);
    cmd.env("DYLD_INSERT_LIBRARIES", dylib);
    cmd
}

/// 게임 stdout/stderr를 로그 파일로 리다이렉트(터미널 런처의 `> "$LOG" 2>&1` 재현).
///
/// 이유: 기본값 Stdio::inherit는 게임 로그를 아무도 읽지 않는 매니저 fd로 흘려보낸다.
/// 파이프 버퍼(64KB)가 차는 순간 게임 write()가 블로킹 → 게임 스레드 정지 → 검은화면 hang.
/// 파일은 backpressure가 없어 로그량과 무관하게 항상 안전(defense-in-depth).
/// stderr는 stdout 핸들을 try_clone해 같은 파일에 합친다(`2>&1`과 동일: 공유 오프셋에 append).
pub fn redirect_output_to(cmd: &mut Command, log_path: &Path) -> Result<(), String> {
    if let Some(parent) = log_path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| format!("create log dir {}: {e}", parent.display()))?;
    }
    // create = 매 실행마다 truncate. 직전 실행 로그를 덮어써 항상 최신 세션만 남긴다.
    let f = File::create(log_path)
        .map_err(|e| format!("open launch log {}: {e}", log_path.display()))?;
    let ferr = f
        .try_clone()
        .map_err(|e| format!("clone launch log handle: {e}"))?;
    cmd.stdout(Stdio::from(f)).stderr(Stdio::from(ferr));
    Ok(())
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

    #[test]
    fn redirect_creates_dir_and_truncates_log() {
        let base = std::env::temp_dir().join("pmm_launch_log_test");
        let _ = std::fs::remove_dir_all(&base);
        let log = base.join("Caches/ue4ss-mac/palworld-launch.log");

        // 1) 부모 디렉터리가 없어도 생성되고 로그 파일이 열린다.
        let mut cmd = Command::new("/bin/echo");
        redirect_output_to(&mut cmd, &log).unwrap();
        assert!(log.exists(), "launch log must be created");

        // 2) 직전 실행 로그가 남아 있어도 매 실행마다 truncate 된다.
        std::fs::write(&log, b"stale content from a previous launch").unwrap();
        let mut cmd2 = Command::new("/bin/echo");
        redirect_output_to(&mut cmd2, &log).unwrap();
        assert_eq!(
            std::fs::metadata(&log).unwrap().len(),
            0,
            "launch log must be truncated on each launch"
        );

        let _ = std::fs::remove_dir_all(&base);
    }

    /// hang의 핵심 위험을 로더 버전과 무관하게 OS 레벨에서 직접 증명한다.
    /// (게임-레벨 재현은 0.2.1 dylib이 있어야 로그가 64KB를 넘겨 불가; 여기선 순수 파이프 동작만 본다.)
    ///
    /// 대조군 = 버그 재현: stdout이 아무도 안 읽는 파이프면 256KB writer가 64KB에서 막혀 안 끝난다.
    /// 처치군 = 수정 증명: redirect_output_to로 파일에 보내면 같은 writer가 즉시 끝나고 전량 기록된다.
    ///
    /// 2초 블로킹 대기가 있어 기본 스위트에서 제외(`--ignored`로 실행). `yes | head -c` = macOS 기준.
    #[test]
    #[ignore = "느린 실측 검증(2s sleep) — cargo test -- --ignored 로 실행"]
    fn redirect_absorbs_high_volume_stdout_without_blocking() {
        use std::time::Duration;
        const VOLUME: &str = "yes | head -c 262144"; // 256KB = 파이프 버퍼(64KB)의 4배

        // --- 대조군: inherit-형 파이프(안 읽음) → writer가 막혀야 정상 ---
        let mut blocked = Command::new("sh")
            .arg("-c")
            .arg(VOLUME)
            .stdout(Stdio::piped()) // 부모가 절대 read 안 함
            .spawn()
            .unwrap();
        std::thread::sleep(Duration::from_secs(2));
        assert!(
            blocked.try_wait().unwrap().is_none(),
            "대조군: 안 읽는 파이프면 writer가 64KB에서 블로킹돼 아직 안 끝나야 함(=hang 메커니즘 실재 확인)"
        );
        blocked.kill().ok();
        blocked.wait().ok();

        // --- 처치군: redirect_output_to(파일) → writer가 막힘 없이 끝나야 함 ---
        let base = std::env::temp_dir().join("pmm_backpressure_test");
        let _ = std::fs::remove_dir_all(&base);
        let log = base.join("palworld-launch.log");
        let mut cmd = Command::new("sh");
        cmd.arg("-c").arg(VOLUME);
        redirect_output_to(&mut cmd, &log).unwrap();
        let mut child = cmd.spawn().unwrap();

        let mut finished = false;
        for _ in 0..40 {
            if child.try_wait().unwrap().is_some() {
                finished = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
        assert!(
            finished,
            "처치군: stdout을 파일로 보내면 backpressure가 없어 writer가 끝나야 함(hang 제거 증명)"
        );
        let len = std::fs::metadata(&log).unwrap().len();
        assert!(
            len > 200_000,
            "처치군: 64KB를 훨씬 넘는 전량이 파일에 기록돼야 함(실측 {len} bytes)"
        );

        let _ = std::fs::remove_dir_all(&base);
    }
}
