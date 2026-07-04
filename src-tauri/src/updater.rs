use serde::Deserialize;
use std::fs;
use std::io::{Read, Write};
use std::path::Path;

/// 모드 원격 버전 매니페스트(updateURL이 가리키는 JSON).
/// 최소 스키마: { "version": "1.2.0", "url": "https://.../mod.zip" }.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RemoteManifest {
    pub version: String,
    #[serde(default)]
    pub url: Option<String>,
}

/// 원격 JSON → RemoteManifest.
pub fn parse_remote(json: &str) -> Result<RemoteManifest, String> {
    serde_json::from_str::<RemoteManifest>(json)
        .map_err(|e| format!("Failed to parse remote manifest: {e}"))
}

/// a가 b보다 새 버전인가. 점으로 나눈 숫자 비교(semver-lite), 숫자 파싱 실패 시 문자열 비교 폴백.
/// 선행 `v` 허용. 길이 다른 버전은 짧은 쪽을 0 패딩("1.3" == "1.3.0").
pub fn version_gt(a: &str, b: &str) -> bool {
    match (parse_version(a), parse_version(b)) {
        (Some(mut va), Some(mut vb)) => {
            let n = va.len().max(vb.len());
            va.resize(n, 0);
            vb.resize(n, 0);
            va > vb
        }
        _ => a.trim() > b.trim(),
    }
}

fn parse_version(s: &str) -> Option<Vec<u64>> {
    let s = s.trim().trim_start_matches('v');
    if s.is_empty() {
        return None;
    }
    let mut out = Vec::new();
    for part in s.split('.') {
        out.push(part.parse::<u64>().ok()?);
    }
    Some(out)
}

/// 200MB 상한(updateURL은 신뢰 입력이 아님 — 무제한 다운로드 방지).
const MAX_DOWNLOAD_BYTES: u64 = 200 * 1024 * 1024;

/// reader→writer 복사하되 max 초과 시 에러. 64KB 버퍼.
pub fn copy_capped(
    reader: &mut impl Read,
    writer: &mut impl Write,
    max: u64,
) -> Result<u64, String> {
    let mut buf = [0u8; 64 * 1024];
    let mut total: u64 = 0;
    loop {
        let n = reader.read(&mut buf).map_err(|e| e.to_string())?;
        if n == 0 {
            break;
        }
        total += n as u64;
        if total > max {
            return Err(format!(
                "Download exceeded the limit ({MAX_DOWNLOAD_BYTES} bytes)."
            ));
        }
        writer.write_all(&buf[..n]).map_err(|e| e.to_string())?;
    }
    Ok(total)
}

/// URL에서 텍스트 GET(블로킹). 네트워크 I/O — 단위테스트 없음(수동 스모크).
pub fn fetch_text(url: &str) -> Result<String, String> {
    ureq::get(url)
        .call()
        .map_err(|e| format!("Request failed: {e}"))?
        .into_string()
        .map_err(|e| format!("Failed to read response body: {e}"))
}

/// URL을 dest로 다운로드(블로킹, 원자적 임시→rename). 네트워크 I/O — 수동 스모크.
pub fn download(url: &str, dest: &Path) -> Result<(), String> {
    let resp = ureq::get(url)
        .call()
        .map_err(|e| format!("Download failed: {e}"))?;
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let tmp = dest.with_extension("download.tmp");
    let mut reader = resp.into_reader();
    let mut file = fs::File::create(&tmp).map_err(|e| e.to_string())?;
    // FIX 2: best-effort cleanup of partial temp file on copy/sync failure
    if let Err(e) = copy_capped(&mut reader, &mut file, MAX_DOWNLOAD_BYTES) {
        let _ = fs::remove_file(&tmp);
        return Err(e);
    }
    if let Err(e) = file.sync_all() {
        let _ = fs::remove_file(&tmp);
        return Err(e.to_string());
    }
    drop(file);
    fs::rename(&tmp, dest).map_err(|e| e.to_string())?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_remote_minimal() {
        let m = parse_remote(r#"{"version":"1.2.0","url":"https://x/m.zip"}"#).unwrap();
        assert_eq!(m.version, "1.2.0");
        assert_eq!(m.url.as_deref(), Some("https://x/m.zip"));
        // url 없어도 파싱됨
        let m2 = parse_remote(r#"{"version":"2.0.0"}"#).unwrap();
        assert_eq!(m2.url, None);
        // version 누락 → 에러
        assert!(parse_remote(r#"{"url":"https://x"}"#).is_err());
    }

    #[test]
    fn version_gt_numeric_not_lexical() {
        assert!(version_gt("1.2.0", "1.1.0"));
        assert!(!version_gt("1.0.0", "1.0.0"));
        assert!(!version_gt("1.0.0", "1.2.0"));
        // 숫자 비교라 "0.16" > "0.9" (문자열이면 "16" < "9"라 틀림)
        assert!(version_gt("0.16", "0.9"));
        // 선행 v 허용
        assert!(version_gt("v2.0", "1.9"));
        // 비숫자 → 문자열 폴백(같으면 false)
        assert!(!version_gt("beta", "beta"));
    }

    #[test]
    fn version_gt_length_padding() {
        assert!(!version_gt("1.3", "1.3.0")); // 동등(짧은 쪽 0 패딩)
        assert!(!version_gt("1.3.0", "1.3")); // 동등
        assert!(version_gt("1.3.1", "1.3")); // 1.3.1 > 1.3.0
        assert!(version_gt("1.3", "1.2.9")); // 1.3.0 > 1.2.9
    }

    #[test]
    fn copy_capped_enforces_limit() {
        use std::io::Cursor;
        // 한도 이하 = OK
        let mut r = Cursor::new(vec![7u8; 100]);
        let mut w: Vec<u8> = Vec::new();
        assert_eq!(copy_capped(&mut r, &mut w, 1000).unwrap(), 100);
        assert_eq!(w.len(), 100);
        // 한도 초과 = Err
        let mut r2 = Cursor::new(vec![7u8; 5000]);
        let mut w2: Vec<u8> = Vec::new();
        assert!(copy_capped(&mut r2, &mut w2, 1000).is_err());
    }
}
