Libertinus Serif, three faces, copied verbatim from the `typst-assets` crate
(v0.15.0, files/fonts/).

Here because the browser build drops that crate's font pack — 9.2 MB, most of
it maths faces a résumé never sets — and `DocumentFont::LibertinusSerif` is the
*default* document font. Without these three the web demo would silently render
a different face from the app for the same input, which is exactly the failure
`docs/OPEN.md` L-11 is about.

Licence: SIL Open Font License 1.1, unchanged. The full text and the copyright
notice ship in `../NOTICE-typst-assets.txt`, which already names these files.
