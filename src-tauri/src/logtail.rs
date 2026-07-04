use std::fs;
use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// 파일의 마지막 max_bytes를 문자열로 반환. 파일이 없으면 안내문(에러 아님).
/// UTF-8 경계는 from_utf8_lossy로 안전 처리(잘린 선두 바이트는 대체문자).
pub fn read_tail(path: &Path, max_bytes: usize) -> Result<String, String> {
    if !path.exists() {
        return Ok("(No log yet - run the game once)".to_string());
    }
    let mut f = fs::File::open(path).map_err(|e| e.to_string())?;
    let len = f.metadata().map_err(|e| e.to_string())?.len();
    let start = len.saturating_sub(max_bytes as u64);
    f.seek(SeekFrom::Start(start)).map_err(|e| e.to_string())?;
    let mut buf = Vec::with_capacity((len - start) as usize);
    f.read_to_end(&mut buf).map_err(|e| e.to_string())?;
    Ok(String::from_utf8_lossy(&buf).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_file_returns_notice() {
        let p = std::env::temp_dir().join("pmm_no_such_log_xyz.log");
        let _ = fs::remove_file(&p);
        let s = read_tail(&p, 1024).unwrap();
        assert!(s.contains("No log yet"));
    }

    #[test]
    fn returns_last_bytes() {
        let dir = std::env::temp_dir().join("pmm_logtail");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("UE4SS.log");
        fs::write(&p, b"AAAAAAAAAABBBBBCCCCC").unwrap(); // 20바이트
        let s = read_tail(&p, 5).unwrap();
        assert_eq!(s, "CCCCC");
        // max가 파일보다 크면 전체
        let all = read_tail(&p, 1000).unwrap();
        assert_eq!(all, "AAAAAAAAAABBBBBCCCCC");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn tail_of_large_file_returns_exact_suffix() {
        let dir = std::env::temp_dir().join("pmm_logtail_big");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let p = dir.join("UE4SS.log");
        let body: Vec<u8> = (0..200_000u32).map(|i| b'A' + (i % 26) as u8).collect();
        fs::write(&p, &body).unwrap();
        let s = read_tail(&p, 64 * 1024).unwrap();
        assert_eq!(s.len(), 64 * 1024);
        assert!(body.ends_with(s.as_bytes()));
        let _ = fs::remove_dir_all(&dir);
    }
}
