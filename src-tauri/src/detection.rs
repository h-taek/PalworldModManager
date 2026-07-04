use crate::paths;
use serde::Serialize;
use std::path::Path;

#[derive(Debug, Serialize, PartialEq)]
pub struct DetectionResult {
    pub game_installed: bool,
    pub container_exists: bool,
}

/// 순수 판정: 주어진 경로들의 존재 여부만 본다(테스트 가능).
pub fn detect(home: &Path, game_bin: &Path) -> DetectionResult {
    DetectionResult {
        game_installed: game_bin.exists(),
        container_exists: paths::container_root(home).exists(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn detects_present_and_absent() {
        let tmp = std::env::temp_dir().join("pmm_detect_test");
        let _ = fs::remove_dir_all(&tmp);
        // 게임 바이너리 역할의 가짜 파일
        let game_bin = tmp.join("Palworld");
        fs::create_dir_all(&tmp).unwrap();
        fs::write(&game_bin, b"x").unwrap();
        // 컨테이너 디렉터리 생성
        let container = paths::container_root(&tmp);
        fs::create_dir_all(&container).unwrap();

        let r = detect(&tmp, &game_bin);
        assert_eq!(
            r,
            DetectionResult {
                game_installed: true,
                container_exists: true
            }
        );

        // 컨테이너 제거 후 false 확인
        fs::remove_dir_all(&container).unwrap();
        let r2 = detect(&tmp, &game_bin);
        assert!(!r2.container_exists);
        assert!(r2.game_installed);

        let _ = fs::remove_dir_all(&tmp);
    }
}
