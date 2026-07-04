//! 단일 pak → 3종 변환의 순수부(분류·결과 타입). repak 추출·retoc 결합 IO는 library.rs(Task 5).

#[allow(dead_code)] // files: 계약상 반환하나 현 버전은 소비하지 않음; removed는 I1(front 통보)에서 소비.
pub enum ConvertResult {
    /// 변환 성공. files=3종 경로, removed=변환 위해 제거한 비에셋 목록(없으면 빈 벡터).
    Converted {
        files: Vec<std::path::PathBuf>,
        removed: Vec<String>,
    },
    /// 비에셋 제거 후에도 retoc 실패. removed=제거한 것, stderr=retoc 마지막 에러.
    NeedsUserDecision {
        removed: Vec<String>,
        stderr: String,
    },
}

/// 엔진이 로드하지 않는 비에셋(모더 메모류)이면 true. 블랙리스트 —
/// 진짜 에셋(.uasset/.uexp/.ubulk/.umap/.ushaderbytecode 등)을 잘못 버리지 않기 위함(spec §7-4).
pub fn is_non_asset(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    const JUNK_EXT: &[&str] = &[".txt", ".md", ".pdf", ".rtf", ".doc", ".docx", ".html", ".url"];
    if JUNK_EXT.iter().any(|e| lower.ends_with(e)) {
        return true;
    }
    let base = lower.rsplit('/').next().unwrap_or(&lower);
    base == "readme" || base == "license" || base == "credits"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn non_asset_classifies_text_and_docs() {
        assert!(is_non_asset("Content/notes.txt"));
        assert!(is_non_asset("README.md"));
        assert!(is_non_asset("Pal/Content/readme"));
        assert!(!is_non_asset("Content/Mesh.uasset"));
        assert!(!is_non_asset("Content/Mesh.uexp"));
        assert!(!is_non_asset("Content/Tex.ubulk"));
    }
}
