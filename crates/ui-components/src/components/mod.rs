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
    avatar::Avatar,
    badge::Badge,
    button::{Button, ButtonVariant, ButtonVariants},
    checkbox::Checkbox,
    // The résumé editor's section cards. `Form::columns(2)` is what turns a
    // stack of full-width rows into a grid: `Start`/`End` are a pair and take
    // one line between them, `Summary` spans both. Before this every field was
    // label-over-input at full width, so one job filled a screen and six of
    // them were an unreadable column.
    form::{Field, Form},
    // The titled container the Settings screen and the layout rail both group
    // their controls with. `SettingGroup`/`SettingItem` — the obvious choice —
    // are unreachable: `SettingGroup::render` is `pub(crate)`, so a group only
    // renders inside the `Settings` page component, which brings its own
    // 250px nav sidebar. Our Settings screen already sits inside the vault
    // rail, and a second sidebar is the "two navigations for one move" the
    // design rules forbid; the layout rail is 220px wide and has no room for a
    // page at all. `GroupBox` is what `SettingGroup` wraps, and it fits both.
    group_box::{GroupBox, GroupBoxVariant},
    // The editor's two panes. A fixed 392px column was one guess at how much
    // room a CV's fields need, and it was wrong for every document that has a
    // long summary or a long employer name.
    resizable::{h_resizable, resizable_panel, ResizableState},
    // The Settings **window** (O-21). The page component brings its own nav
    // column and a search box, which is what a settings window wants and what
    // a pane inside the vault rail could not have had.
    setting::{SettingField, SettingGroup, SettingItem, SettingPage, Settings},
    kbd::Kbd,
    label::Label,
    // Menus. `PopupMenu` is what the gallery card's `···` opens; `ContextMenu`
    // is the right-click variant. Both need `gpui_component::init`, which
    // `crate::init` already calls.
    menu::{ContextMenu, ContextMenuExt, ContextMenuState, DropdownMenu, PopupMenu, PopupMenuItem},
    separator::Separator,
    // The layout rail's Margins and Text-scale controls (C2). `SliderState`
    // is an `Entity` the owning view holds, and it reports movement as
    // `SliderEvent` — the view subscribes and writes the value into the
    // document, which is what makes the change apply to the preview live.
    slider::{Slider, SliderEvent, SliderState},
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
