use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

/// 명명된 활성 모드셋 스냅샷. `mods` = 켜는 id의 순서 목록(= active.json 스냅샷).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Profile {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub mods: Vec<String>,
}

/// 프로필 SSOT. active 프로필의 mods가 곧 작업셋(= 옛 active.json 역할).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProfileStore {
    pub active: String,
    #[serde(default)]
    pub profiles: Vec<Profile>,
}

impl ProfileStore {
    /// 불변식 보장: 프로필 ≥1, active가 실재 id를 가리킴.
    fn normalized(mut self) -> Self {
        if self.profiles.is_empty() {
            self.profiles.push(Profile {
                id: "default".into(),
                name: "Default".into(),
                mods: vec![],
            });
        }
        if !self.profiles.iter().any(|p| p.id == self.active) {
            self.active = self.profiles[0].id.clone();
        }
        self
    }

    /// 활성 프로필의 mods(없으면 빈 슬라이스).
    pub fn active_mods(&self) -> &[String] {
        self.profiles
            .iter()
            .find(|p| p.id == self.active)
            .map(|p| p.mods.as_slice())
            .unwrap_or(&[])
    }

    /// 디스크에서 로드 + 1회 마이그레이션.
    /// - 새 포맷({active,profiles}) 우선
    /// - 아니면: 옛 배열([{...}])이 있으면 스냅샷 채택 + 현재 작업셋(legacy_active)을
    ///   "Default"로 보존·활성화(없던 작업이 날아가지 않게).
    pub fn load(profiles_path: &Path, legacy_active: &[String]) -> ProfileStore {
        let raw = fs::read_to_string(profiles_path).ok();
        if let Some(s) = &raw {
            if let Ok(store) = serde_json::from_str::<ProfileStore>(s) {
                return store.normalized();
            }
        }
        let mut profiles: Vec<Profile> = raw
            .as_deref()
            .and_then(|s| serde_json::from_str::<Vec<Profile>>(s).ok())
            .unwrap_or_default();
        if !profiles.iter().any(|p| p.id == "default") {
            profiles.insert(
                0,
                Profile {
                    id: "default".into(),
                    name: "Default".into(),
                    mods: legacy_active.to_vec(),
                },
            );
        }
        ProfileStore {
            active: "default".into(),
            profiles,
        }
        .normalized()
    }

    /// 원자적 저장(임시→rename, 부모 보장).
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        let json = serde_json::to_string_pretty(self).map_err(|e| e.to_string())?;
        let tmp = path.with_extension("json.tmp");
        fs::write(&tmp, json).map_err(|e| e.to_string())?;
        fs::rename(&tmp, path).map_err(|e| e.to_string())?;
        Ok(())
    }

    pub fn set_enabled_in_active(&mut self, id: &str, enabled: bool) {
        let active = self.active.clone();
        if let Some(p) = self.profiles.iter_mut().find(|p| p.id == active) {
            crate::modstore::set_enabled(&mut p.mods, id, enabled);
        }
    }

    pub fn create(&mut self, name: &str) -> Result<Profile, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Profile name cannot be empty.".into());
        }
        let id = unique_id(&self.profiles, &slug(name));
        let p = Profile {
            id,
            name: name.to_string(),
            mods: vec![],
        };
        self.profiles.push(p.clone());
        Ok(p)
    }

    pub fn duplicate(&mut self, src_id: &str, name: &str) -> Result<Profile, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("Profile name cannot be empty.".into());
        }
        let mods = get(&self.profiles, src_id)
            .ok_or_else(|| format!("Profile not found: {src_id}"))?
            .mods;
        let id = unique_id(&self.profiles, &slug(name));
        let p = Profile {
            id,
            name: name.to_string(),
            mods,
        };
        self.profiles.push(p.clone());
        Ok(p)
    }

    pub fn rename(&mut self, id: &str, new_name: &str) -> Result<(), String> {
        let new_name = new_name.trim();
        if new_name.is_empty() {
            return Err("Profile name cannot be empty.".into());
        }
        match self.profiles.iter_mut().find(|p| p.id == id) {
            Some(p) => {
                p.name = new_name.to_string();
                Ok(())
            }
            None => Err(format!("Profile not found: {id}")),
        }
    }

    pub fn delete(&mut self, id: &str) -> Result<(), String> {
        if self.profiles.len() <= 1 {
            return Err("The last profile cannot be deleted.".into());
        }
        if id == self.active {
            return Err(
                "The active profile cannot be deleted. Switch to another profile first.".into(),
            );
        }
        let before = self.profiles.len();
        self.profiles.retain(|p| p.id != id);
        if self.profiles.len() == before {
            return Err(format!("Profile not found: {id}"));
        }
        Ok(())
    }

    pub fn set_active(&mut self, id: &str) -> Result<(), String> {
        if !self.profiles.iter().any(|p| p.id == id) {
            return Err(format!("Profile not found: {id}"));
        }
        self.active = id.to_string();
        Ok(())
    }

    pub fn remove_from_all(&mut self, id: &str) {
        for p in &mut self.profiles {
            p.mods.retain(|m| m != id);
        }
    }
}

/// 이름 → 안정적 id: 소문자, `[a-z0-9]`만 남기고 공백/`_`/`-`는 `-`로, 양끝 `-` 정리.
/// (prior art `manifest.slugify` 차용 — reference/crossover-app/internal/manifest/manifest.go)
pub fn slug(name: &str) -> String {
    let mut out = String::new();
    for c in name.trim().to_ascii_lowercase().chars() {
        if c.is_ascii_alphanumeric() {
            out.push(c);
        } else if c == ' ' || c == '_' || c == '-' {
            out.push('-');
        }
    }
    let trimmed = out.trim_matches('-').to_string();
    if trimmed.is_empty() {
        "profile".to_string()
    } else {
        trimmed
    }
}

/// id로 프로필 사본 조회.
pub fn get(profiles: &[Profile], id: &str) -> Option<Profile> {
    profiles.iter().find(|p| p.id == id).cloned()
}

/// known(라이브러리 실재 id)에 없는 id를 제거하되 순서는 보존(불변식 복구).
pub fn prune_missing(mods: &[String], known: &[String]) -> Vec<String> {
    mods.iter()
        .filter(|id| known.iter().any(|k| k == *id))
        .cloned()
        .collect()
}

/// 충돌 없는 id 생성(slug 그대로 또는 -2,-3…).
fn unique_id(existing: &[Profile], base: &str) -> String {
    if !existing.iter().any(|p| p.id == base) {
        return base.to_string();
    }
    let mut n = 2;
    loop {
        let cand = format!("{base}-{n}");
        if !existing.iter().any(|p| p.id == cand) {
            return cand;
        }
        n += 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slug_normalizes() {
        assert_eq!(slug("PvP Build"), "pvp-build");
        assert_eq!(slug("  My_Set! "), "my-set");
        assert_eq!(slug("***"), "profile");
    }

    #[test]
    fn prune_drops_unknown_keeps_order() {
        let mods = vec!["a".to_string(), "ghost".to_string(), "b".to_string()];
        let known = vec!["b".to_string(), "a".to_string()];
        assert_eq!(
            prune_missing(&mods, &known),
            vec!["a".to_string(), "b".to_string()]
        );
    }

    #[test]
    fn load_migrates_legacy_active_to_default() {
        let dir = std::env::temp_dir().join("pmm_ps_load");
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        let path = dir.join("profiles.json");

        // (1) 파일 없음 + legacy active.json = [a,b] → Default(active=[a,b]) 활성
        let st = ProfileStore::load(&path, &["a".into(), "b".into()]);
        assert_eq!(st.active, "default");
        assert_eq!(st.profiles.len(), 1);
        assert_eq!(st.active_mods(), &["a".to_string(), "b".to_string()]);

        // (2) 새 포맷 저장 후 재로드 = 그대로(불변식 유지)
        st.save(&path).unwrap();
        let again = ProfileStore::load(&path, &[]);
        assert_eq!(again.active, "default");
        assert_eq!(again.active_mods(), &["a".to_string(), "b".to_string()]);

        // (3) 옛 배열 포맷 + legacy active → 스냅샷 보존 + Default(작업셋) 활성
        let legacy = r#"[{"id":"pvp","name":"PvP","mods":["x"]}]"#;
        fs::write(&path, legacy).unwrap();
        let mig = ProfileStore::load(&path, &["a".into()]);
        assert_eq!(mig.active, "default");
        assert!(mig.profiles.iter().any(|p| p.id == "pvp"));
        assert_eq!(mig.active_mods(), &["a".to_string()]);

        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn store_mutations() {
        let mut st = ProfileStore {
            active: "default".into(),
            profiles: vec![Profile {
                id: "default".into(),
                name: "Default".into(),
                mods: vec![],
            }],
        };

        // 토글: 활성 프로필 mods에 순서대로 반영
        st.set_enabled_in_active("a", true);
        st.set_enabled_in_active("b", true);
        st.set_enabled_in_active("a", false);
        assert_eq!(st.active_mods(), &["b".to_string()]);

        // create: 빈 프로필, 비활성(디스크/active 무변경)
        let p = st.create("PvP Build").unwrap();
        assert_eq!(p.id, "pvp-build");
        assert!(p.mods.is_empty());
        assert_eq!(st.active, "default");

        // duplicate: src의 mods 복사, id 충돌 회피
        st.set_active("pvp-build").unwrap();
        st.set_enabled_in_active("a", true);
        let dup = st.duplicate("pvp-build", "PvP Build").unwrap(); // 같은 이름 → id 유니크화
        assert_eq!(dup.id, "pvp-build-2");
        assert_eq!(dup.mods, vec!["a".to_string()]);

        // delete 가드: 활성 삭제 거부
        assert!(st.delete("pvp-build").is_err());
        // 비활성은 삭제 가능
        st.set_active("default").unwrap();
        st.delete("pvp-build").unwrap();
        // remove_from_all
        st.set_enabled_in_active("z", true);
        st.remove_from_all("z");
        assert!(!st.active_mods().contains(&"z".to_string()));
        // 마지막 프로필 삭제 거부
        while st.profiles.len() > 1 {
            let id = st.profiles.last().unwrap().id.clone();
            let _ = st.delete(&id);
        }
        assert!(st.delete(&st.active.clone()).is_err());
    }
}
