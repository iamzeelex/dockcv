//! CVs DockCV did not write, in the shapes other templates use.
//!
//! Everything the importer is tested against so far came out of our own
//! emitters, which is the one case where the two ends already agree. The files
//! people actually import were made by Word templates, Canva, Europass, a
//! LaTeX class or a website — and those put a CV together in ways ours never
//! does: a sidebar down the left, the whole document inside one table, the
//! contact block in a running header, the dates in a gutter, no section
//! headings at all.
//!
//! These are written as Typst source rather than checked in as binaries on
//! purpose. A fixture nobody can read is a fixture nobody will change, and the
//! *shape* is what is being tested, not any particular file's bytes — so the
//! shape is the thing that should be in the repository. The compiler turns
//! them into PDFs at test time, which is also how the corpus stays honest: it
//! goes through a real PDF, with real glyph positions, not a convenient string.

/// One foreign template, and what any importer worth the name has to get out
/// of it.
pub struct ForeignCv {
    pub name: &'static str,
    /// The layout this emulates, and why it is hard.
    pub shape: &'static str,
    pub source: &'static str,
    /// Facts the file states plainly. Anything here that does not come back is
    /// a defect, whatever the layout was doing.
    pub must_recover: &'static [&'static str],
}

pub fn all() -> Vec<ForeignCv> {
    vec![
        ForeignCv {
            name: "sidebar",
            shape: "a narrow left column with the contact block and skills, the \
                    experience beside it — Canva, Europass and half the Word gallery. \
                    Reading it in visual order interleaves the two columns line by line",
            source: r#"
#set page(paper: "a4", margin: 14mm)
#set text(font: "Libertinus Serif", size: 10pt)
#grid(
  columns: (32%, 1fr), column-gutter: 8mm,
  [
    #text(size: 18pt, weight: "bold")[Marta Iversen]
    #v(2pt)
    #text(size: 10pt)[Staff Data Engineer]
    #v(6pt)
    #text(weight: "bold")[CONTACT]
    #v(2pt)
    marta\@example.com \
    +47 22 55 01 00 \
    Oslo, Norway
    #v(6pt)
    #text(weight: "bold")[SKILLS]
    #v(2pt)
    Spark \
    Airflow \
    Snowflake
  ],
  [
    #text(weight: "bold")[EXPERIENCE]
    #v(3pt)
    #text(weight: "bold")[Staff Data Engineer] \
    Nordisk Data AS, 2020-03 – Present
    - Rebuilt the ingestion pipeline that feeds every internal dashboard.
    - Cut the nightly batch from nine hours to forty minutes.
    #v(4pt)
    #text(weight: "bold")[Data Engineer] \
    Fjord Analytics, 2017-08 – 2020-02
    - Built the first warehouse the company had.
    #v(6pt)
    #text(weight: "bold")[EDUCATION]
    #v(3pt)
    M.Sc. in Informatics \
    University of Oslo, 2015 – 2017
  ],
)
"#,
            must_recover: &[
                "Marta Iversen",
                "marta@example.com",
                "Staff Data Engineer",
                "Nordisk Data AS",
                "Rebuilt the ingestion pipeline that feeds every internal dashboard.",
                "University of Oslo",
                "Snowflake",
            ],
        },
        ForeignCv {
            name: "one big table",
            shape: "the whole CV inside a table, one row per entry with the dates in \
                    their own cell — what a Word template produces and what killed the \
                    previous DOCX engine, here in a PDF where there are no cells at all, \
                    only glyphs in columns",
            source: r#"
#set page(paper: "a4", margin: 16mm)
#set text(font: "Libertinus Serif", size: 10pt)
#align(center)[#text(size: 18pt, weight: "bold")[Tomás Ferreira]]
#align(center)[tomas\@example.com · +351 21 555 0100 · Lisbon]
#v(6pt)
#text(weight: "bold")[PROFESSIONAL EXPERIENCE]
#v(3pt)
#table(
  columns: (25%, 1fr), stroke: none, inset: 4pt,
  [2021-02 – Present], [
    #text(weight: "bold")[Principal Engineer], Atlantic Systems \
    Led the platform team through two acquisitions.
  ],
  [2018-05 – 2021-01], [
    #text(weight: "bold")[Senior Engineer], Tagus Software \
    Owned the billing service end to end.
  ],
)
#v(4pt)
#text(weight: "bold")[EDUCATION]
#v(3pt)
#table(
  columns: (25%, 1fr), stroke: none, inset: 4pt,
  [2014 – 2018], [Licenciatura in Computer Science, Universidade de Lisboa],
)
"#,
            must_recover: &[
                "Tomás Ferreira",
                "tomas@example.com",
                "Principal Engineer",
                "Atlantic Systems",
                "Led the platform team through two acquisitions.",
                "Universidade de Lisboa",
            ],
        },
        ForeignCv {
            name: "contacts in the running header",
            shape: "name and contact details in the page header rather than the body — \
                    a LaTeX class habit, and the one layout every ATS guide warns about, \
                    because the header is a different part of the page and some readers \
                    drop it entirely",
            source: r#"
#set page(
  paper: "a4", margin: (x: 16mm, top: 26mm, bottom: 16mm),
  header: [
    #align(center)[#text(size: 16pt, weight: "bold")[Ingrid Sørensen]]
    #align(center)[#text(size: 9pt)[ingrid\@example.com · +45 33 55 01 00 · Copenhagen]]
  ],
)
#set text(font: "Libertinus Serif", size: 10pt)
#text(weight: "bold")[EXPERIENCE]
#v(3pt)
#text(weight: "bold")[Lead Backend Engineer], Nordic Payments \
2019-09 – Present
- Rewrote the settlement engine that clears every transaction overnight.
#v(4pt)
#text(weight: "bold")[EDUCATION]
#v(3pt)
M.Sc. in Software Engineering, DTU, 2014 – 2016
"#,
            must_recover: &[
                "Ingrid Sørensen",
                "ingrid@example.com",
                "Lead Backend Engineer",
                "Nordic Payments",
                "Rewrote the settlement engine that clears every transaction overnight.",
            ],
        },
        ForeignCv {
            name: "dates in the gutter",
            shape: "a narrow left gutter of dates against entries on the right, with no \
                    punctuation joining them — the reader has to put a date back with \
                    the job it belongs to using nothing but the fact that they share a \
                    line",
            source: r#"
#set page(paper: "a4", margin: 16mm)
#set text(font: "Libertinus Serif", size: 10pt)
#text(size: 18pt, weight: "bold")[Yusuf Demir]
#v(1pt)
yusuf\@example.com · +90 212 555 0100 · Istanbul
#v(8pt)
#text(weight: "bold")[WORK EXPERIENCE]
#v(4pt)
#grid(
  columns: (22%, 1fr), row-gutter: 8pt,
  [2020 – Present], [#text(weight: "bold")[Engineering Manager] \ Bosphorus Tech \ Grew the platform group from four to nineteen.],
  [2016 – 2020], [#text(weight: "bold")[Senior Engineer] \ Anatolia Cloud \ Ran the migration off bare metal.],
)
#v(8pt)
#text(weight: "bold")[EDUCATION]
#v(4pt)
#grid(
  columns: (22%, 1fr),
  [2012 – 2016], [B.Sc. in Computer Engineering \ Boğaziçi University],
)
"#,
            must_recover: &[
                "Yusuf Demir",
                "yusuf@example.com",
                "Engineering Manager",
                "Bosphorus Tech",
                "Grew the platform group from four to nineteen.",
                "Boğaziçi University",
            ],
        },
        ForeignCv {
            name: "no headings at all",
            shape: "a minimalist template with no section headings — the reader has \
                    nothing to segment on and has to tell a job from a degree by what \
                    the lines say",
            source: r#"
#set page(paper: "a4", margin: 18mm)
#set text(font: "Libertinus Serif", size: 10pt)
#text(size: 17pt, weight: "bold")[Hana Kowalczyk]
#v(1pt)
hana\@example.com · +48 22 555 0100 · Kraków
#v(8pt)
#text(weight: "bold")[Principal Engineer], Vistula Systems, 2021-04 – Present \
Rebuilt the scheduler that every batch job in the company runs on.
#v(5pt)
#text(weight: "bold")[Senior Engineer], Wawel Software, 2018-01 – 2021-03 \
Took the API from three customers to nine hundred.
#v(5pt)
#text(weight: "bold")[M.Sc. in Computer Science], Jagiellonian University, 2013 – 2018
"#,
            must_recover: &[
                "Hana Kowalczyk",
                "hana@example.com",
                "Principal Engineer",
                "Vistula Systems",
                "Rebuilt the scheduler that every batch job in the company runs on.",
                "Jagiellonian University",
            ],
        },
        ForeignCv {
            name: "right to left",
            shape: "a Hebrew CV. A PDF records where glyphs landed and never the \
                    order they were typed in, so the text layer comes back spelled \
                    backwards — and a script with no capitals in it made every short \
                    line look like a shouted section heading, so the person's name \
                    became a section with the document filed under it",
            source: r#"
#set page(paper: "a4", margin: 18mm)
#set text(font: "Libertinus Serif", size: 11pt, dir: rtl)
#set par(justify: false)
#text(size: 17pt, weight: "bold")[דניאל כהן]
#v(2pt)
daniel\@example.com
#v(6pt)
#text(weight: "bold")[ניסיון תעסוקתי]
#v(3pt)
#text(weight: "bold")[מהנדס תוכנה בכיר] \
2019 – 2023
"#,
            must_recover: &[
                "daniel@example.com",
                "דניאל כהן",
                "מהנדס תוכנה בכיר",
                "2019",
                "2023",
            ],
        },
    ]
}
