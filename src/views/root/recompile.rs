use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use gpui::{Context, Window};

use crate::render::{self, Rendered};
use crate::resume::diagnostics::{describe_all, CompileMessage};
use crate::resume::template;
use crate::typst_engine::{PageGeometry, Severity, TypstEngine};

use super::Root;

/// Idle time before the quick draft re-render fires (16ms = 1 frame @ 60 FPS).
pub(super) const DRAFT_DEBOUNCE: Duration = Duration::from_millis(16);
/// Further idle time before the crisp, full-resolution re-render fires. Both
/// passes live in one debounced task, so the crisp pass is cancelled by the
/// next edit — you get cheap draft frames while typing and a sharp frame once
/// you pause.
pub(super) const CRISP_DELAY: Duration = Duration::from_millis(150);

/// One frame at 60 Hz. A full-resolution compile faster than this is not worth
/// showing a soft page to avoid.
pub(super) const FRAME_BUDGET: Duration = Duration::from_millis(16);
/// Ceiling on the preview's rasterization scale, in pixels per typographic
/// point. Bounds memory: an A4 page at 6× is ~3600×5000 px, and nothing on a
/// screen can show more detail than the display's own pixels anyway.
pub(super) const MAX_RENDER_SCALE: f32 = 6.0;

/// The compile state the UI can always show something for — "compiling /
/// ready / error", not just "error" (US-07's own acceptance text). A failed
/// or in-flight compile never touches [`Root::rendered`]: the last good
/// frame stays on screen exactly as before; this only tracks what to *say*
/// about the most recent attempt.
///
/// The design doc's warning-styled banner with `Jump to section →` (the Typst-controls spec)
/// is a separate, later task — this type carries what that UI needs
/// ([`CompileMessage`]'s severity and optional section), it just isn't
/// rendered here.
#[derive(Debug, Clone, PartialEq)]
pub enum CompileState {
    /// A recompile is in flight and hasn't produced a frame yet — true only
    /// before the very first render lands, since after that the debounced
    /// draft pass (`schedule_recompile`) always lands quickly enough that
    /// there is no separate "still compiling" gap the user would see beyond
    /// the existing draft→crisp progression.
    Compiling,
    /// The most recent compile succeeded. `warnings` is every diagnostic
    /// that attempt produced — always `Severity::Warning`, never empty *and*
    /// `Error` at once, since an error would have produced `Error` instead.
    Ready { warnings: Vec<CompileMessage> },
    /// The most recent compile failed. `messages` is every diagnostic from
    /// that attempt (errors, plus any warnings emitted before the error),
    /// each already a full sentence naming a section when that's honest.
    Error { messages: Vec<CompileMessage> },
}

/// What one compile attempt produced, for [`Root::apply_render`] to fold
/// into [`CompileState`]. `rendered` is `Some` exactly when the compile
/// succeeded *and* its pixels converted to a GPUI image; `messages` is every
/// diagnostic from the attempt, humanized and section-attributed via
/// `resume::diagnostics::describe_all`.
pub(super) struct RenderOutcome {
    pub(super) rendered: Option<Rendered>,
    pub(super) messages: Vec<CompileMessage>,
    /// What the compiler measured about the laid-out pages — the page count
    /// the toolbar shows (US-08's `1 / 2`) and the overflow the chip reports.
    /// `None` when the compile failed, so the last good measurement stays on
    /// screen beside the last good render rather than blanking.
    pub(super) geometry: Option<PageGeometry>,
}

/// Lock the engine, set the source, compile, and rasterize at `scale`. Runs
/// on a background thread; every type crossing back (`RenderOutcome`) is
/// `Send`.
///
/// `source` doubles as the text `resume::diagnostics::describe_all` needs
/// for section attribution — the same generated document a diagnostic's
/// byte offset points into.
pub(super) fn compile(
    engine: &Arc<Mutex<TypstEngine>>,
    source: String,
    scale: f32,
) -> RenderOutcome {
    let attempt = {
        let mut engine = engine.lock().unwrap_or_else(|e| e.into_inner());
        engine.set_source(source.clone());
        engine.compile_with_diagnostics(scale)
    };

    let messages = describe_all(&attempt.diagnostics, &source);
    let mut measured: Option<PageGeometry> = None;

    let rendered = match attempt.result {
        Ok((pixels, geometry)) => match render::pixels_to_render_image(pixels, scale) {
            Ok(rendered) => {
                measured = Some(geometry);
                Some(rendered)
            }
            Err(message) => {
                // Rasterization itself failed (not a Typst diagnostic) —
                // report it the same way, rather than silently keeping the
                // stale `messages` from a compile that actually succeeded.
                return RenderOutcome {
                    geometry: None,
                    rendered: None,
                    messages: vec![CompileMessage {
                        severity: Severity::Error,
                        section: None,
                        text: format!("Couldn't prepare the preview image: {message}."),
                    }],
                };
            }
        },
        Err(()) => None,
    };

    RenderOutcome {
        rendered,
        messages,
        geometry: measured,
    }
}

impl Root {
    /// Pixels per typographic point the preview should rasterize at, derived
    /// from the size the sheet is **actually drawn**.
    ///
    /// This used to be `window.scale_factor()` — 2.0 on a retina display, and
    /// nothing to do with how large the page was on screen. At 100% zoom that
    /// happened to be roughly right; at 200% it left the same raster stretched
    /// across twice the pixels, which is why zooming in made the page softer
    /// instead of sharper, and why the whole preview "felt like a half
    /// measure". Zoom now *re-renders* rather than magnifying a bitmap.
    pub(super) fn crisp_scale(&self, window: &Window) -> f32 {
        let page_width_pt = self.effective_layout().page_size.width_pt();
        if page_width_pt <= 0.0 {
            return window.scale_factor();
        }
        // Device pixels the sheet occupies, divided by the points it is wide.
        let device_px = self.preview_width() * window.scale_factor();
        (device_px / page_width_pt).clamp(1.0, MAX_RENDER_SCALE)
    }

    pub(super) fn recompile_now(&mut self, window: &mut Window) {
        let source = template::generate_for_with_profiles(&self.doc, &self.profiles);
        let scale = self.crisp_scale(window);
        self.last_scale = scale;
        self.last_source = source.clone();
        self.compile_state = CompileState::Compiling;
        let outcome = compile(&self.engine, source, scale);
        self.apply_render(outcome, window);
    }

    /// Debounced, off-thread recompile. Call this on every edit: the UI has
    /// already updated the model/caret, and this catches the preview up shortly
    /// after the user stops typing — without ever blocking the UI thread.
    pub(super) fn schedule_recompile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let source = template::generate_for_with_profiles(&self.doc, &self.profiles);
        let crisp_scale = self.crisp_scale(window);

        // Skip work entirely when neither the document nor the resolution it
        // needs has changed — editing a variant name changes no output, and
        // re-rendering at a scale we already have would be a compile for
        // nothing.
        if source == self.last_source && (crisp_scale - self.last_scale).abs() < 0.01 {
            return;
        }
        self.last_source = source.clone();
        self.last_scale = crisp_scale;
        self.compile_state = CompileState::Compiling;

        // The draft is half the final resolution rather than a fixed 1×: at
        // 1× the raster was *narrower than the sheet on screen* and the page
        // read as soft the whole time you were typing.
        let draft_scale = (crisp_scale * 0.5).max(1.0);

        // **Skip the draft when the sharp pass is quick enough to wait for.**
        //
        // The draft buys latency to first pixel and pays for it in a page that
        // is visibly soft for as long as the user keeps typing — at 100% on a
        // 2× display the draft is stretched 1.6×. When the full-resolution
        // compile lands inside a frame nobody perceives the wait, so the blur
        // is bought with nothing. The first edit of a session has no
        // measurement yet and takes the draft, which is also the compile most
        // likely to be slow: nothing is cached.
        let skip_draft =
            crisp_scale <= draft_scale + 0.01 || self.last_crisp.is_some_and(|d| d <= FRAME_BUDGET);

        let engine = self.engine.clone();
        let executor = cx.background_executor().clone();

        self.recompile_task = Some(cx.spawn_in(window, async move |this, cx| {
            if !skip_draft {
                executor.timer(DRAFT_DEBOUNCE).await;
                let draft = {
                    let engine = engine.clone();
                    let source = source.clone();
                    executor
                        .spawn(async move { compile(&engine, source, draft_scale) })
                        .await
                };
                if this
                    .update_in(cx, |this, window, _cx| this.apply_render(draft, window))
                    .is_err()
                {
                    return;
                }
                // The draft already is the final resolution — nothing to sharpen.
                if crisp_scale <= draft_scale + 0.01 {
                    return;
                }
                // Only if no new edit cancels us first.
                executor.timer(CRISP_DELAY).await;
            } else {
                executor.timer(DRAFT_DEBOUNCE).await;
            }

            let started = Instant::now();
            let crisp = executor
                .spawn(async move { compile(&engine, source, crisp_scale) })
                .await;
            let elapsed = started.elapsed();
            let _ = this.update_in(cx, |this, window, _cx| {
                this.last_crisp = Some(elapsed);
                this.apply_render(crisp, window);
            });
        }));
    }

    /// Store a freshly rendered page (freeing the previous GPU texture) and
    /// update [`Root::compile_state`] from what the attempt produced. A
    /// failed compile never touches `self.rendered` — the last good frame
    /// stays on screen exactly as before this change.
    pub(super) fn apply_render(&mut self, outcome: RenderOutcome, _window: &mut Window) {
        match outcome.rendered {
            Some(rendered) => {
                if let Some(previous) = self.rendered.take() {
                    // Retired, not dropped — see `retired_images`.
                    self.retired_images.push(previous.image);
                }
                self.rendered = Some(rendered);
                // Only on success: a failed compile leaves the last good
                // measurement beside the last good frame, so the page counter
                // does not blank out while you fix a typo.
                if outcome.geometry.is_some() {
                    self.geometry = outcome.geometry;
                }
                self.compile_state = CompileState::Ready {
                    warnings: outcome.messages,
                };
            }
            None => {
                self.compile_state = CompileState::Error {
                    messages: outcome.messages,
                }
            }
        }
    }

    /// Every message from the most recent failed compile, joined into one
    /// string — feeds `render_preview`'s existing plain-text error box. The
    /// design doc's warning-styled banner with section attribution and a
    /// "Jump to section →" action (the Typst-controls spec) is a
    /// separate, later task; this just keeps the current display correct
    /// under the new state shape.
    pub(super) fn compile_error_text(&self) -> Option<String> {
        match &self.compile_state {
            CompileState::Error { messages } => Some(
                messages
                    .iter()
                    .map(|m| m.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n"),
            ),
            _ => None,
        }
    }
}

#[cfg(test)]
mod draft_policy_tests {
    use super::{FRAME_BUDGET, MAX_RENDER_SCALE};
    use std::time::Duration;

    /// The rule `schedule_recompile` applies, isolated from the task it runs
    /// in: the draft is skipped when it would buy nothing.
    fn skips_draft(crisp_scale: f32, last_crisp: Option<Duration>) -> bool {
        let draft_scale = (crisp_scale * 0.5).max(1.0);
        crisp_scale <= draft_scale + 0.01 || last_crisp.is_some_and(|d| d <= FRAME_BUDGET)
    }

    #[test]
    fn a_sharp_pass_inside_a_frame_is_worth_waiting_for() {
        // 100% zoom on a 2× display: the crisp scale a `PageSize::A4` needs.
        assert!(skips_draft(1.61, Some(Duration::from_millis(8))));
        // Slow enough to notice: the draft earns its blur.
        assert!(!skips_draft(3.23, Some(Duration::from_millis(40))));
    }

    /// The first edit of a session has nothing measured — and is the compile
    /// most likely to be slow, since nothing is cached yet.
    #[test]
    fn the_first_edit_takes_the_draft() {
        assert!(!skips_draft(3.23, None));
    }

    /// At low zoom the draft and the sharp pass are the same raster, so there
    /// is no second pass to wait for whatever the timing says.
    #[test]
    fn a_draft_equal_to_the_sharp_pass_is_skipped_regardless() {
        assert!(skips_draft(1.0, None));
        assert!(skips_draft(1.0, Some(Duration::from_secs(1))));
        // And the ceiling is still the ceiling.
        assert!(!skips_draft(
            MAX_RENDER_SCALE,
            Some(Duration::from_millis(40))
        ));
    }
}
