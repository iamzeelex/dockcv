//! cv-structure-classifier — raw text classifier into structured resume models (Kraton VMC).

wit_bindgen::generate!({
    path: "wit",
    world: "structure-classifier-vmc",
});

use exports::cv::structure_classifier::classifier::{
    ClassifyError, ClassifyRequest, ClassifyResult, Guest, UnplacedItem,
};

use dockcv_core::import::classify_raw_text;

struct Classifier;

impl Guest for Classifier {
    fn classify_text(req: ClassifyRequest) -> Result<ClassifyResult, ClassifyError> {
        if req.raw_text.trim().is_empty() {
            return Err(ClassifyError::EmptyInput);
        }

        let imported = classify_raw_text(&req.format_name, &req.raw_text);

        let document_toml = toml::to_string(&imported.doc)
            .map_err(|e| ClassifyError::ClassificationFailed(format!("TOML export failed: {e}")))?;

        let document_json = serde_json::to_string(&imported.doc.compose())
            .map_err(|e| ClassifyError::ClassificationFailed(format!("JSON export failed: {e}")))?;

        let profile = imported.doc.profile.active();
        let candidate_name = profile.name.clone();
        let candidate_email = profile.email.clone();
        let candidate_phone = profile.phone.clone();

        let work_count = imported.doc.work.active().len() as u32;
        let education_count = imported.doc.education.active().len() as u32;
        let skills_count = imported.doc.skills.active().len() as u32;
        let section_count = imported.doc.sections().len() as u32;

        let unplaced = imported
            .unplaced
            .into_iter()
            .map(|u| UnplacedItem {
                formatted_line: u.line(),
                title: u.title,
                subtitle: u.subtitle,
                details: u.details,
            })
            .collect();

        Ok(ClassifyResult {
            document_toml,
            document_json,
            candidate_name,
            candidate_email,
            candidate_phone,
            section_count,
            work_count,
            education_count,
            skills_count,
            unplaced,
        })
    }
}

export!(Classifier);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_sample_text() {
        let raw = "Jane Doe\njane@example.com\n+1 555 123 4567\n\nEXPERIENCE\nSenior Rust Developer, Acme Corp\n2020 - 2024\n• Architected sovereign computing modules\n• Reduced memory usage by 40%\n\nEDUCATION\nBS Computer Science, Stanford University\n2016 - 2020\n\nSKILLS\nRust, WebAssembly, Typst, Distributed Systems";

        let result = Classifier::classify_text(ClassifyRequest {
            format_name: "Plain Text".into(),
            raw_text: raw.into(),
        })
        .expect("classification succeeds");

        assert_eq!(result.candidate_name, "Jane Doe");
        assert_eq!(result.candidate_email, "jane@example.com");
        assert!(result.work_count >= 1);
        assert!(result.education_count >= 1);
        assert!(result.skills_count >= 1);
        assert!(!result.document_json.is_empty());
        assert!(!result.document_toml.is_empty());
    }

    #[test]
    fn classify_empty_text_returns_error() {
        let result = Classifier::classify_text(ClassifyRequest {
            format_name: "PDF".into(),
            raw_text: "   ".into(),
        });

        assert!(matches!(result, Err(ClassifyError::EmptyInput)));
    }
}
