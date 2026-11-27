# cv-layout-measurer

Deterministic page geometry, line overflow calculation, and Typst PDF/SVG rendering for resume documents.

## Capabilities

1. **`measure-layout(format_name, content, target_pages)`**:
   - Parses resume (`toml`, `json`, or raw `typst`).
   - Typesets using DockCV's embedded Typst engine and Libertinus font.
   - Calculates exact layout geometry:
     - `page_count`: Number of laid out pages.
     - `target_pages`: Requested target page budget (e.g. 1 page).
     - `fits_target`: Whether document meets target budget.
     - `overflow_pt`: Extent of content spilling beyond target page.
     - `overflow_lines_est`: Positive integer for lines over budget (e.g. `+3` lines over), or negative integer for headroom slack remaining (e.g. `-4` lines slack).
     - `line_advance_pt`: Measured baseline distance between lines.
     - `column_pt`: Usable height of the primary column.

2. **`render-pdf(format_name, content, target_pages)`**:
   - Typesets and renders the final ATS-compliant PDF document directly to bytes.
   - Returns both the PDF bytes and the layout metrics.

3. **`render-svg(format_name, content)`**:
   - Typesets and renders vector SVG strings (one per page) for real-time visual inspection in LLM chats or web UI viewers.

## Target

- Target: `wasm32-wasip2`
- Interface: Component Model (`wit/world.wit`)
