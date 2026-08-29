//! DockCV's widgets, plus the facade over the ones upstream already does well.
//!
//! Most primitives — Button, Badge, Tag, Switch, Checkbox, Tooltip, Kbd, Avatar,
//! Label, Separator, menus, Select, Sheet, Scrollbar — come from
//! `gpui-component` and are re-exported here so app code has one import surface
//! and one place to look when something needs replacing. See
//! `crates/ui-components/THIRD_PARTY.md`.
//!
//! What lives here is what upstream has no answer for.

pub mod card;
pub mod empty_state;
pub mod icon;

pub use card::{Card, CardVariant};
pub use empty_state::EmptyState;
pub use icon::{lucide, Assets, DockIcon, Icon, IconName};

// --- upstream, re-exported under our roof ---
pub use gpui_component::{
    // The layout rail's grouping. Nine controls in one column is a list to
    // scroll; four headings with one open is a decision to make.
    accordion::{Accordion, AccordionItem},
    avatar::Avatar,
    badge::Badge,
    // `ButtonGroup` is the segmented control: it joins its children's borders
    // and reports a click as an index into them, so "which segment is on" is
    // `Selectable::selected` on one child rather than three `when(active, ..)`
    // branches in the view. Applications' Board/List/Insights and the layout
    // rail's Letter/A4 were both hand-rolled tracks before.
    button::{Button, ButtonGroup, ButtonVariant, ButtonVariants},
    checkbox::Checkbox,
    // The résumé editor's section cards. `Form::columns(2)` is what turns a
    // stack of full-width rows into a grid: `Start`/`End` are a pair and take
    // one line between them, `Summary` spans both. Before this every field was
    // label-over-input at full width, so one job filled a screen and six of
    // them were an unreadable column.
    form::{Field, Form},
    // A titled container for a run of related controls. Written for a Settings
    // screen that lived inside the vault rail and so could not use upstream's
    // `Settings` page; O-21 moved Settings into a window of its own, where the
    // page component fits, and `settings_window.rs` uses `SettingPage` /
    // `SettingGroup` / `SettingItem` directly. Kept because the facade is meant
    // to be complete — the moment a surface needs a titled group it should not
    // have to reach past this crate for one — but nothing renders it today.
    group_box::{GroupBox, GroupBoxVariant},
    kbd::Kbd,
    label::Label,
    // The row. Left-aligned, hoverable, selectable — the three things a nav
    // entry and a list row need and a `Button` cannot give, because its label
    // sits in an inner `justify_center` flex no caller can reach.
    list::ListItem,
    // Menus. `PopupMenu` is what the gallery card's `···` opens; `ContextMenu`
    // is the right-click variant. Both need `gpui_component::init`, which
    // `crate::init` already calls.
    menu::{ContextMenu, ContextMenuExt, ContextMenuState, DropdownMenu, PopupMenu, PopupMenuItem},
    // The editor's two panes. A fixed 392px column was one guess at how much
    // room a CV's fields need, and it was wrong for every document that has a
    // long summary or a long employer name.
    resizable::{h_resizable, resizable_panel, ResizableState},
    // Scrollbars. `ScrollableElement::overflow_y_scrollbar` is a drop-in for
    // GPUI's `overflow_y_scroll` that keeps the same element as the scroll
    // container and overlays a scrollbar, owning the `ScrollHandle` itself —
    // which is what makes it usable from this app's twenty render helpers,
    // none of which takes a `&mut Window` to hang one on. Before this the app
    // had twenty-two scrollable regions and no scrollbar anywhere.
    scroll::{ScrollableElement, Scrollbar, ScrollbarAxis, ScrollbarShow},
    separator::Separator,
    // The Settings **window** (O-21). The page component brings its own nav
    // column and a search box, which is what a settings window wants and what
    // a pane inside the vault rail could not have had.
    setting::{SettingField, SettingGroup, SettingItem, SettingPage, Settings},
    // The layout rail's Margins and Text-scale controls (C2). `SliderState`
    // is an `Entity` the owning view holds, and it reports movement as
    // `SliderEvent` — the view subscribes and writes the value into the
    // document, which is what makes the change apply to the preview live.
    slider::{Slider, SliderEvent, SliderState},
    // The one animated element in the product: the import wizard's parsing
    // step, which is the only place a user waits on work. It used to draw a
    // `⟳` character that never turned — a spinner that does not spin says the
    // application has hung.
    spinner::Spinner,
    switch::Switch,
    // The Applications list view. This is upstream's *compositional* table —
    // stateless rows and cells — not `DataTable`, which virtualizes and owns
    // its data through a `TableDelegate` entity. A vault holds tens of
    // applications, not thousands, so virtualization buys nothing, and every
    // cell here draws a `Tag`, a chip or a link rather than a string, which is
    // what the composable form is for. Sorting is ours (`applications_data`),
    // pure and tested, rather than delegated to the widget.
    table::{Table, TableBody, TableCaption, TableCell, TableHead, TableHeader, TableRow},
    tag::{Tag, TagVariant},
    tooltip::Tooltip,
};

// --- charts ---
//
// The Applications Insights view. `SankeyChart` renders through upstream's
// `plot` layer, so it is a real element with hover tooltips rather than an
// image; `SankeyLink` addresses nodes **by index into the node list**, which
// is why `applications_analytics` builds its node vector first and maps names
// to indices before constructing any link.
pub use gpui_component::chart::{SankeyChart, SankeyLabel};
pub use gpui_component::plot::shape::{SankeyAlign, SankeyLink, SankeyValueScale};
