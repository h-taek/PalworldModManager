use std::fs;
use std::io::Write;
use std::path::Path;
use std::process::Command;

/// 컨테이너 UE4SS 런타임 골격 보장: `Mods/` 디렉터리 + `UE4SS-settings.ini`.
/// settings.ini가 없으면 번들된 known-good 설정(`settings_src`)을 복사한다.
/// 이미 있으면 보존(기존/사용자 설정 클로버 방지) — 멱등.
pub fn ensure_runtime(ue4ss_dir: &Path, settings_src: &Path) -> std::io::Result<()> {
    fs::create_dir_all(ue4ss_dir.join("Mods"))?;
    let settings = ue4ss_dir.join("UE4SS-settings.ini");
    if !settings.exists() {
        fs::copy(settings_src, &settings)?;
    }
    Ok(())
}

/// 로더 모드(BPModLoaderMod + shared) 프로비저닝: 번들 리소스(`loader_src`)에서
/// 컨테이너 `mods_dir`로 복사. 매니저 소유 런타임 픽스처 — 매 호출 최신 번들로 refresh(멱등).
/// `BPModLoaderMod/enabled.txt`를 보존하므로 mods.txt와 무관하게 UE4SS가 자동 활성화한다.
/// reconcile은 이 디렉터리들을 라이브러리 id/prev_deployed가 아니라서 건드리지 않는다.
/// 원자적 교체(임시 디렉터리→rename)로 부분 복사 상태를 남기지 않는다.
pub fn provision_loader_mods(mods_dir: &Path, loader_src: &Path) -> std::io::Result<()> {
    fs::create_dir_all(mods_dir)?;
    for name in ["BPModLoaderMod", "shared"] {
        let src = loader_src.join(name);
        if !src.is_dir() {
            continue; // 번들 누락 방어(있는 것만 배치)
        }
        let dst = mods_dir.join(name);
        let tmp = mods_dir.join(format!(".{name}.tmp"));
        let _ = fs::remove_dir_all(&tmp);
        crate::library::copy_tree(&src, &tmp, &[])?; // copy_tree: pub(crate), -> std::io::Result<()>
        let _ = fs::remove_dir_all(&dst);
        fs::rename(&tmp, &dst)?;
    }
    Ok(())
}

/// mods.txt 작성: 입력 순서가 곧 로드 순서. 원자적 쓰기(임시→rename).
pub fn write_mods_txt(mods_dir: &Path, entries: &[(String, bool)]) -> std::io::Result<()> {
    fs::create_dir_all(mods_dir)?;
    let mut body = String::new();
    for (name, enabled) in entries {
        body.push_str(name);
        body.push(':');
        body.push(if *enabled { '1' } else { '0' });
        body.push('\n');
    }
    let tmp = mods_dir.join(".mods.txt.tmp");
    {
        let mut f = fs::File::create(&tmp)?;
        f.write_all(body.as_bytes())?;
        f.sync_all()?;
    }
    fs::rename(&tmp, mods_dir.join("mods.txt"))?;
    Ok(())
}

/// 프로브 파일 생성 성공 여부로 실제 쓰기 가능 판정(소유권 포함).
pub fn is_writable(dir: &Path) -> bool {
    if !dir.is_dir() { return false; }
    let probe = dir.join(".pmm_write_probe");
    match fs::File::create(&probe) {
        Ok(_) => { let _ = fs::remove_file(&probe); true }
        Err(_) => false,
    }
}

/// POSIX 안전 작은따옴표 감싸기: 내부의 ' 는 '\'' 스플라이스로 이스케이프.
fn shell_single_quote(s: &str) -> String {
    format!("'{}'", s.replace('\'', "'\\''"))
}

/// 3폴더를 한 번에 생성+chown 하는 셸 스크립트(경로는 작은따옴표로 격리).
pub fn build_chown_script(paths: &[&Path], user: &str) -> String {
    let quoted: Vec<String> = paths.iter().map(|p| shell_single_quote(&p.display().to_string())).collect();
    let joined = quoted.join(" ");
    format!("mkdir -p {joined} && chown {} {joined}", shell_single_quote(user))
}

/// 번들 3폴더가 쓰기 가능하도록 보장. 하나라도 불가면 관리자 권한으로 생성+chown(암호 1회).
pub fn ensure_bundle_writable(mods_dir: &Path, logicmods_dir: &Path, lua_dir: &Path) -> Result<(), String> {
    let dirs = [mods_dir, logicmods_dir, lua_dir];
    if dirs.iter().all(|d| is_writable(d)) {
        return Ok(());
    }
    let user = std::env::var("USER").map_err(|_| "USER 환경변수 없음".to_string())?;
    let shell = build_chown_script(&dirs, &user);
    // AppleScript 문자열 escape: 역슬래시·큰따옴표.
    let esc = shell.replace('\\', "\\\\").replace('"', "\\\"");
    let apple = format!("do shell script \"{esc}\" with administrator privileges");
    let out = Command::new("osascript").arg("-e").arg(apple).output()
        .map_err(|e| format!("osascript 실행 실패: {e}"))?;
    if !out.status.success() {
        return Err(format!("권한 상승 실패(취소됨?): {}", String::from_utf8_lossy(&out.stderr).trim()));
    }
    if !dirs.iter().all(|d| is_writable(d)) {
        return Err("권한 상승 후에도 폴더가 쓰기 불가".into());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn provision_loader_mods_copies_refreshes_and_preserves_others() {
        let base = std::env::temp_dir().join("pmm_provision_loader");
        let _ = fs::remove_dir_all(&base);
        // 가짜 번들 리소스(loader-mods)
        let src = base.join("resources/loader-mods");
        fs::create_dir_all(src.join("BPModLoaderMod/Scripts")).unwrap();
        fs::write(src.join("BPModLoaderMod/enabled.txt"), b"").unwrap(); // 0바이트
        fs::write(src.join("BPModLoaderMod/Scripts/main.lua"), b"v1").unwrap();
        fs::create_dir_all(src.join("shared/UEHelpers")).unwrap();
        fs::write(src.join("shared/UEHelpers/UEHelpers.lua"), b"helpers").unwrap();

        // 컨테이너 Mods/ + 사용자가 둔 외부 모드(보존돼야 함)
        let mods = base.join("Mods");
        fs::create_dir_all(mods.join("External/Scripts")).unwrap();
        fs::write(mods.join("External/Scripts/main.lua"), b"ext").unwrap();

        // 1차 프로비저닝
        provision_loader_mods(&mods, &src).unwrap();
        assert!(mods.join("BPModLoaderMod/enabled.txt").is_file(), "enabled.txt(자동활성화) 존재");
        assert_eq!(fs::read(mods.join("BPModLoaderMod/enabled.txt")).unwrap().len(), 0, "0바이트 보존");
        assert_eq!(fs::read_to_string(mods.join("BPModLoaderMod/Scripts/main.lua")).unwrap(), "v1");
        assert!(mods.join("shared/UEHelpers/UEHelpers.lua").is_file());
        assert!(mods.join("External/Scripts/main.lua").is_file(), "외부 모드 보존");

        // 번들 갱신 후 재프로비저닝 → 최신본으로 refresh(덮어쓰기)
        fs::write(src.join("BPModLoaderMod/Scripts/main.lua"), b"v2").unwrap();
        provision_loader_mods(&mods, &src).unwrap();
        assert_eq!(fs::read_to_string(mods.join("BPModLoaderMod/Scripts/main.lua")).unwrap(), "v2", "번들 갱신 반영");
        assert!(mods.join("External/Scripts/main.lua").is_file(), "재실행에도 외부 모드 보존");

        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn provision_loader_mods_tolerates_missing_component() {
        let base = std::env::temp_dir().join("pmm_provision_loader_missing");
        let _ = fs::remove_dir_all(&base);
        let src = base.join("resources/loader-mods");
        // shared 없이 BPModLoaderMod만
        fs::create_dir_all(src.join("BPModLoaderMod")).unwrap();
        fs::write(src.join("BPModLoaderMod/enabled.txt"), b"").unwrap();
        let mods = base.join("Mods");
        fs::create_dir_all(&mods).unwrap();
        // 누락 컴포넌트가 있어도 에러 없이 있는 것만 배치
        provision_loader_mods(&mods, &src).unwrap();
        assert!(mods.join("BPModLoaderMod/enabled.txt").is_file());
        assert!(!mods.join("shared").exists());
        let _ = fs::remove_dir_all(&base);
    }

    #[test]
    fn ensure_runtime_copies_settings_then_preserves() {
        let tmp = std::env::temp_dir().join("pmm_stage_test");
        let _ = fs::remove_dir_all(&tmp);
        // 가짜 번들 settings 소스
        let src_dir = std::env::temp_dir().join("pmm_stage_src");
        let _ = fs::remove_dir_all(&src_dir);
        fs::create_dir_all(&src_dir).unwrap();
        let settings_src = src_dir.join("UE4SS-settings.ini");
        fs::write(&settings_src, b"[General]\nUseCache = 1\n").unwrap();

        // 최초 스테이징: 디렉터리 + 설정 복사
        ensure_runtime(&tmp, &settings_src).unwrap();
        assert!(tmp.join("Mods").is_dir());
        let staged = tmp.join("UE4SS-settings.ini");
        assert!(staged.is_file());
        assert_eq!(
            fs::read_to_string(&staged).unwrap(),
            "[General]\nUseCache = 1\n"
        );

        // 멱등: 기존 settings는 보존(덮어쓰지 않음)
        fs::write(&staged, b"CUSTOM").unwrap();
        ensure_runtime(&tmp, &settings_src).unwrap();
        assert_eq!(fs::read_to_string(&staged).unwrap(), "CUSTOM");

        let _ = fs::remove_dir_all(&tmp);
        let _ = fs::remove_dir_all(&src_dir);
    }

    #[test]
    fn write_mods_txt_orders_and_flags() {
        let tmp = std::env::temp_dir().join("pmm_modstxt_test");
        let _ = fs::remove_dir_all(&tmp);
        let entries = vec![
            ("AutoHatch".to_string(), true),
            ("Other".to_string(), false),
        ];
        write_mods_txt(&tmp, &entries).unwrap();
        let got = fs::read_to_string(tmp.join("mods.txt")).unwrap();
        assert_eq!(got, "AutoHatch:1\nOther:0\n");
        let _ = fs::remove_dir_all(&tmp);
    }

    #[test]
    fn build_chown_script_quotes_all_three_paths() {
        let a = Path::new("/Applications/Palworld.app/Contents/UE/Pal/Content/Paks/~mods");
        let b = Path::new("/Applications/Palworld.app/Contents/UE/Pal/Content/Paks/LogicMods");
        let c = Path::new("/Applications/Palworld.app/Contents/UE/Pal/Binaries/Win64/Mods");
        let s = build_chown_script(&[a, b, c], "alice");
        assert!(s.contains("mkdir -p"));
        assert!(s.contains("chown alice") || s.contains("chown 'alice'"));
        for p in [a, b, c] {
            assert!(s.contains(&format!("'{}'", p.display())), "경로는 작은따옴표로 감싸야: {}", p.display());
        }
    }

    #[test]
    fn build_chown_script_escapes_embedded_single_quote() {
        let p = Path::new("/Users/bob/Bob's Games/Palworld.app/Content/Paks/~mods");
        let s = build_chown_script(&[p], "o'brien");
        // 경로·유저명 모두 '\'' 스플라이스로 이스케이프되어 셸 리터럴이 깨지지 않음
        assert!(s.contains(r"Bob'\''s Games"), "경로 내 작은따옴표 이스케이프: {s}");
        assert!(s.contains(r"chown 'o'\''brien'"), "유저명 이스케이프: {s}");
    }

    #[test]
    fn is_writable_true_for_temp_false_for_missing() {
        let d = std::env::temp_dir().join("pmm_writable_probe");
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        assert!(is_writable(&d));
        assert!(!is_writable(&d.join("nope")));
        let _ = fs::remove_dir_all(&d);
    }
}
