// Does the module actually typeset, or does it merely link?
//
//   scripts/wasm.sh --node && node crates/dockcv-wasm/smoke.mjs
//
// The question worth asking of a wasm build: the first one produced during
// this work was 0 bytes, because with nothing exported the linker dead-strips
// the whole engine and a green build says nothing.
import { createRequire } from "module";
const require = createRequire(import.meta.url);
const m = require("../../dist/web/dockcv_wasm.js");

const started = Date.now();
const pages = m.render(m.sample());
const ms = Date.now() - started;

const ok = (label, cond) => {
  console.log(`${cond ? "ok  " : "FAIL"} ${label}`);
  if (!cond) process.exitCode = 1;
};

ok(`one page`, pages.length === 1);
ok(`it is SVG`, pages[0].startsWith("<svg"));
ok(`with content (${(pages[0].length / 1024) | 0} KB)`, pages[0].length > 10_000);
ok(`compiled in ${ms} ms`, ms < 5_000);
ok(`reports a version (${m.version()})`, /^\d+\.\d+\.\d+$/.test(m.version()));

let rejected = false;
try { m.render("not a résumé in any format"); } catch { rejected = true; }
ok(`nonsense is refused, not rendered`, rejected);

// The PDF path: what "Download CV" actually hands over.
const pdf = m.render_pdf(m.sample());
ok(`PDF produced (${(pdf.length / 1024) | 0} KB)`, pdf.length > 1000);
ok(`it is a PDF`, String.fromCharCode(...pdf.slice(0, 4)) === "%PDF");
