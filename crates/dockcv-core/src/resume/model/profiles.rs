//! Reusable, vault-wide page layouts.
//!
//! A document owns one fallback [`LayoutSettings`]. A profile is a named
//! alternative that can be shared by any number of documents without copying
//! those settings into every preset. The catalog stores user profiles only;
//! the two built-ins are resolved from code, so a fresh vault needs no
//! `profiles.toml`.

use serde::{Deserialize, Serialize};

use super::layout::LayoutSettings;

pub const DEFAULT_PROFILE: &str = "Default";
pub const ATS_SAFE_PROFILE: &str = "ATS-safe";

/// One named, reusable layout.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct LayoutProfile {
    pub name: String,
    pub layout: LayoutSettings,
}

/// The user-defined part of `<vault>/profiles.toml`.
///
/// A stored profile with a built-in name is an intentional override. This is
/// what makes `Update ATS-safe` possible without changing the binary or
/// duplicating raw layout values into every document that uses it.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct ProfileCatalog {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub profiles: Vec<LayoutProfile>,
}

impl ProfileCatalog {
    /// Resolve a name, preferring a vault override over the shipped value.
    pub fn get(&self, name: &str) -> Option<LayoutSettings> {
        self.profiles
            .iter()
            .find(|profile| profile.name == name)
            .map(|profile| profile.layout)
            .or_else(|| builtin(name))
    }

    /// The layout a document should render with.
    ///
    /// A missing hand-edited profile name falls back to the document's own
    /// layout. Losing presentation because one shared file was renamed would
    /// be the opposite of File-over-App; the unresolved name remains stored so
    /// the UI can expose and repair it.
    pub fn resolve(
        &self,
        profile: Option<&str>,
        document_layout: LayoutSettings,
    ) -> LayoutSettings {
        profile
            .and_then(|name| self.get(name))
            .unwrap_or(document_layout)
    }

    /// Every available name, built-ins first and each name exactly once.
    pub fn names(&self) -> Vec<String> {
        let mut names = vec![DEFAULT_PROFILE.to_string(), ATS_SAFE_PROFILE.to_string()];
        for profile in &self.profiles {
            if !names.contains(&profile.name) {
                names.push(profile.name.clone());
            }
        }
        names
    }

    /// Create or replace a vault profile.
    pub fn upsert(&mut self, name: impl Into<String>, layout: LayoutSettings) {
        let name = name.into();
        if let Some(profile) = self
            .profiles
            .iter_mut()
            .find(|profile| profile.name == name)
        {
            profile.layout = layout;
        } else {
            self.profiles.push(LayoutProfile { name, layout });
        }
    }

    pub fn contains_name(&self, name: &str) -> bool {
        self.get(name).is_some()
    }
}

/// A built-in profile never needs a vault file.
pub fn builtin(name: &str) -> Option<LayoutSettings> {
    match name {
        DEFAULT_PROFILE => Some(LayoutSettings::default()),
        ATS_SAFE_PROFILE => Some(LayoutSettings {
            show_link_marks: false,
            ..LayoutSettings::default()
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtins_exist_without_a_file_and_can_be_overridden() {
        let mut catalog = ProfileCatalog::default();
        assert_eq!(
            catalog.get(DEFAULT_PROFILE),
            Some(LayoutSettings::default())
        );
        assert!(!catalog.get(ATS_SAFE_PROFILE).unwrap().show_link_marks);

        let changed = LayoutSettings {
            text_scale_pct: 93,
            ..LayoutSettings::default()
        };
        catalog.upsert(ATS_SAFE_PROFILE, changed);
        assert_eq!(catalog.get(ATS_SAFE_PROFILE), Some(changed));
        assert_eq!(
            catalog
                .names()
                .iter()
                .filter(|name| name.as_str() == ATS_SAFE_PROFILE)
                .count(),
            1
        );
    }

    #[test]
    fn a_missing_profile_falls_back_without_discarding_the_document_layout() {
        let own = LayoutSettings {
            text_scale_pct: 111,
            ..LayoutSettings::default()
        };
        let catalog = ProfileCatalog::default();
        assert_eq!(catalog.resolve(Some("Renamed elsewhere"), own), own);
    }

    #[test]
    fn catalog_toml_contains_only_vault_profiles() {
        let mut catalog = ProfileCatalog::default();
        catalog.upsert("Compact", LayoutSettings::default());
        let text = toml::to_string_pretty(&catalog).expect("serialize catalog");
        assert!(text.contains("name = \"Compact\""));
        assert!(!text.contains("ATS-safe"));
        let back: ProfileCatalog = toml::from_str(&text).expect("parse catalog");
        assert_eq!(back, catalog);
    }
}
