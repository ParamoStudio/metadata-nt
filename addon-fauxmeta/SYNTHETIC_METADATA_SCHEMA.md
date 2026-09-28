# Synthetic Metadata Pack — Data Model Notes

## File

`synthetic_metadata_profiles_v1.json`

## What it is

A local configuration/data pack for generating coherent synthetic metadata profiles.

It is intentionally **not** a giant list of 500 values for every metadata key.

The data model builds plausible combinations from:

```text
archetype
device
software
persona alias
location (optional)
timestamp set
sparsity pattern
```

This creates a much larger combinatorial space while keeping cross-field constraints enforceable.

## Important sections

### `defaults`

Security/privacy defaults for synthetic mode.

### `generation_rules`

Global rules for dates, identifiers, location, aliases and sparse field population.

### `name_pools`

Locale-consistent given-name and surname components.

Do not interpret these as real identity records.

### `location_profiles`

Major-city reference coordinates, timezone and jitter radius.

GPS use is opt-in.

### `camera_device_profiles`

Curated device identity combinations with a conservative earliest plausible year and common pixel-dimension families.

Do not add real serial numbers.

### software profile sections

- `document_software_profiles`
- `image_software_profiles`
- `video_software_profiles`
- `audio_software_profiles`

### `archetypes`

Defines which families of metadata normally appear together and how sparse they should be.

### `format_recipes`

Maps file extensions to:

- support tier
- writer adapter
- allowed archetypes
- candidate metadata families
- fields that should never be filled automatically

### `consistency_checks`

Rules that the generator/validator must enforce before writing.

## Support tiers

### Tier 1

Expected to be implemented/tested first with ExifTool:

- JPEG
- PNG
- TIFF
- WebP
- HEIC after fixture validation
- PDF after MAT2 cleaning
- MOV
- MP4

### Tier 2

Requires a small format-specific writer:

- MP3 / FLAC / OGG / M4A via Mutagen where appropriate
- DOCX via a narrowly scoped OOXML core-properties adapter

### Tier 3

Profile support exists, but writing is deferred:

- ODT and other formats until separately reviewed

## Why some fields are deliberately absent

The pack does not default to fabricating:

- title
- subject
- description
- keywords
- copyright/license
- artist
- album
- track title
- composer

Those are semantic/content claims rather than simple provenance noise.

## Versioning

The file carries:

```text
schema_version
pack_name
```

The application should reject unsupported schema versions rather than guessing.

## Suggested future data improvements

If the project later needs a larger corpus, add reviewed offline sources in separate versioned packs:

- more camera/device profiles
- editor/encoder version families
- more locale name-component pools
- reviewed public-place coordinate sets
- richer lens/device compatibility tables

Do not fetch these online at application runtime.
