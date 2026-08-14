//! Application theme adapter re-exporting from `dockcv-ui-components`.

pub use dockcv_ui_components::theme::{set_theme_mode, ActiveTheme, Theme, ThemeMode};
// Neither `SANS` nor `SERIF` is named directly by views any more — `TextStyle`
// picks the family per role (E-7). `MONO` survives at two sites that set a
// family without a full role, and should follow them out.
pub use dockcv_ui_components::typography::{StyledText, TextStyle, MONO};
