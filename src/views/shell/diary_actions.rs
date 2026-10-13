use gpui::{Context, Window};

use crate::resume::model::DiaryEntry;
use crate::vault;
use crate::views::save_status;

use super::Shell;

/// Split a `# tag` box into stored tags: `#` is decoration, separators are
/// whatever the user reached for. Deduplicated, because a tag applied twice to
/// one entry is a typo, not two facts.
pub(crate) fn parse_tags(raw: &str) -> Vec<String> {
    let mut tags: Vec<String> = Vec::new();
    for tag in raw
        .split([',', ' ', '\t'])
        .map(|t| t.trim().trim_start_matches('#').trim())
        .filter(|t| !t.is_empty())
    {
        let tag = tag.to_lowercase();
        if !tags.contains(&tag) {
            tags.push(tag);
        }
    }
    tags
}

pub(crate) fn remove_at<T>(items: &mut Vec<T>, index: usize) {
    if index < items.len() {
        items.remove(index);
    }
}

impl Shell {
    pub(crate) fn commit_diary_entry(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(field) = self.diary_draft.clone() else {
            return;
        };
        let text = field.read(cx).value(cx).trim().to_string();
        if text.is_empty() {
            return;
        }
        let tag_field = self.diary_tags.clone();
        let tags = tag_field
            .as_ref()
            .map(|f| parse_tags(&f.read(cx).value(cx)))
            .unwrap_or_default();

        if let Some(vault) = self.vault.clone() {
            let mut diary = vault::load_diary(&vault);
            diary.entries.insert(
                0,
                DiaryEntry {
                    date: vault::today_iso(),
                    text,
                    role: self.diary_role.clone(),
                    tags,
                    confidential: false,
                    used_in: Vec::new(),
                    // Typed straight into the Diary — no document was open.
                    source_doc: None,
                },
            );
            save_status::record(cx, "diary", vault::save_diary(&vault, &diary));
        }
        field.update(cx, |state, cx| state.seed("", window, cx));
        if let Some(tag_field) = tag_field {
            tag_field.update(cx, |state, cx| state.seed("", window, cx));
        }
        // The role deliberately survives the commit — see `diary_role`.
        cx.notify();
    }

    /// Flip an entry's confidential mark (US-36).
    ///
    /// A mark, not a redaction: the wording stays exactly where it is, in the
    /// diary, which is the point — you keep the record you need for a
    /// performance review, and what goes outward is a different sentence.
    pub(crate) fn toggle_diary_confidential(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let mut diary = vault::load_diary(&vault);
        let Some(entry) = diary.entries.get_mut(index) else {
            return;
        };
        entry.confidential = !entry.confidential;
        save_status::record(cx, "diary", vault::save_diary(&vault, &diary));
        cx.notify();
    }

    pub(crate) fn delete_diary_entry(&mut self, index: usize, cx: &mut Context<Self>) {
        let Some(vault) = self.vault.clone() else {
            return;
        };
        let mut diary = vault::load_diary(&vault);
        remove_at(&mut diary.entries, index);
        save_status::record(cx, "diary", vault::save_diary(&vault, &diary));
        cx.notify();
    }
}
