// DOCKCV FORK of altacv 1.6.0 `internal/json-resume.typ`.
//
// Upstream parses and validates a canonical `resume.json` through
// `@preview/gairm-import`, so a user can hand AltaCV a JSON Resume file
// directly. DockCV never takes that path: the dictionary handed to `alta` is
// built in Rust by `src/resume/template.rs` from a `ResumeDoc` that has already
// been validated by the type system. The import is therefore a hard dependency
// on a package that cannot be downloaded (US-10) in service of a route we do
// not use.
//
// `from-json-resume` keeps its name and arity because `lib.typ` re-exports it;
// calling it says why rather than failing to resolve.

#let altacv-schema = none

#let from-json-resume(data) = panic(
  "Importing a JSON Resume file is not available in DockCV: the schema package "
    + "it needs cannot be downloaded at runtime. DockCV builds the CV "
    + "dictionary itself — pass it to `alta` directly.",
)
