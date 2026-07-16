//! 앱 자체 업데이트 체크: GitHub releases/latest 태그를 현재 앱 버전과 비교한다.
//! ad-hoc 서명이라 자동설치는 불가 — 새 버전이면 release_url(브라우저 오픈)로 폴백한다.
//! 모드/UE4SS 업데이트의 프리미티브(updater::version_gt)를 재사용한다.

/// 앱 릴리즈 출처 레포(GitHub). UE4SS와 달리 zip 에셋이 아니라 릴리즈 페이지로 안내한다.
const REPO: &str = "h-taek/PalworldModManager";

/// 프런트로 보내는 앱 업데이트 상태.
#[derive(serde::Serialize)]
pub struct AppStatus {
    /// 현재 실행 중인 앱 버전(package_info).
    pub current: String,
    /// 최신 릴리즈 태그(조회 성공 시).
    pub latest: Option<String>,
    pub update_available: bool,
    /// 새 버전이 있을 때 열어줄 릴리즈 페이지 URL(자동설치 아님).
    pub release_url: Option<String>,
    /// 비치명적 조회 실패 메시지(오프라인 등).
    pub error: Option<String>,
}

#[derive(serde::Deserialize)]
struct GhRelease {
    tag_name: String,
    html_url: String,
}

/// releases/latest JSON에서 태그 + 릴리즈 페이지 URL을 뽑는다. (tag, html_url).
pub fn parse_release(json: &str) -> Result<(String, String), String> {
    let rel: GhRelease =
        serde_json::from_str(json).map_err(|e| format!("Failed to parse release JSON: {e}"))?;
    Ok((rel.tag_name, rel.html_url))
}

/// GitHub releases/latest 조회 → (태그, 릴리즈 URL). (네트워크 — 수동 스모크)
fn fetch_latest() -> Result<(String, String), String> {
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

/// 조회 결과를 받아 상태를 계산한다(네트워크와 분리 — 테스트용).
pub fn status_with(current: &str, fetched: Result<(String, String), String>) -> AppStatus {
    let mut st = AppStatus {
        current: current.to_string(),
        latest: None,
        update_available: false,
        release_url: None,
        error: None,
    };
    match fetched {
        Ok((tag, url)) => {
            st.update_available = crate::updater::version_gt(&tag, current);
            st.latest = Some(tag);
            st.release_url = Some(url);
        }
        Err(e) => st.error = Some(e),
    }
    st
}

/// 시작 시/설정 버튼에서 호출: 메타만 조회(다운로드 안 함). 실패는 error 필드로.
pub fn status(current: &str) -> AppStatus {
    status_with(current, fetch_latest())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_release_extracts_tag_and_html_url() {
        let json = r#"{"tag_name":"v0.1.2","html_url":"https://github.com/h-taek/PalworldModManager/releases/tag/v0.1.2","assets":[]}"#;
        let (tag, url) = parse_release(json).unwrap();
        assert_eq!(tag, "v0.1.2");
        assert_eq!(url, "https://github.com/h-taek/PalworldModManager/releases/tag/v0.1.2");
    }

    #[test]
    fn parse_release_errors_on_missing_field() {
        // html_url 없음 → 파싱 실패
        let json = r#"{"tag_name":"v0.1.2"}"#;
        assert!(parse_release(json).is_err());
    }

    #[test]
    fn status_flags_update_when_latest_is_newer() {
        let st = status_with("0.1.1", Ok(("v0.1.2".into(), "https://x/rel".into())));
        assert!(st.update_available);
        assert_eq!(st.latest.as_deref(), Some("v0.1.2"));
        assert_eq!(st.release_url.as_deref(), Some("https://x/rel"));
        assert!(st.error.is_none());
    }

    #[test]
    fn status_no_update_when_equal() {
        let st = status_with("0.1.2", Ok(("v0.1.2".into(), "https://x/rel".into())));
        assert!(!st.update_available);
        assert_eq!(st.latest.as_deref(), Some("v0.1.2"));
    }

    #[test]
    fn status_no_update_when_current_is_newer() {
        // dev 로컬 버전이 릴리즈보다 높은 경우
        let st = status_with("0.2.0", Ok(("v0.1.2".into(), "https://x/rel".into())));
        assert!(!st.update_available);
    }

    #[test]
    fn status_carries_error_and_stays_silent() {
        let st = status_with("0.1.1", Err("Request failed: offline".into()));
        assert!(!st.update_available);
        assert!(st.latest.is_none());
        assert!(st.release_url.is_none());
        assert_eq!(st.error.as_deref(), Some("Request failed: offline"));
    }
}
