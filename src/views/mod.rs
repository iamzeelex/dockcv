//! UI views layer.

mod applications;
mod applications_analytics;
mod applications_card;
mod applications_data;
mod applications_drag;
mod applications_list;
mod applications_menu;
mod applications_funnel;
mod applications_pin;
mod applications_snapshot;
mod confirm;
mod diary;
mod diary_capture;
mod diary_use;
mod gallery;
pub mod import_flow;
mod library;
mod library_edit;
mod library_usage;
mod preset_matrix;
mod root;
mod root_custom_sections;
mod root_highlights;
mod root_layout_rail;
mod root_overlays;
mod root_section_drag;
mod root_section_layout;
mod root_section_rename;
mod root_section_variants;
mod root_sidebar;
mod root_dates;
mod root_undo;
pub mod save_status;
pub mod settings_window;
mod setup;
mod shell;
mod sidebar;
mod vault_cache;
mod welcome;

pub use root::{init_keybindings, EditorEvent, ExportPdf, Root};
pub use shell::Shell;

#[cfg(test)]
mod field_coverage {
    /// `Education::highlights` was in the model and on the page and in no view,
    /// so an imported line printed on the CV and could be neither found nor
    /// deleted (E-42). The hole was invisible because nothing connected the two
    /// halves: `addressable` said the field existed, the sidebar never asked.
    ///
    /// This reads the view sources and checks each variant is mentioned. It is
    /// a coarse test — a mention in a comment would satisfy it — but it is the
    /// only one that fails when a field is added to the model and forgotten in
    /// the editor, which is the mistake that actually happened.
    #[test]
    fn every_addressable_field_is_drawn_by_some_view() {
        const VIEWS: &[&str] = &[
            include_str!("root_sidebar.rs"),
            include_str!("root_custom_sections.rs"),
            include_str!("root_section_variants.rs"),
            include_str!("root_section_rename.rs"),
            include_str!("preset_matrix.rs"),
            include_str!("root.rs"),
            include_str!("shell.rs"),
        ];
        // The contract, spelled out: adding a variant without adding it here
        // fails, which is the moment to confirm a view draws it.
        const VARIANTS: [&str; 43] = [
            "Name", "Label", "Summary", "Email", "Phone", "Location", "Url",
            "ProfileNetwork", "ProfileUsername", "ProfileUrl",
            "WorkName", "WorkPosition", "WorkLocation", "WorkStart", "WorkEnd",
            "WorkSummary", "WorkHighlight",
            "EduStudyType", "EduInstitution", "EduStart", "EduEnd", "EduUrl",
            "EduHighlight",
            "SkillName", "SkillKeyword",
            "CertName", "CertIssuer", "CertDate", "CertUrl",
            "VolOrg", "VolPosition", "VolStart", "VolEnd", "VolHighlight",
            "CustomSectionTitle", "CustomEntryTitle", "CustomEntrySubtitle",
            "CustomEntryStart", "CustomEntryEnd", "CustomEntryUrl",
            "CustomEntryHighlight",
            "VariantName", "PresetName",
            // (was: `PresetName` deliberately absent. It **is** addressable and no
            // view draws it: a preset is created as `Preset 2` in `root.rs` and
            // can never be renamed. Unlike E-42 the value is at least *visible*
            // — it labels a chip in the gallery and a column in the matrix — so
            // G-14 closed it: the Preset Matrix's left pill now carries the
            // same pen the editor puts on a section header, and the commit
            // writes through `FieldId::PresetName` rather than past it.)
        ];

        for variant in VARIANTS {
            let needle = format!("FieldId::{variant}");
            assert!(
                VIEWS.iter().any(|src| src.contains(&needle)),
                "{needle} is addressable and no view draws it"
            );
        }
    }
}
