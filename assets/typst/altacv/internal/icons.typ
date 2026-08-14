// DOCKCV FORK of altacv 1.6.0 `internal/icons.typ`.
//
// Upstream delegates the glyph to `@preview/fontawesome`, which carries no
// fonts of its own and expects the FontAwesome desktop fonts to be installed.
// DockCV cannot download either (US-10), and bundling them would add roughly a
// megabyte under a second licence for an icon set the app does not otherwise
// use.
//
// It already ships **Lucide** (ISC) and draws its whole interface with it, so
// the CV's icons come from the same set: the document then looks like the
// application rather than like a different product, and the cost is one small
// SVG per icon instead of a font family.
//
// The seam upstream describes is kept exactly — this file still owns only the
// logical-name → glyph table and the sizing/colour wrapper, so `icon(name)`
// behaves the same for every caller.
//
// **Brand marks are the one loss.** Lucide deliberately has no brand set, so
// only `github` is available (from the icon bundle the app already carries).
// Every other network falls back to the generic link mark rather than to a
// wrong logo — an approximate brand is worse than an honest link.

#import "state.typ": _body_colour, _body_size_state

// Logical name → the Lucide stem drawn for it. Renaming a key here is a
// breaking change for any caller of the public `icon(...)`.
#let _utility_icons = (
  book: "book",
  calendar: "calendar",
  email: "mail",
  file: "file-text",
  location: "map-pin",
  microphone: "mic",
  newspaper: "newspaper",
  phone: "phone",
)

// Profile-network icons. Keys are lowercase to match
// `lower(profile.network)` in `internal/header.typ`.
#let _network_icons = (
  bluesky: "link",
  github: "github",
  gitlab: "link",
  link: "link",
  linkedin: "link",
  mastodon: "link",
  medium: "link",
  stackoverflow: "link",
  twitter: "link",
  website: "globe",
)

#let _icon_glyphs = _utility_icons + _network_icons

// Kept from upstream unchanged — `internal/header.typ` imports both, and the
// set of networks a CV may name is a fact about the template, not about which
// icon set draws them.
#let _profile_networks = _network_icons.keys()

#let _network_aliases = (
  x: "twitter",
)

// Lucide draws with `stroke="currentColor"`, which Typst does not resolve, so
// the colour is substituted into the markup before the image is built. Reading
// the file as a string rather than with `image("…")` is what makes that
// possible — and it is the only way an icon can take the document's own text
// colour.
#let _svg(stem, fill) = {
  let markup = read("../assets/icons/" + stem + ".svg")
  let hex = fill.to-hex()
  image(
    bytes(markup.replace("currentColor", hex).replace("'", "\"")),
    format: "svg",
  )
}

#let icon(name, size: auto, shift: auto, fill: auto) = context {
  let body-size = _body_size_state.get()
  let resolved-size = if size == auto { body-size } else { size }
  let resolved-shift = if shift == auto { 0.15 * body-size } else { shift }
  let resolved-fill = if fill == auto { _body_colour } else { fill }

  // An unknown network is a link, not a failure: `header.typ` passes whatever
  // the user typed, and a CV should not stop compiling over a spelling.
  let stem = _icon_glyphs.at(name, default: "link")
  box(
    baseline: resolved-shift,
    width: resolved-size,
    height: resolved-size,
    align(center + horizon, _svg(stem, resolved-fill)),
  )
  h(0.3 * body-size)
}
