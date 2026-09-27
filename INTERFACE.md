# MAT2 Tauri Wrapper — Interface & Workflow Specification

## 1. UX goal

The application should feel like a small utility, not a security dashboard.

A non-technical user should understand the workflow without reading documentation:

```text
Choose files
    ↓
See what MAT2 detects
    ↓
Choose cleaning mode
    ↓
Process
    ↓
See what changed
    ↓
Reveal cleaned copies
```

One window.
Dark.
Compact.
No navigation tree.
No preview.
No unnecessary screens.

---

## 2. Main window

Recommended starting size:

```text
1040 × 660 px
```

Reasonable minimum:

```text
900 × 560 px
```

Resizable.

Use one system WebView/window.

High-level layout:

```text
┌──────────────────────────────────────────────────────────────────────────┐
│ MAT2 Wrapper                                      MAT2 status: Ready     │
├────────────────────────────────┬─────────────────────────────────────────┤
│                                │                                         │
│ FILES                          │ INSPECTION / RESULT                     │
│                                │                                         │
│ [ + Files ] [ + Folder ]       │ Selected: report.docx                  │
│ Drop files here                │                                         │
│                                │ BEFORE             AFTER                │
│ ☑ photo.jpg        Ready       │ Author: Alice   →  Removed             │
│ ☑ report.docx      Ready       │ Creator: Word   →  Removed             │
│ ☐ clip.mp4         Ready       │ Created: ...    →  Remaining           │
│                                │                                         │
│ JPG 12  PNG 4  PDF 3  ALL 19   │ -------------------------------------   │
│                                │ Process details                         │
│ SETTINGS                       │ [20:41:03] Inspecting report.docx       │
│ ● Maximum removal              │ [20:41:04] Cleaning                    │
│ ○ Lightweight                  │ [20:41:05] Verifying                   │
│                                │                                         │
│ Output: Beside source          │                                         │
│ [Advanced ▾]                   │                                         │
│                                │                                         │
│ [ PROCESS 18 FILES ]           │                                         │
├────────────────────────────────┴─────────────────────────────────────────┤
│ MAT2 · Dangerzone · PrivacyTools                    No telemetry          │
└──────────────────────────────────────────────────────────────────────────┘
```

Suggested proportion:

- left panel: ~43%;
- right panel: ~57%.

The comparison/result area gets more room than the settings because the primary value is understanding the result.

---

## 3. Visual language

Dark interface by default in v1.

Use system fonts.

Suggested semantics:

- background: near-black;
- cards/panels: charcoal;
- borders: subtle neutral gray;
- primary accent: muted violet/blue;
- success: restrained green;
- warning: amber;
- error/destructive: restrained red;
- neutral/unchanged: gray.

Do not use bright “hacker” aesthetics.
Do not use animated scanlines.
Do not use fake terminal chrome.

The log is technical text, not decoration.

---

## 4. Top bar

Minimal:

```text
MAT2 Wrapper
MAT2 status: Ready / Problem
```

Optional small version information:

```text
MAT2 0.x.x
App 0.x.x
```

Version comes from actual runtime diagnostics, not a duplicated hard-coded assumption where avoidable.

No hamburger menu is required for v1.

---

## 5. File acquisition

Three input methods:

### Drag and drop

Drop files into the file panel.

### + Files

Native multi-file picker.

### + Folder

Native directory picker.

Folder selection builds a candidate batch.

---

## 6. Folder batch behavior

After choosing a folder:

1. enumerate regular files safely;
2. do not follow symlinked directories;
3. present extension groups;
4. initially select normal candidate files;
5. allow group toggle by extension;
6. allow per-file checkbox override.

Example:

```text
ALL 147   JPG 82   PNG 21   PDF 14   DOCX 8   MP4 4   OTHER 18
```

These are only filters.

Do not claim an extension is safe/supported solely because it appears.

If MAT2 later rejects it:

```text
Unsupported / Not processed
```

is a valid result.

---

## 7. File row

Each row should include only:

```text
[checkbox] filename.ext        type/status
```

Possible status labels:

```text
Ready
Inspecting
Queued
Processing
Verifying
Processed
Warning
Failed
Unsupported
Cancelled
```

No thumbnails.

A file that appears to have already been processed, e.g. `.cleaned` naming or a known MAT2 Output directory, may show a non-blocking warning:

```text
Appears previously processed
```

Do not refuse it automatically.

---

## 8. Selected-file inspection

When a user selects a file row, the right panel shows MAT2-detected metadata.

Before cleaning:

```text
report.docx

Metadata detected by MAT2

Author          Alice Example
Creator         Microsoft Word
Created         2026-09-27T...
Modified        ...
...
```

The wrapper should not invent categories unless they are purely presentational.

Where possible, preserve the exact key names emitted/detected by MAT2 and normalize only for safe display.

---

## 9. Cleaning mode

Main settings expose two choices:

```text
● Maximum removal
○ Lightweight / preserve more structure
```

Mapping must be derived from the supplied MAT2 CLI.

At the time of design:

```text
Maximum removal  → normal MAT2 cleaning
Lightweight      → MAT2 lightweight mode
```

Helper text:

### Maximum removal

> Use MAT2's normal cleaning behavior. This may alter internal structure or quality where MAT2 considers that necessary to remove metadata.

### Lightweight

> Preserve more of the original structure. MAT2 may leave more metadata behind.

Do not call either setting “safe” or “unsafe”.

---

## 10. Inspection-only mode

Advanced or secondary action:

```text
Inspect only
```

Runs MAT2 inspection/show behavior without cleaning.

It updates the right panel and technical log.

It does not produce an output file.

---

## 11. Output control

Default mode:

```text
Output
● Beside source
○ Custom folder
```

### Beside source

For each source parent:

```text
<source parent>/
  MAT2 Output/
    YYYY-MM-DD_HHmmss/
```

A folder batch uses the source folder as root.

If a batch includes files from multiple unrelated directories, each parent may receive its own timestamped MAT2 Output directory.

### Custom folder

User chooses a root with native folder picker:

```text
<chosen root>/
  YYYY-MM-DD_HHmmss/
```

Do not encode the full original absolute path into filenames.

---

## 12. Advanced settings

Collapsed by default.

Every option that exists in the supplied MAT2 CLI must be accounted for in one of three ways:

1. exposed as a processing option;
2. exposed as a diagnostic/action;
3. explicitly documented as deprecated/non-applicable.

The implementation agent must generate an option-parity checklist from the **actual supplied `mat2 --help`**.

Likely categories include:

### Verbose output

Checkbox:

```text
Verbose MAT2 output
```

### Unknown archive members

Dropdown:

```text
Unknown members in archives
[ Abort (recommended) ▾ ]
  Abort
  Omit
  Keep
```

If `Keep` is selected, show:

> Keeping unknown archive members may preserve metadata.

### In-place mode

If the supplied MAT2 version still supports it:

```text
Replace original files
```

Off by default every launch.

Activating requires a warning.

Starting a job requires a second explicit destructive confirmation.

Text:

> This changes the selected originals. No wrapper-created backup is guaranteed.

When active:

- hide/disable output location;
- do not stage to a different final copy as though it were normal mode;
- verify the modified file afterward.

### Diagnostics

Buttons/actions:

```text
Show MAT2 version
List supported formats
Check dependencies
Show MAT2 help
```

All output appears in Process details.

If the supplied upstream includes obsolete compatibility flags that no longer alter behavior, document rather than expose meaningless toggles.

---

## 13. Process button

Button label reflects count:

```text
PROCESS 1 FILE
PROCESS 18 FILES
```

Disabled when:

- nothing selected;
- MAT2 runtime diagnostics are fatally broken;
- a job is already running.

During a job:

```text
CANCEL
```

appears.

---

## 14. Processing pipeline

For normal non-in-place cleaning:

```text
1. PRE-INSPECT
   MAT2 inspects the selected source.

2. STAGE
   Create a private job workspace.
   Copy source bytes into a regular staging file without interpreting content.

3. CLEAN
   Invoke MAT2 with the selected upstream options.
   No shell.

4. BASIC VALIDATION
   Expected output exists.
   Output is a regular file.
   Output is non-empty.
   Process status is acceptable.

5. POST-INSPECT
   MAT2 inspects the produced output.

6. DIFF
   Compare MAT2's before/after detected metadata.

7. COMMIT
   Move/rename the verified staged result into its final output location.

8. PRESENT
   Show before/after and final status.
```

Process files sequentially in v1.

---

## 15. Before/after comparison

This is a core interface feature.

After processing, the selected result should look approximately like:

```text
report.docx                      PROCESSED

BEFORE                           AFTER
────────────────────────────────────────────────────────
Author      Alice Example   →    Removed
Creator     Word            →    Removed
Created     2026-...        →    Removed
Modified    2026-...        →    2026-...
CustomTag   foo             →    Remaining
```

Status vocabulary:

```text
Removed
Changed
Remaining
Not detected
```

Avoid:

```text
Low Risk
High Risk
Safe
Anonymous
Spoofed
```

unless a future design introduces an independently justified model.

Footer summary:

```text
Detected before: 23
Removed: 20
Changed: 2
Still detectable by MAT2: 1
```

If nothing is detected afterward:

```text
No metadata detectable by MAT2
```

Add subdued note:

> This does not guarantee that every possible form of metadata is absent.

---

## 16. Technical process log

Keep the live log visible in the right panel below the semantic result, or in a vertically constrained section inside the same panel.

Example:

```text
Process details

[20:41:03] Queued report.docx
[20:41:03] Inspecting source with MAT2
[20:41:04] Creating private staging copy
[20:41:04] Running MAT2
[20:41:05] MAT2 output created
[20:41:05] Verifying output with MAT2
[20:41:06] Committed output
[20:41:06] Result: 0 metadata fields detectable by MAT2
```

When verbose mode is enabled, include sanitized MAT2 stdout/stderr.

The UI log:

- is not interactive;
- does not accept commands;
- does not interpret ANSI;
- does not convert output into links;
- is not written to disk by default;
- is cleared when the application exits.

---

## 17. Result states

### Processed

Use when:

- cleaning command completed;
- expected output was produced;
- output passed wrapper sanity checks;
- post-inspection completed;
- wrapper has a valid before/after result.

### Warning

Use when:

- output exists and completed;
- MAT2 still detects metadata;
- a non-fatal upstream warning matters;
- format limitations are reported.

Example:

```text
Processed with warning
MAT2 still detects 2 metadata fields
```

### Failed

Use when:

- MAT2 returns failure;
- output missing;
- output invalid;
- verification cannot complete;
- path/output commit fails.

Never display green success for an unverified output.

---

## 18. Reveal output

Successful job shows:

```text
REVEAL OUTPUT
```

Frontend passes only a known result/job ID.

Rust resolves the associated approved output directory.

Do not expose a generic “open arbitrary path” command.

---

## 19. External-resource footer

Always visible and understated:

```text
MAT2 · Dangerzone · PrivacyTools
```

Behavior:

- opens the exact hard-coded URL in the user's default system browser;
- no in-app browsing;
- no UTM;
- no tracking parameter;
- no app-generated identifier.

Destinations:

```text
MAT2
https://github.com/jvoisin/mat2

Dangerzone
https://github.com/freedomofpress/dangerzone

PrivacyTools
https://www.privacytools.io/
```

Optional tooltip for Dangerzone:

> For documents you do not trust and may be malicious.

---

## 20. Empty state

The initial state should contain almost no explanation:

```text
Drop files or folders here

[ Choose Files ]   [ Choose Folder ]

Files stay on this device.
Cleaning is performed locally with MAT2.
```

Do not turn the landing area into documentation.

---

## 21. Error presentation

Errors should be specific.

Good:

```text
report.pdf
Processing failed

MAT2 could not process this file.
No cleaned output was committed.

[Show process details]
```

Bad:

```text
Something went wrong.
```

Dependency error:

```text
MAT2 runtime is incomplete
PDF support dependency is unavailable.
Processing is disabled until diagnostics pass.
```

Do not create endless modal dialogs.

Prefer inline errors.

---

## 22. Destructive mode UX

If upstream in-place mode is available:

First toggle warning:

```text
Replace original files?

Normal mode preserves the original and produces a cleaned copy.
In-place mode changes the selected original files.
```

Then on Process:

```text
Confirm destructive processing of 4 original files

[Cancel] [Replace originals]
```

No “don't ask again”.

Reset to OFF when the app restarts.

---

## 23. Accessibility and keyboard basics

Even though the UI is minimal:

- visible focus state;
- buttons keyboard reachable;
- no color-only status meaning;
- text labels accompany icons;
- semantic HTML controls;
- compare table readable without animation;
- sufficient contrast.

---

## 24. UI non-goals

Do not add in v1:

- sidebar navigation;
- onboarding wizard;
- file previews;
- thumbnails;
- drag-to-reorder;
- rich charts;
- risk dial;
- spoof editor;
- watch-folder screen;
- settings database;
- theme chooser;
- multi-window compare screen;
- embedded web pages;
- interactive terminal.

The single-window workflow is the product.
