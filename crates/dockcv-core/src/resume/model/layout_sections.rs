//! Section-specific presentation choices.

use serde::{Deserialize, Serialize};

/// How the Skills section is laid out.
///
/// Sections differ in what a layout choice even *means* — a skill group is a
/// label and a bag of words, a job is a dated entry with bullets — so this is
/// one enum for one section rather than a `SectionStyle` pretending to span
/// all of them. When Work grows its own choices they get their own type.
///
/// Nothing here derives a proficiency level: the model stores no such field,
/// and a bar chart of invented percentages is exactly the fabricated metric
/// US-14 forbids. Every style below is a different arrangement of words the
/// user actually typed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkillsStyle {
    /// `Category: one, two, three` — one line per group.
    ///
    /// The default, and deliberately the shape every document had before this
    /// existed: a CV written last year renders byte-identically today.
    #[default]
    #[serde(alias = "inline")]
    Rows,
    /// Each keyword in its own pill, the category leading them.
    ///
    /// What a reader scanning for a technology finds fastest, and the reason
    /// this work started — it is the shape every modern builder offers and
    /// the one DockCV could not produce.
    Bubbles,
    /// The category in a fixed left column, keywords flowing beside it.
    ///
    /// Distinct from `Inline`, which wraps keywords under the category's own
    /// indent; here the categories line up as a column, which reads as a
    /// table when there are several.
    Grid,
    /// One flowing list, categories dropped.
    ///
    /// For a CV whose groups are an artefact of import rather than a
    /// distinction worth printing — LinkedIn exports have no categories at
    /// all, so this is the honest shape for that data.
    Compact,
}

impl SkillsStyle {
    pub const ALL: [SkillsStyle; 4] = [
        SkillsStyle::Rows,
        SkillsStyle::Bubbles,
        SkillsStyle::Grid,
        SkillsStyle::Compact,
    ];

    /// What the picker shows.
    pub fn label(self) -> &'static str {
        match self {
            SkillsStyle::Rows => "Rows",
            SkillsStyle::Bubbles => "Bubbles",
            SkillsStyle::Grid => "Grid",
            SkillsStyle::Compact => "Compact",
        }
    }

    /// The word the generated Typst branches on.
    pub fn keyword(self) -> &'static str {
        match self {
            SkillsStyle::Rows => "rows",
            SkillsStyle::Bubbles => "bubbles",
            SkillsStyle::Grid => "grid",
            SkillsStyle::Compact => "compact",
        }
    }
}

/// What goes between two keywords.
///
/// The reason this is a control at all: a Skills section is the densest text
/// on a CV — a real one runs to sixty-odd terms — and a comma disappears
/// between them, so the list reads as one long sentence.
///
/// Measured honestly: a rule is one character *wider* than a comma, so this
/// buys scannability rather than space. The space comes from
/// [`RowSpacing::Tight`] and from dropping the category mark; what this fixes
/// is that sixty comma-separated terms are unreadable at any density.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum SkillSeparator {
    #[default]
    Comma,
    /// `a | b` — the densest of the four, and what the reference layouts use.
    Rule,
    /// `a · b`
    Middot,
    /// `a • b`
    Bullet,
}

impl SkillSeparator {
    pub const ALL: [SkillSeparator; 4] = [
        SkillSeparator::Comma,
        SkillSeparator::Rule,
        SkillSeparator::Middot,
        SkillSeparator::Bullet,
    ];

    pub fn label(self) -> &'static str {
        match self {
            SkillSeparator::Comma => "a, b",
            SkillSeparator::Rule => "a | b",
            SkillSeparator::Middot => "a · b",
            SkillSeparator::Bullet => "a • b",
        }
    }

    /// The characters printed between two keywords, spacing included.
    pub fn printed(self) -> &'static str {
        match self {
            SkillSeparator::Comma => ", ",
            // Single spaces, not double. The first version padded these to
            // `  |  ` and made the section *wider* than commas — it bought
            // scannability and paid for it in the wrapping, which is the
            // opposite of the point. One space each side still separates.
            SkillSeparator::Rule => " | ",
            SkillSeparator::Middot => " · ",
            SkillSeparator::Bullet => " • ",
        }
    }
}

/// What follows a category name, before its keywords.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CategoryMark {
    #[default]
    Colon,
    /// `Category — a, b`, which reads as a heading rather than a key.
    Dash,
    /// `(Category) a, b`
    Bracket,
    /// Nothing at all — the weight of the category carries it.
    None,
}

impl CategoryMark {
    pub const ALL: [CategoryMark; 4] = [
        CategoryMark::Colon,
        CategoryMark::Dash,
        CategoryMark::Bracket,
        CategoryMark::None,
    ];

    pub fn label(self) -> &'static str {
        match self {
            CategoryMark::Colon => "Name:",
            CategoryMark::Dash => "Name —",
            CategoryMark::Bracket => "(Name)",
            CategoryMark::None => "Name",
        }
    }

    /// `(before, after)` the category name.
    pub fn wraps(self) -> (&'static str, &'static str) {
        match self {
            CategoryMark::Colon => ("", ":"),
            CategoryMark::Dash => ("", " —"),
            CategoryMark::Bracket => ("(", ")"),
            CategoryMark::None => ("", ""),
        }
    }
}

/// How much air a Skills section leaves between its rows.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum RowSpacing {
    /// The old spacing.
    #[default]
    Spacious,
    /// Half of it. Eight groups at spacious spacing cost most of a page.
    Tight,
}

impl RowSpacing {
    pub const ALL: [RowSpacing; 2] = [RowSpacing::Spacious, RowSpacing::Tight];

    pub fn label(self) -> &'static str {
        match self {
            RowSpacing::Spacious => "Spacious",
            RowSpacing::Tight => "Tight",
        }
    }

    /// Points between rows.
    pub fn gap_pt(self) -> f32 {
        match self {
            RowSpacing::Spacious => 2.0,
            RowSpacing::Tight => 0.5,
        }
    }
}

/// Everything about how the Skills section is set.
///
/// A struct rather than five fields on `LayoutSettings` because they belong
/// together and TOML says so: `[layout.skills]` with five keys reads as one
/// decision, `skills_separator = …` alongside `page_size` reads as debris.
/// The cost is a migration, since documents already carry `skills = "inline"`
/// — see the `Deserialize` impl, which accepts both shapes.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
pub struct SkillsLayout {
    pub style: SkillsStyle,
    pub separator: SkillSeparator,
    pub mark: CategoryMark,
    pub spacing: RowSpacing,
    /// Start each row with a bullet, so groups read as a list.
    pub bullets: bool,
}

impl<'de> Deserialize<'de> for SkillsLayout {
    /// Accepts the table this writes *and* the bare string that documents
    /// written before the options existed carry (`skills = "inline"`).
    ///
    /// Without this every such document would fail to load — not fall back,
    /// fail — because a string is not a table. A migration that loses the
    /// user's chosen style would be quieter and worse.
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Either {
            JustTheStyle(SkillsStyle),
            Whole {
                #[serde(default)]
                style: SkillsStyle,
                #[serde(default)]
                separator: SkillSeparator,
                #[serde(default)]
                mark: CategoryMark,
                #[serde(default)]
                spacing: RowSpacing,
                #[serde(default)]
                bullets: bool,
            },
        }

        Ok(match Either::deserialize(deserializer)? {
            Either::JustTheStyle(style) => SkillsLayout {
                style,
                ..SkillsLayout::default()
            },
            Either::Whole {
                style,
                separator,
                mark,
                spacing,
                bullets,
            } => SkillsLayout {
                style,
                separator,
                mark,
                spacing,
                bullets,
            },
        })
    }
}

/// Where a dated entry puts its date and location.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MetaPosition {
    /// Right-aligned on the title's own line — compact, and what every
    /// document did before this existed.
    #[default]
    Right,
    /// On its own line under the title. Costs a line per entry and buys a
    /// title that is never squeezed by a long date range.
    Below,
}

impl MetaPosition {
    pub const ALL: [MetaPosition; 2] = [MetaPosition::Right, MetaPosition::Below];

    pub fn label(self) -> &'static str {
        match self {
            MetaPosition::Right => "Right of title",
            MetaPosition::Below => "Below title",
        }
    }

    pub fn keyword(self) -> &'static str {
        match self {
            MetaPosition::Right => "right",
            MetaPosition::Below => "below",
        }
    }
}

/// Which of the date and the location comes first.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MetaOrder {
    #[default]
    DateFirst,
    LocationFirst,
}

impl MetaOrder {
    pub const ALL: [MetaOrder; 2] = [MetaOrder::DateFirst, MetaOrder::LocationFirst];

    pub fn label(self) -> &'static str {
        match self {
            MetaOrder::DateFirst => "Date, place",
            MetaOrder::LocationFirst => "Place, date",
        }
    }

    pub fn keyword(self) -> &'static str {
        match self {
            MetaOrder::DateFirst => "date-first",
            MetaOrder::LocationFirst => "location-first",
        }
    }
}

/// How a run of text is emphasised.
///
/// One type for the two places that need it — an entry's subtitle and its
/// date/location line — because "regular, bold or italic" is the same choice
/// twice and two enums saying it would be two things to keep in step.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Emphasis {
    Regular,
    Bold,
    #[default]
    Italic,
}

impl Emphasis {
    pub const ALL: [Emphasis; 3] = [Emphasis::Regular, Emphasis::Bold, Emphasis::Italic];

    pub fn label(self) -> &'static str {
        match self {
            Emphasis::Regular => "Regular",
            Emphasis::Bold => "Bold",
            Emphasis::Italic => "Italic",
        }
    }

    pub fn keyword(self) -> &'static str {
        match self {
            Emphasis::Regular => "regular",
            Emphasis::Bold => "bold",
            Emphasis::Italic => "italic",
        }
    }
}

/// The glyph a bullet list uses.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum BulletGlyph {
    #[default]
    Dot,
    Dash,
    /// No marker at all — the indent carries it. For a CV whose bullets are
    /// full sentences and read as paragraphs.
    None,
}

impl BulletGlyph {
    pub const ALL: [BulletGlyph; 3] = [BulletGlyph::Dot, BulletGlyph::Dash, BulletGlyph::None];

    pub fn label(self) -> &'static str {
        match self {
            BulletGlyph::Dot => "• Dot",
            BulletGlyph::Dash => "– Dash",
            BulletGlyph::None => "None",
        }
    }

    /// What Typst's `list(marker: …)` is given.
    pub fn marker(self) -> &'static str {
        match self {
            BulletGlyph::Dot => "•",
            BulletGlyph::Dash => "–",
            BulletGlyph::None => "",
        }
    }
}

/// How a dated entry — a job, a degree, a certificate — is set.
///
/// Separate from [`SkillsLayout`] because they are different shapes of data:
/// an entry is a title, a subtitle, two pieces of metadata and a list, and a
/// skill group is a label and a bag of words. A single `SectionStyle` spanning
/// both would have to pretend they answer the same questions.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntryLayout {
    #[serde(default)]
    pub meta_position: MetaPosition,
    #[serde(default)]
    pub meta_order: MetaOrder,
    #[serde(default)]
    pub subtitle: Emphasis,
    #[serde(default)]
    pub meta: Emphasis,
    #[serde(default)]
    pub bullet: BulletGlyph,
    /// Indent the summary and bullets under the entry's title, so the block
    /// reads as belonging to it rather than starting again at the margin.
    #[serde(default)]
    pub indent_body: bool,
}

/// Which edge the header sits against.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HeaderAlign {
    /// What every document did before this existed.
    #[default]
    Center,
    Left,
}

impl HeaderAlign {
    pub const ALL: [HeaderAlign; 2] = [HeaderAlign::Center, HeaderAlign::Left];

    pub fn label(self) -> &'static str {
        match self {
            HeaderAlign::Center => "Centred",
            HeaderAlign::Left => "Left",
        }
    }

    pub fn keyword(self) -> &'static str {
        match self {
            HeaderAlign::Center => "center",
            HeaderAlign::Left => "left",
        }
    }
}

/// How the contact details under the name are arranged.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum ContactLayout {
    /// One flowing line, items joined by a separator. Cheapest in space and
    /// what the header has always done.
    #[default]
    Inline,
    /// One per line. Costs several lines at the top of the page and buys a
    /// header that never wraps mid-address.
    Stacked,
    /// Two columns — half the lines of `Stacked`, still one item per row.
    Columns,
}

impl ContactLayout {
    pub const ALL: [ContactLayout; 3] = [
        ContactLayout::Inline,
        ContactLayout::Stacked,
        ContactLayout::Columns,
    ];

    pub fn label(self) -> &'static str {
        match self {
            ContactLayout::Inline => "One line",
            ContactLayout::Stacked => "One per line",
            ContactLayout::Columns => "Two columns",
        }
    }

    pub fn keyword(self) -> &'static str {
        match self {
            ContactLayout::Inline => "inline",
            ContactLayout::Stacked => "stacked",
            ContactLayout::Columns => "columns",
        }
    }

    /// Whether a separator between items is a real choice for this shape.
    ///
    /// It is not, for the two that put each item on its own row — and a
    /// control that changes nothing is a label pretending to be a control
    /// (E-43). The rail hides it rather than offering a dead one.
    pub fn uses_separator(self) -> bool {
        matches!(self, ContactLayout::Inline)
    }
}

/// How the block above the first section is set: the name, the title under it,
/// and the contact details.
///
/// No icon control here. The reference layouts draw a glyph before each
/// detail, which needs an icon font in the document — the vendored AltaCV
/// package carries FontAwesome for exactly that — and wiring one into this
/// template is its own piece of work rather than a fourth dropdown.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeaderLayout {
    #[serde(default)]
    pub align: HeaderAlign,
    #[serde(default)]
    pub contacts: ContactLayout,
    /// Between contact details, when they share a line. Reuses the Skills
    /// separator because it is the same question — what goes between items in
    /// a run — and a second enum saying it would be a second thing to keep in
    /// step.
    #[serde(default)]
    pub separator: SkillSeparator,
}

/// What one section sets differently from the document's own layout.
///
/// Sparse on purpose: a row exists only while a section actually differs, so a
/// document nobody has customised carries no table at all and the
/// document-wide setting stays the single place to change everything. On a CV,
/// uniformity is the default and difference is the exception — a page assembled
/// from seven layouts reads as broken, not as designed.
///
/// This is the shape the rest of the per-section settings land in: `Option`
/// fields for the layouts that have a document-wide value to fall back to
/// (`HeadingLayout`, `EntryLayout`), plain fields for the ones that only ever
/// make sense for one section, like the flag below.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SectionOverrides {
    /// Print no heading above this section.
    ///
    /// The case that asked for it is Profile: a great many CVs open with the
    /// summary paragraph directly under the contact line, and the renderer
    /// always printed "PROFILE" over it. Per section rather than a seventh
    /// [`HeadingStyle`] because it is never a decision about the whole
    /// document — a CV with no section headings at all is not a CV.
    #[serde(default)]
    pub no_heading: bool,

    // One `Option` per *field* rather than one per struct. Overriding a whole
    // `HeadingLayout` would mean that choosing a style for one section quietly
    // pins its capitalisation and alignment too — and then changing the
    // document's capitalisation would visibly skip the section the user had
    // only ever restyled. Per field, "Skills is set in a rule" stays true and
    // stays *only* that.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading_style: Option<HeadingStyle>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading_case: Option<HeadingCase>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub heading_align: Option<HeaderAlign>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta_position: Option<MetaPosition>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta_order: Option<MetaOrder>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub subtitle: Option<Emphasis>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub meta: Option<Emphasis>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bullet: Option<BulletGlyph>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub indent_body: Option<bool>,
}

impl SectionOverrides {
    /// Whether this row carries nothing. The signal to drop it rather than
    /// write a line of defaults to disk — see the struct's own comment.
    pub fn is_empty(self) -> bool {
        self == Self::default()
    }

    /// Whether this section departs from the document's heading at all — the
    /// question the generated Typst asks before emitting anything for it.
    pub fn touches_heading(self) -> bool {
        self.heading_style.is_some() || self.heading_case.is_some() || self.heading_align.is_some()
    }

    /// The same question for a dated entry.
    pub fn touches_entries(self) -> bool {
        self.meta_position.is_some()
            || self.meta_order.is_some()
            || self.subtitle.is_some()
            || self.meta.is_some()
            || self.bullet.is_some()
            || self.indent_body.is_some()
    }

    /// This section's heading, resolved against the document's.
    pub fn headings(self, document: HeadingLayout) -> HeadingLayout {
        HeadingLayout {
            style: self.heading_style.unwrap_or(document.style),
            case: self.heading_case.unwrap_or(document.case),
            align: self.heading_align.unwrap_or(document.align),
        }
    }

    /// This section's dated entries, resolved against the document's.
    pub fn entries(self, document: EntryLayout) -> EntryLayout {
        EntryLayout {
            meta_position: self.meta_position.unwrap_or(document.meta_position),
            meta_order: self.meta_order.unwrap_or(document.meta_order),
            subtitle: self.subtitle.unwrap_or(document.subtitle),
            meta: self.meta.unwrap_or(document.meta),
            bullet: self.bullet.unwrap_or(document.bullet),
            indent_body: self.indent_body.unwrap_or(document.indent_body),
        }
    }
}

/// How the bar above each section is drawn.
///
/// The band is what every document has had until now, and it is a strong
/// choice: it reads as a divider, but it also spends a filled block of ink on
/// every section of a page that is mostly white. The alternatives are the
/// quieter ways the same job is done on a CV — a rule, a border, or nothing at
/// all — so the decision stops being the template's.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HeadingStyle {
    /// A filled band across the column. What every document did before this.
    #[default]
    Band,
    /// A hairline under the heading, the full width of the column.
    Rule,
    /// The heading, then a hairline carrying on to the right margin. Costs no
    /// line of its own, which on a full CV is a section's worth of space.
    RuleToMargin,
    /// A hairline under the words only, as long as they are.
    Underline,
    /// A thin border around the heading — the band's shape without its fill.
    Boxed,
    /// The words alone. The type does the separating.
    Plain,
}

impl HeadingStyle {
    pub const ALL: [HeadingStyle; 6] = [
        HeadingStyle::Band,
        HeadingStyle::Rule,
        HeadingStyle::RuleToMargin,
        HeadingStyle::Underline,
        HeadingStyle::Boxed,
        HeadingStyle::Plain,
    ];

    pub fn label(self) -> &'static str {
        match self {
            HeadingStyle::Band => "Filled band",
            HeadingStyle::Rule => "Rule under",
            HeadingStyle::RuleToMargin => "Rule to margin",
            HeadingStyle::Underline => "Underline",
            HeadingStyle::Boxed => "Boxed",
            HeadingStyle::Plain => "Plain",
        }
    }

    pub fn keyword(self) -> &'static str {
        match self {
            HeadingStyle::Band => "band",
            HeadingStyle::Rule => "rule",
            HeadingStyle::RuleToMargin => "rule-to-margin",
            HeadingStyle::Underline => "underline",
            HeadingStyle::Boxed => "boxed",
            HeadingStyle::Plain => "plain",
        }
    }

    /// Whether the style puts the heading against an edge the alignment
    /// control can move it along. `RuleToMargin` cannot: its rule fills
    /// whatever the words leave, so the words are always at the left.
    pub fn can_align(self) -> bool {
        !matches!(self, HeadingStyle::RuleToMargin)
    }
}

/// Whether a section title is shouted or printed as the user typed it.
///
/// Two options, not three. "Title Case" would mean rewriting the user's own
/// words, and the rules for doing that are language-specific — this app is
/// used in Ukrainian, where they are not English's. Upper-casing is a
/// reversible display decision; re-capitalising is an edit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum HeadingCase {
    /// What every document did before this existed.
    #[default]
    Upper,
    AsTyped,
}

impl HeadingCase {
    pub const ALL: [HeadingCase; 2] = [HeadingCase::Upper, HeadingCase::AsTyped];

    pub fn label(self) -> &'static str {
        match self {
            HeadingCase::Upper => "UPPERCASE",
            HeadingCase::AsTyped => "As typed",
        }
    }

    pub fn keyword(self) -> &'static str {
        match self {
            HeadingCase::Upper => "upper",
            HeadingCase::AsTyped => "as-typed",
        }
    }
}

/// How the bar above each section is set.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct HeadingLayout {
    #[serde(default)]
    pub style: HeadingStyle,
    #[serde(default)]
    pub case: HeadingCase,
    /// Which edge the words sit against. Reuses [`HeaderAlign`] because it is
    /// the same question the header already asks, and a second enum saying it
    /// would be a second thing to keep in step.
    #[serde(default)]
    pub align: HeaderAlign,
}
