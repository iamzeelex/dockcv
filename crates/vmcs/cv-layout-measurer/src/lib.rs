wit_bindgen::generate!({
    world: "layout-measurer-vmc",
});

use exports::cv::layout_measurer::measurer::*;
use dockcv_core::resume::parse_any;
use dockcv_core::resume::template;
use dockcv_core::typst_engine::TypstEngine;

struct LayoutMeasurerVmc;

fn resolve_typst(format_name: &str, content: &str) -> Result<String, LayoutError> {
    if format_name.eq_ignore_ascii_case("typst") {
        return Ok(content.to_string());
    }

    let doc = parse_any(content).map_err(LayoutError::ParseFailure)?;
    Ok(template::generate_for(&doc))
}

impl Guest for LayoutMeasurerVmc {
    fn measure_layout(
        format_name: String,
        content: String,
        target_pages: u32,
    ) -> Result<MeasurementResult, LayoutError> {
        let typst_source = resolve_typst(&format_name, &content)?;
        let engine = TypstEngine::new(&typst_source);

        let geometry = engine
            .measure()
            .map_err(LayoutError::CompilationFailure)?;

        let target_pages = target_pages.max(1);
        let fits_target = (geometry.page_count as u32) <= target_pages;
        let line_advance = geometry.line_advance_pt.unwrap_or(14.0);

        let overflow_lines_est = if geometry.page_count as u32 > target_pages {
            ((geometry.overflow_pt / line_advance).ceil() as i32).max(1)
        } else {
            let slack = (geometry.column_pt - (geometry.last_page_used_pt - geometry.last_page_content_top_pt)).max(0.0);
            -((slack / line_advance).floor() as i32)
        };

        let metric = PageMetric {
            page_count: geometry.page_count as u32,
            target_pages,
            fits_target,
            overflow_pt: geometry.overflow_pt,
            overflow_lines_est,
            page_height_pt: geometry.page_height_pt,
            last_page_used_pt: geometry.last_page_used_pt,
            column_pt: geometry.column_pt,
            line_advance_pt: line_advance,
        };

        Ok(MeasurementResult {
            metric,
            typst_source,
            diagnostics: Vec::new(),
        })
    }

    fn render_pdf(
        format_name: String,
        content: String,
        target_pages: u32,
    ) -> Result<RenderResult, LayoutError> {
        let typst_source = resolve_typst(&format_name, &content)?;
        let engine = TypstEngine::new(&typst_source);

        let geometry = engine
            .measure()
            .map_err(LayoutError::CompilationFailure)?;

        let pdf_bytes = engine
            .compile_to_pdf()
            .map_err(LayoutError::CompilationFailure)?;

        let target_pages = target_pages.max(1);
        let fits_target = (geometry.page_count as u32) <= target_pages;
        let line_advance = geometry.line_advance_pt.unwrap_or(14.0);

        let overflow_lines_est = if geometry.page_count as u32 > target_pages {
            ((geometry.overflow_pt / line_advance).ceil() as i32).max(1)
        } else {
            let slack = (geometry.column_pt - (geometry.last_page_used_pt - geometry.last_page_content_top_pt)).max(0.0);
            -((slack / line_advance).floor() as i32)
        };

        let metric = PageMetric {
            page_count: geometry.page_count as u32,
            target_pages,
            fits_target,
            overflow_pt: geometry.overflow_pt,
            overflow_lines_est,
            page_height_pt: geometry.page_height_pt,
            last_page_used_pt: geometry.last_page_used_pt,
            column_pt: geometry.column_pt,
            line_advance_pt: line_advance,
        };

        Ok(RenderResult {
            metric,
            pdf_bytes,
            diagnostics: Vec::new(),
        })
    }

    fn render_svg(
        format_name: String,
        content: String,
    ) -> Result<Vec<String>, LayoutError> {
        let typst_source = resolve_typst(&format_name, &content)?;
        let engine = TypstEngine::new(&typst_source);

        engine
            .compile_to_svg()
            .map_err(LayoutError::CompilationFailure)
    }
}

export!(LayoutMeasurerVmc);

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_TOML: &str = r#"
[basics]
name = "Jane Doe"
label = "Principal Distributed Systems Engineer"
email = "jane@example.com"
phone = "+1 555 019 2834"
summary = "Specialist in high-throughput sovereign computation engines, Wasm component architectures, and deterministic state machines."

[[work]]
name = "Acme Corp"
position = "Lead Architect"
startDate = "2021-01"
summary = "Designed sovereign compute sandboxes."
highlights = [
    "Delivered zero-trust execution environment",
    "Optimized data pipeline throughput by 40%"
]

[[skills]]
name = "Systems"
keywords = ["Rust", "Wasm", "Component Model", "Linux"]
"#;

    #[test]
    fn test_measure_layout_toml() {
        let result = LayoutMeasurerVmc::measure_layout(
            "toml".to_string(),
            SAMPLE_TOML.to_string(),
            1,
        );
        assert!(result.is_ok(), "measure_layout failed: {:?}", result.err());
        let m = result.unwrap();
        assert_eq!(m.metric.page_count, 1);
        assert!(m.metric.fits_target);
        assert_eq!(m.metric.overflow_pt, 0.0);
        assert!(m.metric.overflow_lines_est <= 0); // has slack
        assert!(!m.typst_source.is_empty());
    }

    #[test]
    fn test_render_pdf_toml() {
        let result = LayoutMeasurerVmc::render_pdf(
            "toml".to_string(),
            SAMPLE_TOML.to_string(),
            1,
        );
        assert!(result.is_ok(), "render_pdf failed: {:?}", result.err());
        let res = result.unwrap();
        assert_eq!(res.metric.page_count, 1);
        assert!(!res.pdf_bytes.is_empty());
        // Verify PDF magic header %PDF-
        assert_eq!(&res.pdf_bytes[0..5], b"%PDF-");
    }

    #[test]
    fn test_render_svg_toml() {
        let result = LayoutMeasurerVmc::render_svg(
            "toml".to_string(),
            SAMPLE_TOML.to_string(),
        );
        assert!(result.is_ok(), "render_svg failed: {:?}", result.err());
        let svgs = result.unwrap();
        assert_eq!(svgs.len(), 1);
        assert!(svgs[0].contains("<svg"));
    }
}
