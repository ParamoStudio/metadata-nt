# Synthetic Metadata Add-on — Integration Handoff

## Status

This is an **add-on** to the MAT2 Tauri wrapper already being implemented.

Do not restart or redesign the main application.

Assume the base project has already implemented or is implementing:

- Tauri 2 UI
- file selection and batch registry
- MAT2 runner
- staging/output pipeline
- pre/post MAT2 inspection
- result/log UI
- hard-coded external links
- security regression tests

This add-on attaches **after the MAT2 cleaning stage** and remains optional.

The accompanying data file is:

```text
synthetic_metadata_profiles_v1.json
```

Treat it as a bundled, read-only local resource.

---

# 1. Goal

Add an Advanced option:

```text
Synthetic metadata
○ Off
● Add plausible decoy metadata
```

The feature exists for privacy workflows where a user prefers a cleaned file to contain a plausible, internally coherent set of non-original metadata rather than an obviously empty metadata block.

The module must not weaken the original MAT2 workflow.

MAT2 remains the sanitisation authority.

---

# 2. Mandatory pipeline order

The order is not negotiable:

```text
ORIGINAL
  ↓
MAT2 PRE-INSPECT
  ↓
STAGING COPY
  ↓
MAT2 CLEAN
  ↓
MAT2 CLEAN VERIFICATION
  ↓
OPTIONAL SYNTHETIC PROFILE GENERATION
  ↓
CONSISTENCY VALIDATION
  ↓
SYNTHETIC METADATA WRITER
  ↓
POST-WRITE INSPECTION
  ↓
ORIGINAL-VALUE ABSENCE CHECK
  ↓
EXPECTED-SYNTHETIC-VALUE CHECK
  ↓
ATOMIC FINAL COMMIT
```

Never write decoy metadata to the original file.

Never write decoy metadata before MAT2 has removed the original metadata.

---

# 3. Why profile-based generation

Do not independently randomize every tag.

Bad:

```text
Make=random
Model=random
Software=random
Timezone=random
Lens=random
Date=random
```

This creates impossible combinations.

Instead construct a profile:

```text
Archetype
 + optional DeviceProfile
 + SoftwareProfile
 + PersonaAlias
 + optional LocationProfile
 + TimestampSet
 = SyntheticProfile
```

Then map that profile into the tags valid for the destination format.

The bundled JSON contains:

- format recipes
- archetypes
- camera/device profiles
- document software profiles
- image/video/audio software profiles
- locale-specific synthetic name-component pools
- city/timezone location profiles
- sparsity probabilities
- consistency rules

---

# 4. The database is combinatorial, not exhaustive

Do not interpret the JSON as “one output profile per row”.

Example:

```text
24 camera profiles
× many timestamp choices
× 50 locations
× thousands of locale-consistent alias combinations
× software profiles
× sparse-field sampling
```

produces a very large output space without shipping thousands of repeated rows.

This is preferable to 500 independent values per tag.

---

# 5. Privacy defaults

Synthetic mode is OFF by default.

When enabled, defaults are:

```text
Profile scope:       Independent per file
Location:            Off
Synthetic author:    On only where the chosen format/archetype commonly carries authorship
Synthetic serial:    Off
Semantic content:    Never synthesize
```

Do not persist Synthetic mode across application restarts unless the owner later explicitly requests it.

---

# 6. Advanced UI

Inside the existing Advanced area:

```text
Synthetic metadata
[ ] Add plausible decoy metadata

Profile behavior
● Independent profile per file
○ Consistent profile for this batch

Identity fields
● Synthetic alias where plausible
○ Leave empty

Location
● No synthetic location
○ City/country only
○ Include synthetic GPS

Technical/device metadata
● Synthetic where plausible
○ Leave empty

Device serial identifiers
● Leave empty
○ Generate synthetic identifiers   [expert]
```

Serial identifiers remain OFF by default.

Show a warning next to exact GPS:

> Synthetic GPS is deliberately false. It may affect how recipients interpret the file.

The application must display which values are synthetic.

---

# 7. Preview

Before starting a job, optionally allow:

```text
Preview synthetic profile
```

Example:

```text
Archetype      smartphone_photo
Device         Apple / iPhone 14 Pro
Created        2024-05-18 16:22:07 +02:00
Author         —
Location       disabled
Software       omitted by sparsity rule
```

For a consistent batch persona:

```text
Batch profile
Device         Canon EOS R6 Mark II
Alias          Laura Vega
Timezone       Europe/Madrid
GPS            disabled
```

Individual timestamps can still differ.

Do not show/store a stable global profile ID that would become a user fingerprint.

---

# 8. Randomness

Use the OS cryptographic RNG.

Rust:

```text
getrandom / rand backed by OS CSPRNG
```

or another minimal current equivalent already accepted by the project.

Do not derive the profile from:

- input file hash
- file name
- machine ID
- installation ID
- user account name

Reason: deterministic derivation from input creates cross-user correlation.

## Internal deterministic job behavior

For reproducibility within a single job:

1. generate a fresh random job seed;
2. keep it only in memory;
3. derive per-file choices from `(job_seed, selection_id)`;
4. discard the seed when the job/session ends.

Do not embed the seed in the exported file.

Do not log it persistently.

---

# 9. Consistent batch mode

`Consistent profile for this batch` means a shared synthetic context, not identical metadata.

May share:

- persona alias
- device family
- software family
- timezone
- city if location enabled

Should vary naturally:

- timestamps
- UUIDs/document IDs
- instance IDs
- some sparse optional tags
- per-file encoder/export values where format requires them

Never reuse the same XMP InstanceID across multiple output files.

---

# 10. Sparse output is a feature

Do not fill every writable tag.

Use `archetype.sparsity`.

A normal smartphone JPEG may have:

```text
Make
Model
DateTimeOriginal
optional Software
optional GPS
```

It does not need:

```text
Author
Copyright
Keywords
OwnerName
20 XMP identity fields
```

A sparse plausible profile is less distinctive than a fully populated synthetic block.

---

# 11. Fields that are NOT part of v1 synthetic privacy mode

Do not automatically invent semantic claims:

- Title
- Subject
- Description
- Keywords
- Copyright
- License
- Artist
- Album
- Track title
- Composer
- Publisher
- factual scene description

These alter the apparent content or rights of a file, not merely provenance.

Keep them absent/untouched unless a later explicit editorial feature is designed.

---

# 12. Real-person avoidance

The included persona data contains **name components**, not a database of real identities.

Rules:

- construct a given name + surname within one locale pool;
- treat the result as a synthetic alias;
- never add phone number;
- never add email address;
- never add street address;
- never copy IDs from a real person;
- never claim the alias corresponds to an actual person.

Accidental collision with a real person's name is possible with common names and is not evidence of that person's involvement.

Document this.

---

# 13. Location policy

The database ships major-city reference points and timezones.

Default location = OFF.

## City-only

If supported by the format, synthetic location may contain:

```text
City
CountryCode
Timezone-consistent date offset
```

without exact GPS.

## GPS opt-in

If GPS is enabled:

1. choose a location profile;
2. jitter within its declared radius;
3. round to reasonable precision;
4. use the location timezone for timestamp offsets;
5. never generate a street address.

The bundled centers are approximate city references, not a promise that every jittered coordinate lands on a specific public venue.

If the project wants stronger geographic realism later, replace the location set with a reviewed offline public-places corpus.

---

# 14. Dates

Generate dates subject to:

```text
creation/capture <= modification
capture year >= selected device earliest_plausible_year
```

Do not select a camera/device that did not exist at the synthetic capture time.

If location is enabled, use its timezone for offset-bearing fields.

If location is disabled, choose one coherent timezone/offset for the profile rather than independently randomizing each timestamp.

---

# 15. Identifiers

Generate:

- XMP DocumentID
- XMP InstanceID
- UUID-like values

as fresh random UUIDv4-style identifiers where appropriate.

Never copy an identifier from:

- the original file
- public sample files
- real hardware databases

## Serial numbers

Serial metadata is OFF by default.

If enabled:

- generate format-only synthetic values;
- never use published real serials;
- do not pretend to reproduce vendor checksum schemes unless they are explicitly documented and reviewed.

It is better to omit a serial than invent an obviously impossible one.

---

# 16. Writer architecture

Add a module boundary such as:

```text
SyntheticMetadataEngine
├── ProfileGenerator
├── ConsistencyValidator
├── WriterRouter
│   ├── ExifToolWriter
│   ├── MutagenWriter
│   └── OoxmlCorePropertiesWriter
└── SyntheticVerifier
```

Do not put synthetic logic into `Mat2Runner`.

MAT2 and synthetic writing must remain independently auditable.

---

# 17. Writer routing

Read the `format_recipes` section in the JSON.

## Tier 1 — ExifTool

Primary candidates:

- JPEG
- PNG
- TIFF
- WebP
- HEIC/compatible image containers after tests
- PDF (only after MAT2 cleaning)
- MOV
- MP4

ExifTool is used as a writer, not as a fallback sanitizer.

Always invoke it without a shell and with argument arrays.

Pin/bundle the ExifTool version used by the release or otherwise record it in diagnostics.

## Tier 2 — existing MAT2 runtime dependencies / small adapters

### MP3 / audio

ExifTool's current ID3 tag tables expose standard ID3 frames as non-writable.

Use Mutagen, already part of MAT2's audio dependency stack, if the packaged runtime already contains it.

Synthetic audio mode should focus on technical provenance fields:

- encoder
- encoding/tagging time

Do not synthesize Artist/Album/Title.

### DOCX

If implemented:

- operate only on the MAT2-cleaned output;
- rewrite only `docProps/core.xml`;
- create a new archive;
- do not extract arbitrary ZIP paths to disk;
- preserve document payload;
- verify the resulting DOCX opens and core properties match.

This adapter requires dedicated ZipSlip/path safety tests even if it does not extract files.

## Tier 3

ODT and other formats remain disabled until a separately reviewed writer exists.

A format may support MAT2 cleaning without supporting synthetic mode.

The UI must distinguish these states.

---

# 18. PDF rule

ExifTool writes PDF metadata using incremental update techniques.

Therefore:

```text
ORIGINAL PDF
   ↓ MAT2 clean/rebuild
CLEAN PDF
   ↓ ExifTool synthetic write
DECOY PDF
```

is acceptable for this module.

Never:

```text
ORIGINAL PDF
   ↓ ExifTool overwrite/spoof
```

as the privacy workflow.

The clean intermediate must have passed MAT2 verification first.

---

# 19. Format recipe validation at startup/build time

Do not blindly trust the bundled candidate tag list forever.

For the pinned ExifTool version:

- query/list writable tags during development/build validation;
- test each enabled recipe against fixtures;
- fail tests when a supposedly writable field no longer behaves as expected.

The database is policy/configuration; the writer's integration tests prove reality.

---

# 20. Verification model

The old final criterion:

```text
No metadata detectable by MAT2
```

is not sufficient once synthetic metadata is deliberately present.

Split verification.

## Phase A — Clean verification

Before synthetic writing:

- MAT2 output exists;
- original sensitive values are absent according to the existing cleaner verification;
- clean result is accepted.

## Phase B — Synthetic verification

After writing:

Re-read with:

- MAT2 inspection where meaningful;
- ExifTool/Mutagen/OOXML adapter as the corresponding writer/reader.

Confirm:

```text
expected synthetic field == generated value
original sensitive value != output value
no original sensitive values reappeared
format remains valid/openable
```

The UI should say:

```text
Original identifying metadata removed
Synthetic metadata added and verified
```

not:

```text
Anonymous
Forensically indistinguishable
Impossible to trace
```

---

# 21. Before / cleaned / synthetic comparison

Extend the existing comparison view from two columns to three when synthetic mode is on:

```text
ORIGINAL            CLEANED             SYNTHETIC OUTPUT
Author: A        →  —                →  Robin Hayes
GPS: ...         →  —                →  —
Camera: Apple    →  —                →  SONY / ILCE-7M4
Created: ...     →  —                →  2024-03-08 ...
```

Colors:

- original sensitive: red/muted red
- removed/empty clean: neutral gray
- synthetic: violet/accent
- remaining original value: warning amber/red

Every synthetic value should carry a UI-only `Synthetic` status.

Do not embed the word “synthetic” into the output tag value itself.

---

# 22. Job result states

Synthetic mode adds:

```text
Cleaned
Synthetic writing
Synthetic verifying
Success (synthetic)
Warning
Failed
```

If synthetic writing fails **after a valid MAT2-cleaned file exists**:

Preferred behavior:

1. keep the valid clean intermediate available as a clean result;
2. do not pretend the spoof succeeded;
3. do not silently publish the partially written synthetic file;
4. show:

```text
MAT2 cleaning succeeded.
Synthetic metadata could not be applied.
Clean output is available.
```

This is safer than destroying a valid clean result.

---

# 23. Security constraints

Do not add:

- networking;
- online geocoding;
- external name API;
- remote device-profile lookup;
- telemetry;
- persistent user profile;
- install ID;
- global deterministic seed.

All datasets are bundled and local.

Do not parse/render thumbnails to choose profiles.

Do not let the frontend submit arbitrary ExifTool arguments.

Frontend submits typed choices only:

```text
SyntheticOptions {
  enabled
  profile_scope
  identity_mode
  location_mode
  technical_mode
  serial_mode
}
```

Rust/backend constructs all writer arguments.

---

# 24. Tests

## Profile generator

Test:

- same-locale names;
- timestamps ordered;
- device release guard;
- location timezone consistency;
- serial off by default;
- semantic fields absent;
- independent profiles differ;
- consistent batch shares intended context but not UUIDs.

## Sparsity

With deterministic test seed:

- expected optional-field sampling is stable;
- fields not allowed by recipe never appear.

## Writer router

Each extension routes only to its approved adapter.

Unsupported synthetic format returns:

```text
Synthetic mode unavailable for this format
```

while MAT2 clean remains usable.

## Security

- filenames beginning `-`
- Unicode
- quotes
- shell metacharacters
- control characters
- no shell
- no arbitrary writer tags from frontend
- no original path passed to synthetic writer
- no persistent seed/log

## PDF

Prove the writer receives the clean staged PDF, not the original.

## DOCX if enabled

- archive reconstruction
- no path extraction
- core properties only
- document remains openable

---

# 25. Data file integrity

Bundle:

```text
synthetic_metadata_profiles_v1.json
```

Record its SHA-256 in release/build metadata.

Validate at startup/build:

- schema version supported;
- required sections present;
- no duplicate profile IDs;
- all archetype references resolve;
- location latitude/longitude valid;
- release years sensible;
- format aliases resolve;
- no semantic-field defaults have accidentally been added.

Do not silently accept malformed profile data.

---

# 26. Data updates

The synthetic pack is versioned independently:

```text
schema_version
pack_name
```

Updating the pack should not require modifying MAT2.

Future pack changes require:

- schema validation;
- consistency tests;
- format fixture tests;
- security review if new writer/tag families are introduced.

No auto-download in v1.

---

# 27. Recommended implementation order from current project state

The main project is already around Task 11.

Do not rewind.

Implement this add-on after the existing basic UI/job pipeline is stable:

### S1 — Load/validate bundled JSON

No writers yet.

### S2 — ProfileGenerator + tests

Generate an in-memory profile and preview.

### S3 — ConsistencyValidator + tests

Reject impossible date/device/location combinations.

### S4 — Advanced UI controls

Synthetic mode remains non-functional behind a feature flag until writer tests pass.

### S5 — ExifToolWriter

Start with JPEG/PNG fixtures.

### S6 — PDF writer path

Only clean intermediate allowed.

### S7 — MOV/MP4 writer path

Fixture-tested writable fields only.

### S8 — SyntheticVerifier

Original-absence + expected-decoy checks.

### S9 — Three-column comparison UI

Original / cleaned / synthetic.

### S10 — Mutagen audio writer

Only if packaged runtime already includes Mutagen and tests are straightforward.

### S11 — DOCX adapter

Optional for this release; do not block image/PDF/video synthetic mode.

### S12 — Security/regression review

Re-run the main project's release gates plus add-on tests.

---

# 28. Definition of Done

Synthetic mode is complete when:

1. it is OFF by default;
2. it only operates on a verified MAT2-cleaned staged file;
3. profiles are internally coherent;
4. writer arguments are typed/backend-generated;
5. original identifying values do not survive into the synthetic result;
6. expected synthetic values are re-read and verified;
7. output remains valid;
8. the UI clearly distinguishes synthetic values;
9. failure falls back to an available clean MAT2 output rather than false success;
10. no network or persistent identity is introduced.

---

# 29. References to review

ExifTool tag families and writability:

- https://exiftool.org/TagNames/
- https://exiftool.org/TagNames/PDF.html
- https://exiftool.org/TagNames/PNG.html
- https://exiftool.org/TagNames/QuickTime.html
- https://exiftool.org/TagNames/XMP.html
- https://exiftool.org/TagNames/ID3.html

Exif standard:

- https://www.cipa.jp/e/std/std-sec.html

ID3:

- https://id3.org/id3v2.4.0-frames

Prior art:

- https://github.com/davvikq/deceptive-metadata-shredder

The prior-art project is useful for comparison, but this implementation must preserve the current wrapper's core trust model: MAT2 cleans; the add-on only writes optional decoy metadata afterward.
