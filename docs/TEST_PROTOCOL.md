# MAT2 Wrapper — Manual QA / Test Protocol (v1)

Companion to the automated suites (`cargo test`, `npm run lint:all`).
Manual tests exercise the real GUI + real MAT2 runtime on macOS.

**Status: protocol defined; results to be recorded in the QA session against
the release candidate (Task 24 checklist is executed in the same session).**

## Environment

| Item | Value |
|---|---|
| Machine | macOS 27.0 (26A428), arm64 |
| MAT2 upstream | 0.15.0 @ `70c17d3d7b2835e02c7177c6cc58f1911848583c` (see `docs/UPSTREAM_SNAPSHOT.md`; verify with `scripts/verify-upstream.sh`) |
| Runtime | `<project>/.venv/bin/python` + `upstream-mat2/mat2` (dev build); packaged runtime for RC (Task 19) |
| Build under test | `cd app && npm run tauri dev` (dev) or the `.app` from `npm run tauri build` (RC) |
| Automated evidence at protocol creation | cargo test **84/84**; lint:all (no-remote-refs, ipc-surface, no-html-sinks) **PASS**; upstream MAT2 suite **146/147** (1 ffmpeg-version-pinned mp4 failure, 1 upstream skip) |

## QA fixture kit

Located at `.omo/tmp/qa-fixtures/` (NOT committed; regenerate per below).
The app itself never downloads anything — fixtures are fetched by the tester.

| Fixture | Origin | SHA-256 |
|---|---|---|
| `DSCN0010.jpg` (Test A) | https://github.com/ianare/exif-samples/raw/master/jpg/gps/DSCN0010.jpg | `17307b1207eb6487d7908e9d154890b46e3d2e0192369cfd3f4c33d5a5af4035` |
| `batch/a.jpg`, `batch/b.jpg` (Test C) | copies of `upstream-mat2/tests/data/dirty.jpg` | `eee5061debc152851ffaf6b092ac768f1a51864fbf97a2628ef3fecefbc29be1` |
| `batch/c.png` | copy of `dirty.png` | `15e7da4dc117396255d2691b08738acff37d02b4c52183c826186c8189519b10` |
| `batch/d.pdf` | copy of `dirty.pdf` | `d69c2681d336cffa99b0ba0283a581991537ebe750548c3b4aff1384e44879cb` |
| `batch/ignore.txt` | copy of `dirty.txt` | `f83950e6975f033c9f2ddbf6367a930b0d126f063995352710c3e6a032c97a4c` |
| `testB.pdf` (Test B) | copy of `dirty.pdf` | `d69c2681d336cffa99b0ba0283a581991537ebe750548c3b4aff1384e44879cb` |

Pre-recorded Test A inputs (captured 2026-09-28, before any wrapper run):

- MAT2 pre-inspection: `.omo/tmp/qa-fixtures/testA_pre_mat2.txt` (106 lines —
  GPS lat/lon `43°28'2.81"N 11°53'6.46"E`, Make NIKON, Model COOLPIX P6000,
  Software "Nikon Transfer 1.1 W", CreateDate/ModifyDate, thumbnail, XMP toolkit…)
- Observational ExifTool pre-inspection: `.omo/tmp/qa-fixtures/testA_pre_exiftool.txt`
  (ExifTool 13.55; observer only — never a product sanitiser)

---

## Test A — public image with metadata (JPEG + GPS)

1. Source URL + SHA-256 recorded (table above). ☐
2. MAT2 pre-inspection recorded (`testA_pre_mat2.txt`). ☐
3. Optional observational ExifTool output recorded (`testA_pre_exiftool.txt`). ☐
4. In the app: add `DSCN0010.jpg` (drag or + Files). Row appears, status Ready. ☐
5. Select the row → right panel lists MAT2-detected metadata (GPS*, Make, Model…). ☐
6. PROCESS 1 FILE → statuses walk Queued→Inspecting→Processing→Verifying→Processed. ☐
7. Before/after comparison shows GPS/Make/Model/etc. as **Removed**; summary
   reads "No metadata detectable by MAT2" + epistemic note. ☐
8. REVEAL OUTPUT opens Finder at `<fixtures>/MAT2 Output/<ts>/`. ☐
9. Manual MAT2 post-inspection of the committed output:
   `.venv/bin/python upstream-mat2/mat2 -s -- "<output>"` → "No metadata found". ☐
10. Optional observational ExifTool post-inspection of output recorded. ☐
11. Original SHA-256 after test **unchanged**:
    `shasum -a 256 DSCN0010.jpg` == value in table. ☐

## Test B — PDF with metadata

1. Add `testB.pdf`; inspect → metadata listed (Author/Creator/dates…). ☐
2. Process → expected **Warning** "MAT2 still detects 3 metadata fields"
   (0.15.0 keeps structural `creation-date:-1`, `format:PDF-1.5`, `mod-date:-1`
   by design — UI must not overclaim). ☐
3. Source `testB.pdf` SHA unchanged after run. ☐
4. Committed output opens in Preview/PDF viewer; renders normally. ☐
5. Manual MAT2 post-inspection shows only the 3 structural fields. ☐

## Test C — folder batch

Fixture: `.omo/tmp/qa-fixtures/batch/` (a.jpg, b.jpg, c.png, d.pdf, ignore.txt).

1. "+ Folder" → choose `batch/` → 5 rows, extension chips `ALL 5 JPG 2 PNG 1 PDF 1 TXT 1`. ☐
2. Group toggles: filter JPG → "Uncheck group" → JPG rows unchecked; ALL → count reflects 3. ☐
3. Individual uncheck: uncheck `ignore.txt` (also demonstrates TXT processing if re-checked). ☐
4. PROCESS 3 FILES → sequential statuses (one file at a time through the ladder). ☐
5. Output layout: `batch/MAT2 Output/<YYYY-MM-DD_HHmmss>/` contains exactly the
   committed outputs (`a.cleaned.jpg`, `b.cleaned.jpg`, `c.cleaned.png`, `d.cleaned.pdf`
   per checked set; d.pdf → Warning). ☐
6. Originals in `batch/` byte-identical (SHA table above). ☐

## Test D — cancellation

1. Batch of ≥4 files (Test C kit + DSCN0010.jpg). Start job. ☐
2. CANCEL during the 2nd file. ☐
3. Already-committed outputs survive in the output dir. ☐
4. Current file → Cancelled; later files → Cancelled (never start). ☐
5. No staging leftovers: `ls /var/folders/**/mat2-wrapper-job-* 2>/dev/null` empty
   (or `$TMPDIR` check). ☐

## Test E — destructive in-place mode (Task 13 rules)

1. Advanced → "Replace original files" → modal appears with INTERFACE §22 copy;
   Cancel → checkbox stays OFF. ☐
2. Enable → amber warning visible; Output fieldset disabled. ☐
3. Process a COPY fixture (`cp batch/a.jpg /tmp/inplace-test.jpg`) → second
   modal "Confirm destructive processing of 1 original files" [Cancel][Replace
   originals]; Cancel → nothing runs. ☐
4. Confirm → file modified in place; status Processed/Warning; diff shown;
   NO "MAT2 Output" directory created. ☐
5. After job: toggle auto-resets to OFF (arm consumed). ☐
6. **Stale-state check**: enable in-place, quit app, relaunch → toggle OFF;
   PROCESS with normal settings works; a crafted `start_clean_job(inplace:true)`
   without arming is rejected (dev-tools/console invoke or code review ref:
   `lib.rs` arm gate + `set_inplace_armed`). ☐

## Test F — runtime diagnostics gating (Task 15)

1. Healthy: top bar "MAT2 status: Ready" + "mat2 0.15.0 · App 0.1.0". ☐
2. Advanced → all four diagnostics buttons print sane output into Process details. ☐
3. Missing runtime: launch with `MAT2_WRAPPER_PYTHON=/nonexistent npm run tauri dev`
   → status "Problem", inline reason, **PROCESS disabled**, app stays open. ☐
4. Wrong executable: `MAT2_WRAPPER_PYTHON=/bin/echo` → Problem (version query
   fails/garbage) → PROCESS disabled. ☐
5. Diagnostic output with control chars renders as sanitized plain text (no
   color codes, no links). ☐

## Test G — links, reveal, offline

1. Footer MAT2/Dangerzone/PrivacyTools open the exact hard-coded URLs in the
   default browser; address bar has **no** query/tracking parameters. ☐
2. REVEAL OUTPUT only appears after a job with committed outputs; opens the
   job's own output folder(s). ☐
3. Offline run: disable Wi-Fi → full Test C batch still works end-to-end. ☐

## Test H — synthetic metadata add-on (owner-approved extension)

Prereq: Advanced → "Add plausible decoy metadata (synthetic)" visible; toggle
OFF by default on every launch (verify after restart).

1. Toggle on → options groups appear (Profile behavior / Identity / Location /
   Technical / Serial); serial + GPS default to safe options. ☐
2. Select `DSCN0010.jpg` → "Preview synthetic profile" → coherent preview
   (archetype, device with plausible year, created date, timezone; no seed or
   selection id visible). Preview twice → different values (throwaway seed). ☐
3. Location → "Include synthetic GPS" → amber falsity warning appears. ☐
4. Process DSCN0010.jpg with synthetic on → row status walks …→ Synthetic
   writing → Processed; detail reads "Original identifying metadata removed.
   Synthetic metadata added and verified." ☐
5. Select the processed row → comparison table shows ORIGINAL/CLEANED/
   SYNTHETIC columns; synthetic values rendered in violet with "Synthetic"
   status; original GPS/camera values show Removed. ☐
6. Output verification (terminal):
   `resources/mat2-runtime/mat2-runtime mat2 -s -- "<output>"` → shows ONLY
   synthetic values; none of the original GPS/Make/Model/software values. ☐
7. Observational: exiftool on output shows decoy Make/Model/dates; no
   "Created with GIMP"-style originals; no seed fragments. ☐
8. Original DSCN0010.jpg SHA-256 unchanged. ☐
9. Batch mode: 2+ images, "Consistent profile for this batch" → preview/outputs
   share persona-device-timezone family; XMP InstanceIDs differ per file
   (exiftool -XMP-xmpMM:InstanceID on both outputs). ☐
10. Mixed batch: image + `ignore.txt` → txt Processed with "Synthetic mode
    unavailable for this format" note; image synthetic-verified. ☐
11. In-place conflict: enable in-place → synthetic checkbox disables (and
    vice versa); attempting both via crafted IPC is rejected by the backend
    (code ref: start_clean_job guard). ☐
12. Restart app → synthetic toggle OFF again (not persisted). ☐
13. Failure fallback (dev only): launch with
    `MAT2_WRAPPER_SYNTH_PACK=/nonexistent` → job start rejected by pack
    integrity pin (or per-file Warning "Clean output is available" if engine
    fails mid-job); clean outputs always survive. ☐

---

## Results log

| Test | Date | Build | Result | Notes |
|---|---|---|---|---|
| A | — | — | pending | — |
| B | — | — | pending | — |
| C | — | — | pending | — |
| D | — | — | pending | — |
| E | — | — | pending | — |
| F | — | — | pending | — |
| G | — | — | pending | — |
| H | — | — | pending | — |
