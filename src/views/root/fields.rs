use std::collections::HashSet;

use gpui::{prelude::*, Context, Entity, Subscription, Window};

use dockcv_ui_components::{TextFieldEvent, TextFieldState};

use crate::resume::edit::FieldId;

use super::Root;

/// A field's live input state together with the subscription that writes its
/// changes into the document. Dropping the pair unsubscribes.
pub(crate) struct FieldBinding {
    pub(crate) state: Entity<TextFieldState>,
    pub(crate) _subscription: Subscription,
}

impl Root {
    /// Bring [`Root::fields`] in line with the document.
    ///
    /// Runs once per frame. Fields keep their state — and therefore their caret,
    /// selection and undo history — across renders; they are rebuilt only when
    /// `fields_stale` says the model moved under them.
    pub(super) fn sync_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.fields_stale {
            self.fields.clear();
            self.fields_stale = false;
            self.focus_handle.focus(window, cx);
        }

        let wanted = FieldId::addressable(&self.doc);
        for field in &wanted {
            if self.fields.contains_key(field) {
                continue;
            }
            let Some(value) = field.get(&self.doc).cloned() else {
                continue;
            };
            self.fields
                .insert(*field, self.bind_field(*field, value, window, cx));
        }

        // Retire states for fields the document no longer has, so a deleted
        // bullet cannot leave a live input behind.
        if self.fields.len() > wanted.len() {
            let live: HashSet<FieldId> = wanted.into_iter().collect();
            self.fields.retain(|field, _| live.contains(field));
        }
    }

    /// Create one field's state, seed it from the model, and wire its changes back.
    fn bind_field(
        &self,
        field: FieldId,
        value: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> FieldBinding {
        let state = cx.new(|cx| {
            let state = if field.multiline() {
                TextFieldState::auto_grow(2, 12, window, cx)
            } else {
                TextFieldState::single_line(window, cx)
            };
            state.seed(value, window, cx);
            state
        });

        let subscription = cx.subscribe_in(
            &state,
            window,
            move |this, state, event: &TextFieldEvent, window, cx| {
                if !matches!(event, TextFieldEvent::Changed) {
                    return;
                }
                let value = state.read(cx).value(cx).to_string();
                let Some(target) = field.get_mut(&mut this.doc) else {
                    return;
                };
                if *target == value {
                    return;
                }
                *target = value;
                this.schedule_save(cx);
                this.schedule_recompile(window, cx);
                cx.notify();
            },
        );

        FieldBinding {
            state,
            _subscription: subscription,
        }
    }
}
