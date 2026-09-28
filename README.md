<p align="center">
  <img src="assets/banner.png" width="150" alt="metadata'nt">
</p>

<h1 align="center">metadata'nt</h1>

<p align="center">
  <strong>Keep the file. Lose the trail.</strong>
</p>

<p align="center">
  A local-first desktop interface for <a href="https://github.com/jvoisin/mat2">MAT2</a>.<br>
  Inspect metadata, remove it, replace it with plausible decoys, or optionally plant an investigation tripwire.
</p>

---

## What is this?

MAT2 is already very good at removing metadata.

**metadata'nt sits on top of MAT2** and turns it into a small desktop workflow for people who would rather drag in a file than work from a terminal.

The cleaning is still MAT2. metadata'nt adds the interface, safer defaults, before/after inspection, batch processing, synthetic metadata and a few optional defensive tools around it.

No account. No subscription. No telemetry. No bullshit.

<p align="center">
  <img src="assets/screenshot-main.png" width="900" alt="metadata'nt main window">
</p>

## Who is it for?

- **Journalists & sources** — remove location, device and author metadata before sharing material.
- **Activists & whistleblowers** — reduce accidental identifying information in files.
- **Lawyers & researchers** — inspect and sanitise documents before disclosure or publication.
- **Photographers & creators** — see exactly what metadata leaves your machine.
- **OSINT & security people** — create controlled files with clean or synthetic metadata.
- **Anyone sending files directly** through email, messaging, cloud storage or file transfer.

## What it does

### Clean

Drag in files, inspect what MAT2 can detect and process them locally.

Two cleaning modes are available:

**Maximum removal**  
MAT2's normal cleaning mode. Removes as much metadata as possible, but may modify internal file structure or quality where necessary.

**Lightweight**  
Preserves more of the original file data, at the cost of potentially leaving some metadata behind.

### Plant plausible metadata

After MAT2 has cleaned a file, metadata'nt can optionally add a small, internally consistent synthetic profile:

- creation / modification dates
- software
- device or camera information where appropriate
- synthetic identity/location data where supported

Synthetic metadata is written **only after cleaning** and never to the original file.

### Investigation Tripwire

Synthetic profiles can optionally contain a Canarytokens reference.

If somebody follows that reference, Canarytokens may generate an alert.

This is a **signal, not attribution**: the URL may be recognisable to an experienced analyst and automated scanners can also trigger it.

Using this feature contacts `canarytokens.org`.

## Basic use

1. Drop a file or folder into metadata'nt.
2. Select the file to inspect detected metadata.
3. Choose **Maximum removal** or **Lightweight**.
4. Choose where cleaned files should be saved.
5. Press **Process files**.
6. Compare the original and processed metadata.
7. Use **Reveal output** when you're done.

That's basically it.

Advanced settings contain archive handling, synthetic metadata, the optional tripwire, diagnostics and destructive in-place replacement.

## Privacy model

Normal file inspection and cleaning happen locally on your machine.

metadata'nt does not upload your files for processing.

Two optional features can create network connections:

- **Update checks** contact GitHub, if you enable them. This exposes your IP address to GitHub.
- **Investigation Tripwire** contacts Canarytokens.org when explicitly enabled.

Neither feature sends the document itself.

## A note about metadata

“No metadata detected” does **not** mean mathematically proven metadata-free.

Complex file formats can contain information that MAT2 — or any metadata tool — does not know how to detect.

metadata'nt deliberately follows MAT2's conservative wording here.

If your threat model involves hostile documents rather than identifying metadata, take a look at [Dangerzone](https://dangerzone.rocks/) as well.

## Built on MAT2

The actual metadata sanitisation engine is [MAT2 — Metadata Anonymisation Toolkit 2](https://github.com/jvoisin/mat2), created by **Julien Voisin** with support from the **Tails project**.

metadata'nt does not reimplement MAT2's cleaners. It provides a desktop interface and an additional workflow around them.

MAT2 is distributed under the **GNU Lesser General Public License v3.0 or later (LGPL-3.0-or-later)**.

This project is independent from MAT2 and is not an official MAT2 frontend.

## License

metadata'nt is free software released under the **GNU General Public License v3.0 (GPL-3.0)**.

See [`LICENSE`](LICENSE).

Bundled third-party components remain under their respective licenses. See the project's third-party notices for details.

---

<p align="center">
  <strong>No subscriptions, no telemetry, no bullshit.</strong>
</p>
