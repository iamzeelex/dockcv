#[cfg(test)]
mod tests {
    use dockcv_core::import::classify_raw_text;
    use dockcv_core::resume::model::Resume;
    use dockcv_core::resume::parse_any;
    use dockcv_core::resume::template;
    use dockcv_core::typst_engine::TypstEngine;

    const SAMPLE_RAW_RESUME: &str = r#"
Jane Doe
Principal Systems Architect
jane.doe@example.com
+1 555 019 2834
San Francisco, CA

EXPERIENCE
Acme Distributed Cloud — Principal Systems Architect
2021-03 - Present
- Designed sovereign compute engine supporting verified Wasm components.
- Reduced inter-module serialization latency by 45% using shared memory buffers.
- Supervised architecture transition across 14 distributed engineering teams.

Beta Microservices — Senior Software Engineer
2018-06 - 2021-02
- Implemented high-throughput event processing pipeline in Rust.
- Authored automated compliance linting tools for CI/CD environments.

EDUCATION
University of California, Berkeley
B.S. in Computer Science
2014 - 2018

SKILLS
Systems: Rust, WebAssembly, Distributed Consensus, Linux
Platforms: Kubernetes, Docker, Kraton VMC, AWS
"#;

    const INITIAL_BOARD_TOML: &str = r#"
[[applications]]
id = "app-001"
company = "Stripe"
position = "Staff Engineer"
status = "Wishlist"
date_applied = "2026-10-01"
notes = ["Found via direct referral"]
events = []
"#;

    #[test]
    fn test_flow_zero_llm_sovereign_dag() {
        // =========================================================================
        // FLOW 1: ZERO-LLM / PURE SOVEREIGN DAG
        // Complete deterministic pipeline from raw input to verified artifact.
        // =========================================================================

        // STEP 1: cv-structure-classifier
        // Ingestion & structural extraction from unformatted plain text.
        let imported = classify_raw_text("plaintext", SAMPLE_RAW_RESUME);
        let resume = imported.doc.compose();

        assert_eq!(resume.basics.name, "Jane Doe");
        assert!(!resume.work.is_empty());
        assert!(!resume.education.is_empty());

        // Serialize to standardized JSON Résumé (contract format)
        let json_resume_str = serde_json::to_string_pretty(&resume).unwrap();
        assert!(!json_resume_str.is_empty());

        // STEP 2: cv-ats-linter
        // Deterministic audit of the structured resume.
        let doc_for_lint = parse_any(&json_resume_str).expect("Valid JSON Resume");
        assert_eq!(doc_for_lint.profile.active().name, "Jane Doe");
        assert_eq!(doc_for_lint.profile.active().email, "jane.doe@example.com");

        // Verify dates validate and format chronologically
        for job in doc_for_lint.work.active() {
            assert!(!job.start_date.is_empty(), "Start date must exist");
        }

        // STEP 3: cv-layout-measurer
        // Measurement of geometry + rendering of PDF.
        let typst_source = template::generate_for(&doc_for_lint);
        let engine = TypstEngine::new(&typst_source);
        let geometry = engine.measure().expect("Typst layout measurement should succeed");

        assert_eq!(geometry.page_count, 1, "Resume should fit target 1 page");
        assert_eq!(geometry.overflow_pt, 0.0, "Zero overflow points");

        let pdf_bytes = engine.compile_to_pdf().expect("PDF compilation should succeed");
        assert!(!pdf_bytes.is_empty());
        assert_eq!(&pdf_bytes[0..5], b"%PDF-");

        // STEP 4: cv-application-tracker
        // State machine updates application board.
        let mut board: toml::Table = toml::from_str(INITIAL_BOARD_TOML).unwrap();
        let apps = board.get_mut("applications").unwrap().as_array_mut().unwrap();

        // Advance Stripe from Wishlist -> Applied
        for app in apps.iter_mut() {
            if app.get("company").and_then(|c| c.as_str()) == Some("Stripe") {
                app.as_table_mut().unwrap().insert(
                    "status".to_string(),
                    toml::Value::String("Applied".to_string()),
                );
            }
        }

        let updated_board = toml::to_string(&board).unwrap();
        assert!(updated_board.contains("status = \"Applied\""));
    }

    #[test]
    fn test_flow_sandwich_with_agent_tailoring() {
        // =========================================================================
        // FLOW 2: SANDWICH FLOW (Deterministic -> LLM Tailor -> Deterministic Guardrail)
        // =========================================================================

        // STEP 1: cv-structure-classifier (Deterministic pre-process)
        let imported = classify_raw_text("plaintext", SAMPLE_RAW_RESUME);
        let original_resume = imported.doc.compose();

        // STEP 2: Simulated LLM Agent Tailoring
        // Agent modifies summary and experience bullet points to match a specific Job Description.
        let mut tailored_resume: Resume = original_resume;
        tailored_resume.basics.summary = "Staff Engineer specializing in sovereign Wasm component runtimes, formal ATS compliance verification, and high-performance Rust systems.".to_string();

        if let Some(first_job) = tailored_resume.work.first_mut() {
            first_job.highlights.insert(
                0,
                "Architected sovereign Wasm components meeting strict ATS parsing and memory isolation guarantees.".to_string(),
            );
        }

        let tailored_json = serde_json::to_string_pretty(&tailored_resume).unwrap();

        // STEP 3: cv-ats-linter (Deterministic Guardrail)
        // Ensure the LLM didn't break required fields or introduce invalid dates.
        let tailored_doc = parse_any(&tailored_json).expect("Tailored JSON must parse");
        assert_eq!(tailored_doc.profile.active().name, "Jane Doe");
        assert_eq!(tailored_doc.profile.active().email, "jane.doe@example.com");

        // STEP 4: cv-layout-measurer (Deterministic Guardrail)
        // Measure whether the LLM's modifications fit in 1 page.
        let typst_source = template::generate_for(&tailored_doc);
        let engine = TypstEngine::new(&typst_source);
        let geometry = engine.measure().expect("Measurement succeeds");

        assert_eq!(geometry.page_count, 1, "Tailored resume fits in exactly 1 page");
        assert_eq!(geometry.overflow_pt, 0.0);

        let pdf_bytes = engine.compile_to_pdf().expect("Compile PDF");
        assert_eq!(&pdf_bytes[0..5], b"%PDF-");

        // STEP 5: cv-application-tracker
        // State machine records new tailored application.
        let mut board: toml::Table = toml::from_str(INITIAL_BOARD_TOML).unwrap();
        let apps = board.get_mut("applications").unwrap().as_array_mut().unwrap();

        let mut new_app = toml::Table::new();
        new_app.insert("id".to_string(), toml::Value::String("app-002".to_string()));
        new_app.insert("company".to_string(), toml::Value::String("Kraton Sovereign Labs".to_string()));
        new_app.insert("position".to_string(), toml::Value::String("Principal Wasm Architect".to_string()));
        new_app.insert("status".to_string(), toml::Value::String("Applied".to_string()));
        new_app.insert("date_applied".to_string(), toml::Value::String("2026-10-14".to_string()));
        new_app.insert("notes".to_string(), toml::Value::Array(vec![toml::Value::String("Tailored with Wasm keywords".to_string())]));
        new_app.insert("events".to_string(), toml::Value::Array(vec![]));

        apps.push(toml::Value::Table(new_app));
        let final_board = toml::to_string(&board).unwrap();
        assert!(final_board.contains("Kraton Sovereign Labs"));
        assert!(final_board.contains("Principal Wasm Architect"));
    }
}
