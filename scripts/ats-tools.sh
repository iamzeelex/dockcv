#!/usr/bin/env bash
# Fetch the external extractors the ATS conformance harness reads a PDF with.
#
# These are dev-time tools, not dependencies of the app: DockCV still ships a
# pure-Rust binary that needs nothing installed. They are here because the
# claim the harness makes — "the stacks half the market runs on read this file
# the same way" — cannot be checked with our own reader alone.
#
# Everything lands in `target/ats-tools/`, which is derived, rebuildable and
# already gitignored. Deleting it costs one run of this script.
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
TOOLS="$ROOT/target/ats-tools"
# Pinned: an extractor that changes under us turns a regression into a mystery.
PDFBOX_VERSION="3.0.4"
PDFMINER_VERSION="20250506"
# A .docx is the other file a CV gets sent as, and several ATS parse it better
# than they parse a PDF. `python-docx` reads paragraph *styles*, which is what
# a parser keys on when it looks for a heading; `docx2txt` is the crude end of
# the same market and reads nothing but the runs.
PYTHON_DOCX_VERSION="1.2.0"
DOCX2TXT_VERSION="0.9"

mkdir -p "$TOOLS"

say() { printf '\033[1m%s\033[0m\n' "$*"; }

# 1. poppler's pdftotext — the layout-preserving personality, and the one
#    shell-pipeline parsers use.
if command -v pdftotext >/dev/null 2>&1; then
  say "pdftotext: $(pdftotext -v 2>&1 | head -1)"
elif [ "$(uname)" = "Darwin" ] && command -v brew >/dev/null 2>&1; then
  say "installing poppler via Homebrew"
  brew install poppler
elif command -v apt-get >/dev/null 2>&1; then
  say "installing poppler-utils"
  sudo apt-get update -qq && sudo apt-get install -y -qq poppler-utils
else
  echo "pdftotext missing and no package manager found — install poppler yourself" >&2
fi

# 2. pdfminer.six — what Python-side parsers read with.
VENV="$TOOLS/venv"
if [ ! -x "$VENV/bin/pdf2txt.py" ]; then
  say "creating venv and installing pdfminer.six==$PDFMINER_VERSION"
  python3 -m venv "$VENV"
  "$VENV/bin/pip" install -q --disable-pip-version-check "pdfminer.six==$PDFMINER_VERSION"
fi
if [ ! -f "$VENV/lib/.docx-readers" ]; then
  say "installing python-docx==$PYTHON_DOCX_VERSION and docx2txt==$DOCX2TXT_VERSION"
  "$VENV/bin/pip" install -q --disable-pip-version-check \
    "python-docx==$PYTHON_DOCX_VERSION" "docx2txt==$DOCX2TXT_VERSION"
  touch "$VENV/lib/.docx-readers"
fi
say "pdfminer: $("$VENV/bin/python" -c 'import pdfminer; print(pdfminer.__version__)')"
say "python-docx: $("$VENV/bin/python" -c 'import docx; print(docx.__version__)' 2>/dev/null || echo unknown)"

# 3. Apache PDFBox — the Java stack under Tika and under a large share of
#    in-house ATS parsing.
JAR="$TOOLS/pdfbox-app-$PDFBOX_VERSION.jar"
if [ ! -f "$JAR" ]; then
  say "downloading pdfbox-app-$PDFBOX_VERSION.jar"
  curl -fsSL -o "$JAR" \
    "https://repo1.maven.org/maven2/org/apache/pdfbox/pdfbox-app/$PDFBOX_VERSION/pdfbox-app-$PDFBOX_VERSION.jar"
fi
if command -v java >/dev/null 2>&1; then
  say "pdfbox: $(java -jar "$JAR" version 2>&1 | head -1)"
else
  echo "java missing — PDFBox will be skipped by the harness" >&2
fi

say "ats tools ready in $TOOLS"
