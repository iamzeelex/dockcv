//! Data-driven multilingual section taxonomy and heading verification.
//!
//! Loads `assets/taxonomy.toml` at compile-time via `include_str!` and exposes
//! fast lookup for known headings across 11+ languages.
//! Eliminates hardcoded heading tables from ATS verification and importer alike.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::OnceLock;

use super::model::SectionKind;

static TAXONOMY_TOML: &str = include_str!("../../../../assets/taxonomy.toml");
static SECTION_TAXONOMY: OnceLock<SectionTaxonomy> = OnceLock::new();

pub type RawTaxonomy = BTreeMap<String, BTreeMap<String, Vec<String>>>;

pub struct SectionTaxonomy {
    /// Maps each built-in SectionKind to the set of all normalized (lowercase, trimmed)
    /// titles across all supported languages in the taxonomy.
    known_headings: HashMap<SectionKind, HashSet<String>>,
    /// Maps (SectionKind, language_code) to the preferred display heading.
    preferred: HashMap<(SectionKind, &'static str), &'static str>,
    /// Full raw taxonomy by family name.
    raw: RawTaxonomy,
}

impl SectionTaxonomy {
    /// Global singleton instance of the taxonomy.
    pub fn global() -> &'static Self {
        SECTION_TAXONOMY.get_or_init(Self::init)
    }

    fn init() -> Self {
        let raw: RawTaxonomy = toml::from_str(TAXONOMY_TOML)
            .expect("embedded assets/taxonomy.toml must be valid TOML");

        let mut known_headings = HashMap::new();

        let family_mapping = [
            ("summary", SectionKind::Profile),
            ("work", SectionKind::Work),
            ("education", SectionKind::Education),
            ("skills", SectionKind::Skills),
            ("certificates", SectionKind::Certificates),
            ("volunteer", SectionKind::Organizations),
        ];

        for (family, kind) in family_mapping {
            let mut words_set = HashSet::new();
            if let Some(lang_map) = raw.get(family) {
                for words in lang_map.values() {
                    for word in words {
                        let clean = normalize_heading(word);
                        if !clean.is_empty() {
                            words_set.insert(clean);
                        }
                    }
                }
            }
            known_headings.insert(kind, words_set);
        }

        let mut preferred = HashMap::new();
        // Preferred headings for languages DockCV emits:
        preferred.insert((SectionKind::Profile, "en"), "Profile");
        preferred.insert((SectionKind::Work, "en"), "Work Experience");
        preferred.insert((SectionKind::Education, "en"), "Education");
        preferred.insert((SectionKind::Skills, "en"), "Skills");
        preferred.insert((SectionKind::Certificates, "en"), "Certifications");
        preferred.insert((SectionKind::Organizations, "en"), "Organizations");

        preferred.insert((SectionKind::Profile, "de"), "Profil");
        preferred.insert((SectionKind::Work, "de"), "Berufserfahrung");
        preferred.insert((SectionKind::Education, "de"), "Ausbildung");
        preferred.insert((SectionKind::Skills, "de"), "Kenntnisse");
        preferred.insert((SectionKind::Certificates, "de"), "Zertifikate");
        preferred.insert((SectionKind::Organizations, "de"), "Ehrenamt");

        preferred.insert((SectionKind::Profile, "ru"), "О себе");
        preferred.insert((SectionKind::Work, "ru"), "Опыт работы");
        preferred.insert((SectionKind::Education, "ru"), "Образование");
        preferred.insert((SectionKind::Skills, "ru"), "Навыки");
        preferred.insert((SectionKind::Certificates, "ru"), "Сертификаты");
        preferred.insert((SectionKind::Organizations, "ru"), "Волонтерство");

        preferred.insert((SectionKind::Profile, "uk"), "Про себе");
        preferred.insert((SectionKind::Work, "uk"), "Досвід роботи");
        preferred.insert((SectionKind::Education, "uk"), "Освіта");
        preferred.insert((SectionKind::Skills, "uk"), "Навички");
        preferred.insert((SectionKind::Certificates, "uk"), "Сертифікати");
        preferred.insert((SectionKind::Organizations, "uk"), "Громадська діяльність");

        preferred.insert((SectionKind::Profile, "fr"), "Profil");
        preferred.insert((SectionKind::Work, "fr"), "Expérience professionnelle");
        preferred.insert((SectionKind::Education, "fr"), "Formation");
        preferred.insert((SectionKind::Skills, "fr"), "Compétences");
        preferred.insert((SectionKind::Certificates, "fr"), "Certifications");
        preferred.insert((SectionKind::Organizations, "fr"), "Bénévolat");

        preferred.insert((SectionKind::Profile, "es"), "Perfil");
        preferred.insert((SectionKind::Work, "es"), "Experiencia laboral");
        preferred.insert((SectionKind::Education, "es"), "Educación");
        preferred.insert((SectionKind::Skills, "es"), "Habilidades");
        preferred.insert((SectionKind::Certificates, "es"), "Certificaciones");
        preferred.insert((SectionKind::Organizations, "es"), "Voluntariado");

        Self {
            known_headings,
            preferred,
            raw,
        }
    }

    /// Check if a heading is recognized for a given section kind across any language in the taxonomy.
    pub fn is_known_heading(&self, kind: SectionKind, heading: &str) -> bool {
        let clean = normalize_heading(heading);
        self.known_headings
            .get(&kind)
            .is_some_and(|set| set.contains(&clean))
    }

    /// Get preferred heading recommendation for a section kind and language code.
    pub fn preferred_heading(&self, kind: SectionKind, lang: &str) -> Option<&'static str> {
        self.preferred.get(&(kind, lang)).copied()
    }

    /// Access the raw parsed taxonomy.
    pub fn raw(&self) -> &RawTaxonomy {
        &self.raw
    }
}

pub fn normalize_heading(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut space = false;
    for ch in title.chars() {
        if ch.is_alphanumeric() {
            if space {
                out.push(' ');
                space = false;
            }
            out.extend(ch.to_lowercase());
        } else if !out.is_empty() {
            space = true;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taxonomy_recognizes_multilingual_headings() {
        let tax = SectionTaxonomy::global();
        // English
        assert!(tax.is_known_heading(SectionKind::Work, "Work Experience"));
        assert!(tax.is_known_heading(SectionKind::Work, "experience"));
        // German
        assert!(tax.is_known_heading(SectionKind::Work, "Berufserfahrung"));
        assert!(tax.is_known_heading(SectionKind::Education, "Ausbildung"));
        // Russian
        assert!(tax.is_known_heading(SectionKind::Work, "Опыт работы"));
        assert!(tax.is_known_heading(SectionKind::Skills, "Навыки"));
        // Ukrainian
        assert!(tax.is_known_heading(SectionKind::Work, "Досвід роботи"));
        assert!(tax.is_known_heading(SectionKind::Profile, "Про себе"));
        // French
        assert!(tax.is_known_heading(SectionKind::Work, "Expérience professionnelle"));
        assert!(tax.is_known_heading(SectionKind::Education, "Formation"));
        // Spanish
        assert!(tax.is_known_heading(SectionKind::Work, "Experiencia laboral"));
    }
}
