//! How the gallery orders its cards, and the control that says so.
//!
//! Apart from `gallery.rs` because ordering is one decision with three answers
//! and a stored preference behind it, while the gallery is a screen. It is also
//! the half that can be tested: the sort is arithmetic over `DocMeta`, and the
//! case worth pinning — a document nobody has sent — is arithmetic too.

use gpui::{Context, IntoElement};

use dockcv_ui_components::{Button, ButtonExt, DropdownMenu, IconName, PopupMenuItem};

use crate::config;
use crate::resume::model::Applications;
use crate::vault;

use super::shell::Shell;

/// How the gallery orders its cards.
///
/// The order was `docs.sort()` in `vault.rs` — alphabetical by path, which is a
/// fact about the filesystem and not a decision about the product. It stays
/// alphabetical there, because the fingerprint and every non-gallery caller
/// depend on a stable order; ordering for *looking* is a view concern and
/// happens here.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum GallerySort {
    /// Most recently edited first. The default, because the thing you touched
    /// this morning is the thing you are coming back to.
    #[default]
    Recent,
    /// Most recently sent first, from the applications board. Documents never
    /// sent go last — they are not "sent longest ago", they have no answer.
    LastSent,
    /// Alphabetical by file name. The one case alphabetical is a real answer,
    /// which is when you know what the file is called.
    Name,
}

impl GallerySort {
    pub(super) const ALL: [GallerySort; 3] = [
        GallerySort::Recent,
        GallerySort::LastSent,
        GallerySort::Name,
    ];

    pub(super) fn label(self) -> &'static str {
        match self {
            Self::Recent => "Recently edited",
            Self::LastSent => "Recently sent",
            Self::Name => "Name",
        }
    }

    /// The word stored in `config.toml`. Spelled out for the same reason
    /// `update_check` is: the file stays readable and says what it does.
    pub(super) fn word(self) -> &'static str {
        match self {
            Self::Recent => "recent",
            Self::LastSent => "last-sent",
            Self::Name => "name",
        }
    }

    /// A word this build does not know reads as the default rather than as an
    /// error — a hand-edited config should cost an order, not a launch.
    pub(super) fn from_word(word: &str) -> Self {
        Self::ALL
            .into_iter()
            .find(|sort| sort.word() == word)
            .unwrap_or_default()
    }
}

/// Order `metas` in place.
///
/// Split from the view so it can be tested without a window: the interesting
/// part is what happens to a document nobody has sent, and that is arithmetic.
pub(super) fn sort_documents(
    metas: &mut [vault::DocMeta],
    sort: GallerySort,
    applications: &Applications,
) {
    match sort {
        GallerySort::Recent => {
            metas.sort_by(|a, b| {
                b.modified_secs
                    .cmp(&a.modified_secs)
                    .then_with(|| a.stem.cmp(&b.stem))
            });
        }
        GallerySort::LastSent => {
            metas.sort_by(|a, b| {
                let sent =
                    |m: &vault::DocMeta| applications.last_sent_for(&m.stem).map(str::to_string);
                // `None` sorts last: a document that has never been sent has no
                // answer to "when", and putting it at the top under "recently
                // sent" would be the sort telling a lie.
                match (sent(a), sent(b)) {
                    (Some(x), Some(y)) => y.cmp(&x),
                    (Some(_), None) => std::cmp::Ordering::Less,
                    (None, Some(_)) => std::cmp::Ordering::Greater,
                    (None, None) => std::cmp::Ordering::Equal,
                }
                .then_with(|| a.stem.cmp(&b.stem))
            });
        }
        GallerySort::Name => metas.sort_by(|a, b| a.stem.cmp(&b.stem)),
    }
}

impl Shell {
    /// The order control, beside the search box.
    ///
    /// Deliberately the same gesture as the applications board's — same button
    /// style, same icon, same checked menu — because two sort controls in one
    /// product that look and behave differently is a design failure, not a
    /// choice.
    pub(super) fn gallery_sort_control(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let active = self.gallery_sort;
        let shell = cx.weak_entity();

        Button::new("gallery-sort")
            .selector()
            .icon(IconName::SortAscending)
            .label(active.label())
            .tooltip("Order the gallery")
            .dropdown_menu(move |mut menu, _window, _cx| {
                for sort in GallerySort::ALL {
                    let shell = shell.clone();
                    menu = menu.item(
                        PopupMenuItem::new(sort.label())
                            .checked(sort == active)
                            .on_click(move |_ev, _window, cx| {
                                let _ = shell.update(cx, |this, cx| {
                                    this.set_gallery_sort(sort, cx);
                                });
                            }),
                    );
                }
                menu
            })
    }

    /// Change the order, and remember it. A sort you chose and lost on relaunch
    /// is a control that does not work.
    pub(super) fn set_gallery_sort(&mut self, sort: GallerySort, cx: &mut Context<Self>) {
        if self.gallery_sort == sort {
            return;
        }
        self.gallery_sort = sort;
        let mut stored = config::load();
        stored.gallery_sort = sort.word().to_string();
        config::save(&stored);
        cx.notify();
    }
}

#[cfg(test)]
mod tests {
    use super::{sort_documents, GallerySort};
    use crate::resume::model::{Application, ApplicationStatus, Applications, SentCv, StageChange};
    use crate::vault::DocMeta;

    fn meta(stem: &str, modified: Option<u64>) -> DocMeta {
        DocMeta {
            path: std::path::PathBuf::from(format!("/vault/{stem}.toml")),
            stem: stem.to_string(),
            name: "Albert Einstein".into(),
            label: String::new(),
            presets: 0,
            preset_names: Vec::new(),
            unreadable: false,
            modified_secs: modified,
            search: Vec::new(),
        }
    }

    fn sent(stem: &str, on: &str) -> Application {
        Application {
            sent_as: Some(SentCv {
                document: stem.into(),
                preset: String::new(),
            }),
            history: vec![StageChange {
                at: on.into(),
                to: ApplicationStatus::Applied.word().into(),
            }],
            ..Default::default()
        }
    }

    fn stems(metas: &[DocMeta]) -> Vec<&str> {
        metas.iter().map(|m| m.stem.as_str()).collect()
    }

    #[test]
    fn recency_is_the_default_and_beats_the_alphabet() {
        let mut metas = vec![
            meta("alpha", Some(100)),
            meta("zulu", Some(300)),
            meta("mike", Some(200)),
        ];
        sort_documents(&mut metas, GallerySort::default(), &Applications::default());
        assert_eq!(stems(&metas), ["zulu", "mike", "alpha"]);
        assert_eq!(GallerySort::default(), GallerySort::Recent);
    }

    /// The case worth having a test for: a document nobody has sent has no
    /// answer to "when", and must not read as the most recently sent.
    #[test]
    fn a_document_never_sent_sorts_last_under_last_sent() {
        let apps = Applications {
            entries: vec![sent("alpha", "2026-03-01"), sent("zulu", "2026-08-01")],
        };
        let mut metas = vec![
            meta("never-sent", Some(999)),
            meta("alpha", Some(1)),
            meta("zulu", Some(2)),
        ];
        sort_documents(&mut metas, GallerySort::LastSent, &apps);
        assert_eq!(stems(&metas), ["zulu", "alpha", "never-sent"]);
    }

    #[test]
    fn name_orders_by_the_file_stem_not_the_person() {
        let mut metas = vec![meta("zulu", Some(300)), meta("alpha", Some(100))];
        sort_documents(&mut metas, GallerySort::Name, &Applications::default());
        assert_eq!(stems(&metas), ["alpha", "zulu"]);
    }

    /// Two documents that tie on the sort key still come out in one order,
    /// every time — a grid that reshuffles on every repaint is unusable.
    #[test]
    fn a_tie_is_broken_by_name_so_the_order_is_stable() {
        let apps = Applications::default();
        for sort in GallerySort::ALL {
            let mut metas = vec![meta("zulu", Some(5)), meta("alpha", Some(5))];
            sort_documents(&mut metas, sort, &apps);
            assert_eq!(stems(&metas), ["alpha", "zulu"], "{sort:?} is unstable");
        }
    }

    /// End to end through the real config parse, not just `from_word`: a
    /// `config.toml` written before this field existed has no key for it, and
    /// the gallery has to open on recency rather than on whatever `Default`
    /// happens to put first.
    #[test]
    fn a_config_without_the_key_opens_on_recency() {
        let older: crate::config::Config = toml::from_str(
            r#"
            vault = "/Users/someone/cvault"
            theme = "slate_dark"
            update_check = "manual"
            "#,
        )
        .expect("a config from before this field still parses");
        assert_eq!(older.gallery_sort, "");
        assert_eq!(
            GallerySort::from_word(&older.gallery_sort),
            GallerySort::Recent
        );
        assert_eq!(
            GallerySort::from_word(&older.gallery_sort).label(),
            "Recently edited"
        );

        // And a key that is present is honoured.
        let stored: crate::config::Config =
            toml::from_str(r#"gallery_sort = "name""#).expect("parses");
        assert_eq!(
            GallerySort::from_word(&stored.gallery_sort),
            GallerySort::Name
        );
    }

    #[test]
    fn an_unknown_word_in_the_config_reads_as_the_default() {
        assert_eq!(GallerySort::from_word("last-sent"), GallerySort::LastSent);
        assert_eq!(GallerySort::from_word("nonsense"), GallerySort::Recent);
        for sort in GallerySort::ALL {
            assert_eq!(GallerySort::from_word(sort.word()), sort);
        }
    }
}
