# DockCV Sovereign VMC Suite for Kraton (v0.4.1)

This directory contains standalone, sovereign **Virtual Monadic Components (VMCs)** extracted from DockCV. Each module is compiled to the Wasm Component Model (`wasm32-wasip2`), adheres to Kraton WIT specifications, and performs deterministic computation without touching the host filesystem or OS clock.

---

## The Day-1 Flow: Candidate $\leftrightarrow$ Recruiter Workflow

```
[Raw Candidate CV: Text / Markdown / Typst]
                   │
                   ▼
       cv-structure-classifier (VMC)
      ┌─────────────────────────────┐
      │ • Extracts Contact Profile  │
      │ • Groups Logical Sections   │
      │ • Outputs TOML / JSON Doc   │
      └────────────┬────────────────┘
                   │
                   ▼
      LLM Agent (Semantic Reasoning)
      ┌─────────────────────────────┐
      │ • Aligns keywords with JD   │
      │ • Rewrites bullet points    │
      └────────────┬────────────────┘
                   │
         ┌─────────┴─────────┐
         ▼                   ▼
    cv-ats-linter    cv-layout-measurer (VMCs)
   ┌──────────────┐ ┌───────────────────────────┐
   │ • Date checks│ │ • Measures exact overflow │
   │ • Bullet lint│ │ • Est. lines over/under   │
   │ • Hidden text│ │ • Produces PDF / SVG      │
   └──────────────┘ └─────────────┬─────────────┘
                                  │
                                  ▼
                       cv-application-tracker (VMC)
                      ┌───────────────────────────┐
                      │ • Advances pipeline state │
                      │ • Tracks conversion funnel│
                      │ • Logs interview steps    │
                      └───────────────────────────┘
```

---

## Module Overview

| Module | World / Interface | Binary Size | Purpose |
|---|---|---|---|
| **`cv-structure-classifier`** | `structure-classifier-vmc` | ~1.9 MB | Recovers candidate profile, logical sections, experience, education, and skills from unstyled or imported text. |
| **`cv-ats-linter`** | `ats-linter-vmc` | ~1.6 MB | Validates headings, date chronologies, contact info presence, unrendered markup, and hidden zero-width text. |
| **`cv-layout-measurer`** | `layout-measurer-vmc` | ~32 MB | Measures page overflow (`overflow_pt`, `overflow_lines_est`), verifies 1-page fit, and compiles final PDF/SVG via embedded Typst engine. |
| **`cv-application-tracker`** | `application-tracker-vmc` | ~430 KB | State machine managing Kanban applications (`Wishlist` $\to$ `Applied` $\to$ `Interviewing` $\to$ `Offer`/`Closed`), dates, notes, and conversion funnels. |

---

## Building All VMCs

All components are configured for `wasm32-wasip2`:

```bash
# In crates/vmcs/cv-structure-classifier
cargo build --target wasm32-wasip2 --release

# In crates/vmcs/cv-ats-linter
cargo build --target wasm32-wasip2 --release

# In crates/vmcs/cv-layout-measurer
cargo build --target wasm32-wasip2 --release

# In crates/vmcs/cv-application-tracker
cargo build --target wasm32-wasip2 --release
```
