# Security

DockCV runs on your machine, keeps your documents in a folder you chose, and
makes no network request you did not ask for. Most of what usually goes wrong
with an application does not apply here, and the things that can go wrong are
therefore specific.

## Reporting something

Use GitHub's private reporting form — the **Security** tab, then *Report a
vulnerability*. It opens a discussion only you and the maintainer can read, which
is the right place for anything that could hurt someone before it is fixed.

Please do not open a public issue for a vulnerability. A public issue is a
working exploit with a title.

There is one maintainer, so an answer takes days rather than hours. You will get
an acknowledgement that a person read it, an assessment once there is one, and
credit in the release notes unless you would rather not be named.

## What is worth reporting

The threat model is narrow, and these are the parts of it that matter:

- **Anything that writes outside the vault**, or writes to a path assembled from
  content in a document. Import reads files from strangers; a crafted CV that
  escapes the folder it was imported into is the most serious bug this project
  could have.
- **Anything that makes the application talk to the network** other than the
  update check in `src/update.rs`, which is off by default, fetches one static
  file, and sends nothing about the user.
- **Anything that puts document content into the log.** The log is written to be
  attachable to a bug report: it records what the app did and never what you
  wrote, and the home directory is rewritten as `~`. A leak there travels.
- **Code execution through a document.** The Typst compiler runs in-process on
  generated source. If a value from a CV can reach that source unescaped and
  change what is compiled, say so.
- **Anything in the update path** that could point a person at a download that is
  not ours.

## What is not a vulnerability

- The macOS build is signed ad-hoc and not notarised, so the first launch needs
  the *Open Anyway* step. That is a consequence of having no paid Apple developer
  account and is documented rather than hidden.
- Your vault is not encrypted. It is a folder of plain text files, on purpose:
  you can read them without this application, and that is the whole promise.
  Anyone who can read your home directory can read your CV — as they can read
  everything else in it. Use full-disk encryption, which your operating system
  already offers.
- Dependency advisories with no path to exploitation here. `cargo-audit` and
  `cargo-deny` run in CI, and an advisory that fires is looked at, but a report
  that consists of their output is a bot's work rather than a finding.

## Supported versions

The newest release. This is early software with one maintainer; there is no
backporting, and the fix for anything found today will ship in the next version.
