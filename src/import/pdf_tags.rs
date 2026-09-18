//! A PDF read as a document rather than as a picture of one.
//!
//! Most of this is the tag tree: when a PDF carries a `/StructTreeRoot` it has
//! said, in the file, what each run of text *is* — a heading, a paragraph, a
//! list item, a cell. Word writes those tags. LinkedIn's own export writes
//! them. Typst writes them, so DockCV's does too. A reader that ignores them
//! and infers structure from where the glyphs landed is throwing away the
//! answer and then guessing it.
//!
//! The other readings here exist for the conformance harness, which asks a
//! different question — not "what does this file say" but "do the several ways
//! of reading it agree".
//!
//! The four ways a résumé PDF gets read, in process.
//!
//! An ATS does not have a PDF reader of its own. It has one of a small number
//! of extraction stacks underneath it, and each of them makes a different
//! decision about the one thing a page does not record: what order the words
//! are in. So "is this file parseable" is not a yes or no, it is the same file
//! read four ways and asked whether the four agree.
//!
//! | Personality | Who reads this way |
//! |---|---|
//! | [`content_order`] | a parser that trusts the file's own order — the cheapest thing anyone writes |
//! | [`sorted`] | `pdf-extract` (ours), pdfminer, PDFBox: glyphs sorted by where they landed |
//! | layout | `pdftotext -layout`, which keeps columns and is where two-column CVs go wrong — external, see `external.rs` |
//! | [`structure`] | a tag-aware reader: the document's own tree, not a guess from geometry |
//!
//! The fourth is the one nothing else in this category reads, and it is the
//! one that is actually correct: a tagged PDF records that a line *is* a
//! heading and which paragraphs belong under it. Typst writes those tags for
//! us. `structure` is therefore both half of the conformance harness and the
//! mechanism the importer will use on other people's tagged files (B5).

use std::collections::BTreeMap;
#[cfg(test)]
use std::panic::{catch_unwind, AssertUnwindSafe};

use lopdf::content::Content;
use lopdf::{Dictionary, Document, Object, ObjectId};

/// One structure element's own text, with its tag and how deep it sits.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tagged {
    /// The PDF structure type: `H1`…`H6`, `P`, `L`, `LI`, `Lbl`, `LBody`,
    /// `Span`, `Link`, `Figure`, `Div`.
    pub tag: String,
    pub depth: usize,
    pub text: String,
}

/// What a parser that trusts the file's own order sees.
///
/// This is the strictest of the three: nothing is sorted, nothing is inferred,
/// the text arrives in the order the page paints it. A layout that reads
/// correctly here reads correctly everywhere.
#[cfg(test)] // the conformance harness's reading, not the importer's
pub fn content_order(pdf: &[u8]) -> Result<String, String> {
    let doc = load(pdf)?;
    let pages: Vec<u32> = doc.get_pages().keys().copied().collect();
    doc.extract_text(&pages)
        .map_err(|e| format!("content-order extraction failed: {e}"))
}

/// What a parser that sorts glyphs by position sees — ours, pdfminer's,
/// PDFBox's default.
///
/// `pdf-extract` answers constructs it does not handle by panicking rather
/// than by returning an error (see `import/engines/pdf.rs` for the full note),
/// so the call is caught the same way the importer catches it.
#[cfg(test)] // the conformance harness's reading, not the importer's
pub fn sorted(pdf: &[u8]) -> Result<String, String> {
    catch_unwind(AssertUnwindSafe(|| pdf_extract::extract_text_from_mem(pdf)))
        .map_err(|_| "sorted extraction panicked inside pdf-extract".to_string())?
        .map_err(|e| format!("sorted extraction failed: {e}"))
}

/// What a tag-aware reader sees: the document's own structure tree, in order,
/// each element carrying the text that belongs to it.
///
/// Returns an empty vector for an untagged PDF — that is not an error, it is
/// the answer, and it is what tells the importer to fall back to inference.
pub fn structure(pdf: &[u8]) -> Result<Vec<Tagged>, String> {
    let doc = load(pdf)?;
    let root = match doc
        .catalog()
        .ok()
        .and_then(|c| c.get(b"StructTreeRoot").ok())
    {
        Some(root) => root.clone(),
        None => return Ok(Vec::new()),
    };

    // Every page's marked content, keyed by the id the structure tree refers
    // to it with. Built once: a CV is kilobytes and the tree points back into
    // the same pages many times over.
    let mut marked: BTreeMap<(ObjectId, u32), String> = BTreeMap::new();
    for (_, page_id) in doc.get_pages() {
        collect_marked(&doc, page_id, &mut marked);
    }

    let mut out = Vec::new();
    walk(&doc, &root, 0, None, &marked, &mut out);
    Ok(out)
}

/// [`structure`] rendered the way the other personalities return their
/// reading, one element to a line, so the harness can compare like with like.
#[cfg(test)] // the conformance harness's reading, not the importer's
pub fn structure_text(pdf: &[u8]) -> Result<String, String> {
    Ok(structure(pdf)?
        .iter()
        .filter(|t| !t.text.trim().is_empty())
        .map(|t| t.text.trim().to_string())
        .collect::<Vec<_>>()
        .join("\n"))
}

fn load(pdf: &[u8]) -> Result<Document, String> {
    Document::load_mem(pdf).map_err(|e| format!("this is not a PDF we can open: {e}"))
}

/// Follow a reference, or hand back what was already a direct object.
///
/// A dangling reference reads as itself rather than as an error: a structure
/// tree with one broken link is still worth the rest of its text, and the
/// harness's job is to report what a parser recovers, not to refuse the file.
fn deref<'a>(doc: &'a Document, obj: &'a Object) -> &'a Object {
    doc.dereference(obj).map(|(_, o)| o).unwrap_or(obj)
}

/// Walk one page's content stream and bucket every run of text under the
/// innermost open marked-content id.
///
/// The text operators are the four of PDF 32000-1 §9.4.3; `'` and `"` also
/// break the line, which for extraction is all they do that matters here.
fn collect_marked(doc: &Document, page_id: ObjectId, out: &mut BTreeMap<(ObjectId, u32), String>) {
    let Ok(fonts) = doc.get_page_fonts(page_id) else {
        return;
    };
    let encodings: BTreeMap<Vec<u8>, _> = fonts
        .into_iter()
        .filter_map(|(name, font)| font.get_font_encoding(doc).ok().map(|enc| (name, enc)))
        .collect();
    let Ok(content) = Content::decode(&doc.get_page_content(page_id)) else {
        return;
    };

    let properties = doc
        .get_page_resources(page_id)
        .ok()
        .and_then(|(res, _)| res)
        .and_then(|res| res.get(b"Properties").ok())
        .and_then(|obj| deref(doc, obj).as_dict().ok())
        .cloned()
        .unwrap_or_default();

    // A stack rather than a single value: marked content nests, and the id in
    // force is the innermost one that has an id at all.
    let mut open: Vec<Option<u32>> = Vec::new();
    let mut encoding = None;

    for op in &content.operations {
        match op.operator.as_ref() {
            "BDC" => open.push(mcid_of(doc, op.operands.get(1), &properties)),
            "BMC" => open.push(None),
            "EMC" => {
                open.pop();
            }
            "Tf" => {
                encoding = op
                    .operands
                    .first()
                    .and_then(|o| o.as_name().ok())
                    .and_then(|name| encodings.get(name));
            }
            "Tj" | "TJ" | "'" | "\"" => {
                let Some(enc) = encoding else { continue };
                let Some(mcid) = open.iter().rev().find_map(|m| *m) else {
                    continue;
                };
                let text = show(enc, &op.operands);
                if !text.is_empty() {
                    out.entry((page_id, mcid)).or_default().push_str(&text);
                }
            }
            _ => {}
        }
    }
}

/// The `/MCID` of a `BDC` property list, whether it is written inline or
/// named through the page's `/Properties`.
fn mcid_of(doc: &Document, operand: Option<&Object>, properties: &Dictionary) -> Option<u32> {
    let dict = match operand? {
        Object::Dictionary(d) => d.clone(),
        Object::Name(name) => deref(doc, properties.get(name).ok()?)
            .as_dict()
            .ok()?
            .clone(),
        Object::Reference(id) => doc.get_object(*id).ok()?.as_dict().ok()?.clone(),
        _ => return None,
    };
    dict.get(b"MCID").ok()?.as_i64().ok().map(|n| n as u32)
}

/// Decode one text-showing operator's operands with the font in force.
fn show(encoding: &lopdf::Encoding, operands: &[Object]) -> String {
    let mut out = String::new();
    for operand in operands {
        match operand {
            Object::String(bytes, _) => {
                if let Ok(text) = Document::decode_text(encoding, bytes) {
                    out.push_str(&text);
                }
            }
            Object::Array(items) => {
                for item in items {
                    if let Object::String(bytes, _) = item {
                        if let Ok(text) = Document::decode_text(encoding, bytes) {
                            out.push_str(&text);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

/// Walk the structure tree, emitting one [`Tagged`] per element that owns
/// text.
///
/// `/Pg` is inherited: an element that does not name a page belongs to the one
/// its parent named, which is how Typst writes a document whose sections do
/// not each start a page.
fn walk(
    doc: &Document,
    obj: &Object,
    depth: usize,
    page: Option<ObjectId>,
    marked: &BTreeMap<(ObjectId, u32), String>,
    out: &mut Vec<Tagged>,
) {
    match deref(doc, obj) {
        Object::Array(items) => {
            for item in items {
                walk(doc, item, depth, page, marked, out);
            }
        }
        Object::Dictionary(dict) => {
            let page = dict
                .get(b"Pg")
                .ok()
                .and_then(|o| match o {
                    Object::Reference(id) => Some(*id),
                    _ => None,
                })
                .or(page);

            let Ok(Object::Name(tag)) = dict.get(b"S") else {
                // Not a structure element — the root itself, or an /MCR.
                if let Ok(kids) = dict.get(b"K") {
                    walk(doc, kids, depth, page, marked, out);
                }
                return;
            };
            let tag = String::from_utf8_lossy(tag).to_string();

            let mut text = String::new();
            let mut children = Vec::new();
            if let Ok(kids) = dict.get(b"K") {
                gather(doc, kids, page, marked, &mut text, &mut children);
            }

            out.push(Tagged { tag, depth, text });
            for child in children {
                walk(doc, &child, depth + 1, page, marked, out);
            }
        }
        _ => {}
    }
}

/// Split one element's `/K` into the text it owns directly and the child
/// elements that own their own.
fn gather(
    doc: &Document,
    kids: &Object,
    page: Option<ObjectId>,
    marked: &BTreeMap<(ObjectId, u32), String>,
    text: &mut String,
    children: &mut Vec<Object>,
) {
    match deref(doc, kids) {
        Object::Integer(mcid) => {
            if let Some(found) = page.and_then(|p| marked.get(&(p, *mcid as u32))) {
                text.push_str(found);
            }
        }
        Object::Array(items) => {
            for item in items {
                gather(doc, item, page, marked, text, children);
            }
        }
        Object::Dictionary(dict) => {
            // A marked-content reference: text on a page, possibly another one.
            if dict.get(b"S").is_err() {
                let on = dict
                    .get(b"Pg")
                    .ok()
                    .and_then(|o| match o {
                        Object::Reference(id) => Some(*id),
                        _ => None,
                    })
                    .or(page);
                if let (Some(on), Ok(mcid)) = (on, dict.get(b"MCID").and_then(|o| o.as_i64())) {
                    if let Some(found) = marked.get(&(on, mcid as u32)) {
                        text.push_str(found);
                    }
                }
                return;
            }
            children.push(kids.clone());
        }
        _ => {}
    }
}

/// Every heading a tagged PDF declares, in document order.
///
/// Empty for an untagged file, which is the signal to fall back to inference:
/// a heading is a claim the document makes about itself, and where it makes
/// none there is nothing here to report.
pub fn headings(pdf: &[u8]) -> Vec<String> {
    structure(pdf)
        .unwrap_or_default()
        .into_iter()
        .filter(|t| {
            t.tag.len() == 2 && t.tag.starts_with('H') && t.tag.as_bytes()[1].is_ascii_digit()
        })
        .map(|t| t.text.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|t| !t.is_empty())
        .collect()
}
