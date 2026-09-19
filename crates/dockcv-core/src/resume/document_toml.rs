//! Compatibility boundary for DockCV's human-readable document format.
//!
//! Both the desktop vault and the browser engine accept the same TOML. Schema
//! migrations therefore belong beside the model, not in either host: otherwise
//! a document can open in one DockCV and fail in another.

use std::collections::{BTreeMap, BTreeSet};

use toml::Value;

use super::model::ResumeDoc;

/// Parse a DockCV document, applying every forward migration first.
///
/// Call this at product boundaries instead of deserializing [`ResumeDoc`]
/// directly. Serde remains useful for current-shape round-trip tests, while
/// this function is the compatibility promise made to files from old builds.
pub fn parse_document_toml(text: &str) -> Result<ResumeDoc, toml::de::Error> {
    let mut value: Value = toml::from_str(text)?;
    pre_variant_ids::migrate(&mut value);
    let mut doc: ResumeDoc = value.try_into()?;
    doc.reconcile_presets();
    Ok(doc)
}

/// Documents written before 0.4.0 pinned variants by display name and gave
/// variants no stable identity. The typed model cannot represent those pins,
/// so this migration runs over the generic TOML value before deserialization.
mod pre_variant_ids {
    use super::*;

    /// Which table each built-in `SectionKind` keeps its variants in.
    /// `Organizations` is stored under JSON Resume's historical `volunteer`.
    pub(super) const BUILT_INS: [(&str, &str); 6] = [
        ("Profile", "profile"),
        ("Work", "work"),
        ("Education", "education"),
        ("Skills", "skills"),
        ("Certificates", "certificates"),
        ("Organizations", "volunteer"),
    ];

    /// A variant name as an old file spelled it, against the id it now has.
    type Names = BTreeMap<String, i64>;

    pub(super) fn migrate(doc: &mut Value) {
        let mut known: Vec<(Value, Names)> = Vec::new();

        for (kind, field) in BUILT_INS {
            if let Some(section) = doc.get_mut(field) {
                known.push((
                    Value::String(kind.to_string()),
                    number_the_variants(section),
                ));
            }
        }
        for (id, section) in custom_sections(doc) {
            let mut table = toml::map::Map::new();
            table.insert("Custom".to_string(), Value::Integer(id));
            known.push((Value::Table(table), number_the_variants(section)));
        }

        resolve_pins(doc, &known);
    }

    /// Every custom section's id paired with its `content` table.
    fn custom_sections(doc: &mut Value) -> Vec<(i64, &mut Value)> {
        let Some(sections) = doc.get_mut("custom_sections").and_then(Value::as_array_mut) else {
            return Vec::new();
        };
        sections
            .iter_mut()
            .filter_map(|section| {
                let id = section.get("id")?.as_integer()?;
                Some((id, section.get_mut("content")?))
            })
            .collect()
    }

    /// Ensure every variant has one positive, unique id and return the legacy
    /// name lookup. Existing valid ids are preserved; a zero, duplicate or
    /// missing id is assigned above every valid id already present.
    fn number_the_variants(section: &mut Value) -> Names {
        let stored_next = section
            .get("next_id")
            .and_then(Value::as_integer)
            .unwrap_or(1);
        let Some(variants) = section.get_mut("variants").and_then(Value::as_array_mut) else {
            return Names::new();
        };

        let mut used = BTreeSet::new();
        let mut next = variants
            .iter()
            .filter_map(|variant| variant.get("id").and_then(Value::as_integer))
            .filter(|id| *id > 0)
            .max()
            .unwrap_or(0)
            + 1;
        next = next.max(stored_next);

        let mut names = Names::new();
        for variant in variants.iter_mut() {
            let existing = variant.get("id").and_then(Value::as_integer);
            let id = match existing {
                Some(id) if id > 0 && used.insert(id) => id,
                _ => {
                    while used.contains(&next) || next <= 0 {
                        next += 1;
                    }
                    let id = next;
                    next += 1;
                    used.insert(id);
                    if let Some(table) = variant.as_table_mut() {
                        table.insert("id".to_string(), Value::Integer(id));
                    }
                    id
                }
            };
            next = next.max(id + 1);
            if let Some(name) = variant.get("name").and_then(Value::as_str) {
                // The old name lookup also selected the first duplicate.
                names.entry(name.to_string()).or_insert(id);
            }
        }

        if let Some(table) = section.as_table_mut() {
            table.insert("next_id".to_string(), Value::Integer(next));
        }
        names
    }

    /// Turn every `["Work", "Infra-heavy"]` pin into `["Work", 2]`.
    /// Unknown names become id 0: an intentionally unresolved pin that the
    /// matrix can report and `apply_preset` handles deterministically.
    fn resolve_pins(doc: &mut Value, known: &[(Value, Names)]) {
        let Some(presets) = doc.get_mut("presets").and_then(Value::as_array_mut) else {
            return;
        };
        for preset in presets.iter_mut() {
            let Some(selection) = preset.get_mut("selection").and_then(Value::as_array_mut) else {
                continue;
            };
            for pin in selection.iter_mut() {
                let Some(pair) = pin.as_array_mut() else {
                    continue;
                };
                let (Some(section), Some(name)) = (pair.first().cloned(), pair.get(1)) else {
                    continue;
                };
                let Some(name) = name.as_str().map(str::to_string) else {
                    continue; // already an id
                };
                let id = known
                    .iter()
                    .find(|(kind, _)| *kind == section)
                    .and_then(|(_, names)| names.get(&name).copied())
                    .unwrap_or(0);
                pair[1] = Value::Integer(id);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::resume::model::{Preset, Resume, SectionKind};

    /// Convert a current document value into the exact pre-0.4 shape: variant
    /// ids/counters removed and preset ids replaced by names.
    fn downgrade_to_names(mut value: Value) -> String {
        let mut known: Vec<(Value, BTreeMap<i64, String>)> = Vec::new();

        for (kind, field) in pre_variant_ids::BUILT_INS {
            if let Some(section) = value.get_mut(field) {
                known.push((Value::String(kind.to_string()), remove_variant_ids(section)));
            }
        }
        if let Some(sections) = value
            .get_mut("custom_sections")
            .and_then(Value::as_array_mut)
        {
            for section in sections {
                let id = section.get("id").and_then(Value::as_integer).unwrap();
                let mut kind = toml::map::Map::new();
                kind.insert("Custom".to_string(), Value::Integer(id));
                if let Some(content) = section.get_mut("content") {
                    known.push((Value::Table(kind), remove_variant_ids(content)));
                }
            }
        }

        if let Some(presets) = value.get_mut("presets").and_then(Value::as_array_mut) {
            for preset in presets {
                let Some(selection) = preset.get_mut("selection").and_then(Value::as_array_mut)
                else {
                    continue;
                };
                for pin in selection {
                    let pair = pin.as_array_mut().unwrap();
                    let section = pair[0].clone();
                    let id = pair[1].as_integer().unwrap();
                    let name = known
                        .iter()
                        .find(|(kind, _)| *kind == section)
                        .and_then(|(_, names)| names.get(&id))
                        .unwrap();
                    pair[1] = Value::String(name.clone());
                }
            }
        }

        toml::to_string_pretty(&value).unwrap()
    }

    fn remove_variant_ids(section: &mut Value) -> BTreeMap<i64, String> {
        section.as_table_mut().unwrap().remove("next_id");
        let variants = section
            .get_mut("variants")
            .and_then(Value::as_array_mut)
            .unwrap();
        variants
            .iter_mut()
            .map(|variant| {
                let table = variant.as_table_mut().unwrap();
                let id = table.remove("id").unwrap().as_integer().unwrap();
                let name = table.get("name").unwrap().as_str().unwrap().to_string();
                (id, name)
            })
            .collect()
    }

    fn legacy_document() -> String {
        let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
        doc.add_variant(SectionKind::Work);
        doc.work.active_name_mut().clone_from(&"Infra-heavy".into());
        let custom = doc.add_custom_section("Languages");
        doc.add_variant(SectionKind::Custom(custom));
        doc.variant_name_mut(SectionKind::Custom(custom))
            .unwrap()
            .clone_from(&"German".into());
        doc.add_preset("Platform role");

        let value: Value = toml::from_str(&toml::to_string_pretty(&doc).unwrap()).unwrap();
        downgrade_to_names(value)
    }

    #[test]
    fn a_pre_040_document_migrates_names_and_becomes_total() {
        let text = legacy_document();
        assert!(!text.contains("next_id"), "{text}");
        let legacy: Value = toml::from_str(&text).unwrap();
        let work_pin = legacy["presets"][0]["selection"]
            .as_array()
            .unwrap()
            .iter()
            .find_map(|pin| {
                let pair = pin.as_array()?;
                (pair[0].as_str() == Some("Work"))
                    .then(|| pair[1].as_str())
                    .flatten()
            });
        assert_eq!(work_pin, Some("Infra-heavy"));

        let mut doc = parse_document_toml(&text).expect("old document opens");
        let custom = doc.custom_sections[0].id;
        assert_eq!(doc.presets[0].selection.len(), doc.sections().len());

        doc.set_active_variant(SectionKind::Work, 0);
        doc.set_active_variant(SectionKind::Custom(custom), 0);
        doc.apply_preset(0);
        assert_eq!(doc.work.active_name(), "Infra-heavy");
        assert_eq!(doc.variant_name(SectionKind::Custom(custom)), "German");
        assert!(doc.unresolved_pins(0).is_empty());
    }

    #[test]
    fn an_unknown_legacy_name_stays_visibly_unresolved() {
        let mut legacy: Value = toml::from_str(&legacy_document()).unwrap();
        let selection = legacy["presets"][0]["selection"].as_array_mut().unwrap();
        let work = selection
            .iter_mut()
            .find(|pin| pin.as_array().unwrap()[0].as_str() == Some("Work"))
            .unwrap()
            .as_array_mut()
            .unwrap();
        work[1] = Value::String("Deleted cut".into());
        let mut doc = parse_document_toml(&toml::to_string_pretty(&legacy).unwrap())
            .expect("old document opens");

        assert!(doc.unresolved_pins(0).contains(&SectionKind::Work));
        doc.set_active_variant(SectionKind::Work, 1);
        doc.apply_preset(0);
        assert_eq!(
            doc.work.active, 0,
            "a broken pin has one deterministic fallback"
        );
    }

    #[test]
    fn duplicate_or_zero_ids_are_repaired_without_repointing_the_first() {
        let mut doc = ResumeDoc::from_resume(Resume::default(), "Base");
        doc.add_variant(SectionKind::Work);
        let first = doc.work.variants[0].id;
        doc.presets.push(Preset {
            name: "Base work".into(),
            selection: vec![(SectionKind::Work, first)],
            hidden: Vec::new(),
            order: Vec::new(),
            titles: Vec::new(),
        });
        let mut value: Value = toml::from_str(&toml::to_string_pretty(&doc).unwrap()).unwrap();
        let variants = value["work"]["variants"].as_array_mut().unwrap();
        variants[1]
            .as_table_mut()
            .unwrap()
            .insert("id".to_string(), Value::Integer(first.as_u32() as i64));

        let parsed = parse_document_toml(&toml::to_string_pretty(&value).unwrap()).unwrap();
        let ids = parsed.work.ids();
        assert_ne!(ids[0], ids[1]);
        assert_eq!(
            parsed.presets[0].variant_for(SectionKind::Work),
            Some(ids[0])
        );
        assert!(parsed.work.next_id > ids[1].as_u32());
    }
}
