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
    /// selection and undo history — across renders; they are reconciled when
    /// `fields_stale` says the model moved under them.
    pub(super) fn sync_fields(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let is_stale = self.fields_stale;
        self.fields_stale = false;

        let wanted = FieldId::addressable(&self.doc);
        let live: HashSet<FieldId> = wanted.iter().copied().collect();

        // 1. When the document model changed structurally (undo/redo, variant
        // switch, deletion index shifts, preset change, or external disk reload),
        // reconcile live fields whose values changed in the document without
        // dropping their entities. Fields whose values have not changed remain
        // completely untouched, preserving their caret, selection, and undo history.
        if is_stale {
            for field in &wanted {
                if let Some(binding) = self.fields.get(field) {
                    if let Some(model_value) = field.get(&self.doc) {
                        let current_value = binding.state.read(cx).value(cx);
                        if current_value.as_ref() != model_value {
                            binding.state.update(cx, |state, cx| {
                                state.seed(model_value, window, cx);
                            });
                        }
                    }
                }
            }
        }

        // 2. Bind newly addressable fields.
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

        // 3. Retire states for fields the document no longer has (e.g. a deleted bullet).
        // If a retired field had focus, return focus to the root handle so focus is
        // not left trapped on a detached element.
        if self.fields.len() > wanted.len() {
            let mut focused_retired = false;
            self.fields.retain(|field, binding| {
                let keep = live.contains(field);
                if !keep && binding.state.read(cx).is_focused(window, cx) {
                    focused_retired = true;
                }
                keep
            });
            if focused_retired {
                self.focus_handle.focus(window, cx);
            }
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
