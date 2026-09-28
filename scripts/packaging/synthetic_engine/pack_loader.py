#!/usr/bin/env python3
"""S1: bundled synthetic-profile pack loader + integrity validation.

Validates per SYNTHETIC_METADATA_HANDOFF.md §25:
schema version supported, required sections present, no duplicate profile
ids, archetype references resolve, location coordinates valid, release
years sensible, format aliases resolve, and no semantic-field defaults.

Semantic fields (title/subject/description/keywords/copyright/license/
artist/album/track title/composer/publisher) are NEVER synthesizable: any
occurrence in sparsity tables or candidate fields is hard-filtered with a
recorded warning (the pack ships one such stray entry: office_pdf_export
sparsity 'title' — the error-severity semantic_fields_off rule wins).
"""
import hashlib
import json
import re

SUPPORTED_SCHEMA_MAJOR = 1

REQUIRED_SECTIONS = (
    'defaults', 'generation_rules', 'name_pools', 'location_profiles',
    'camera_device_profiles', 'document_software_profiles',
    'image_software_profiles', 'video_software_profiles',
    'audio_software_profiles', 'archetypes', 'format_recipes',
    'consistency_checks',
)

SEMANTIC_TOKENS = (
    'title', 'subject', 'description', 'keywords', 'copyright', 'license',
    'artist', 'album', 'track', 'composer', 'publisher',
)

SEMANTIC_EXACT_FIELDS = {
    'Title', 'Subject', 'Description', 'Keywords', 'Copyright', 'License',
    'Artist', 'Album', 'TrackTitle', 'Composer', 'Publisher',
    'PDF:Title', 'PDF:Subject', 'PDF:Keywords',
    'XMP-dc:Title', 'XMP-dc:Description', 'XMP-dc:Subject',
    'dc:title', 'dc:subject', 'dc:description', 'cp:keywords',
    'meta:keyword', 'TIT2/Title', 'TPE1/Artist', 'TALB/Album',
    'TCOM/Composer', 'TCOP/Copyright',
}


class PackError(Exception):
    pass


def _is_semantic(field_name):
    if field_name in SEMANTIC_EXACT_FIELDS:
        return True
    low = field_name.lower()
    base = low.rsplit(':', 1)[-1].rsplit('/', 1)[-1]
    return any(base == t or base.startswith(t) for t in
               ('title', 'subject', 'description', 'keywords', 'copyright',
                'license', 'artist', 'album', 'tracktitle', 'composer',
                'publisher'))


def sha256_file(path):
    h = hashlib.sha256()
    with open(path, 'rb') as f:
        for chunk in iter(lambda: f.read(1 << 20), b''):
            h.update(chunk)
    return h.hexdigest()


def load_and_validate(path):
    """Load the pack, run all §25 validations, return (pack, warnings)."""
    with open(path, 'r', encoding='utf-8') as f:
        pack = json.load(f)
    warnings = []

    version = str(pack.get('schema_version', ''))
    m = re.match(r'^(\d+)\.', version)
    if not m or int(m.group(1)) != SUPPORTED_SCHEMA_MAJOR:
        raise PackError('unsupported schema_version: %r' % version)

    for section in REQUIRED_SECTIONS:
        if section not in pack:
            raise PackError('required section missing: %s' % section)

    # duplicate ids
    for section in ('camera_device_profiles', 'location_profiles',
                    'document_software_profiles', 'image_software_profiles',
                    'video_software_profiles', 'audio_software_profiles'):
        ids = [p['id'] for p in pack[section]]
        dupes = {i for i in ids if ids.count(i) > 1}
        if dupes:
            raise PackError('duplicate ids in %s: %s' % (section, sorted(dupes)))
    arch_ids = [a['id'] for a in pack['archetypes']]
    if len(set(arch_ids)) != len(arch_ids):
        raise PackError('duplicate archetype ids')

    # location validity
    for loc in pack['location_profiles']:
        lat, lon = loc.get('center_lat'), loc.get('center_lon')
        if not isinstance(lat, (int, float)) or not (-90 <= lat <= 90):
            raise PackError('invalid latitude in %s' % loc.get('id'))
        if not isinstance(lon, (int, float)) or not (-180 <= lon <= 180):
            raise PackError('invalid longitude in %s' % loc.get('id'))
        if not loc.get('timezone'):
            raise PackError('missing timezone in %s' % loc.get('id'))
        if not (0 < loc.get('gps_jitter_radius_km', 0) <= 50):
            raise PackError('implausible jitter radius in %s' % loc.get('id'))

    # release years sensible
    for cam in pack['camera_device_profiles']:
        y = cam.get('earliest_plausible_year')
        if not isinstance(y, int) or not (1990 <= y <= 2035):
            raise PackError('implausible earliest_plausible_year in %s' % cam.get('id'))
    for section in ('document_software_profiles', 'image_software_profiles',
                    'video_software_profiles', 'audio_software_profiles'):
        for prof in pack[section]:
            y = prof.get('earliest_year')
            if not isinstance(y, int) or not (1990 <= y <= 2035):
                raise PackError('implausible earliest_year in %s/%s' % (section, prof.get('id')))

    # archetype references resolve (requires: camera_device:<class> | timestamp | families)
    cam_classes = {c.get('class') for c in pack['camera_device_profiles']}
    software_sections = {'document_software', 'image_software', 'video_software', 'audio_software'}
    for arch in pack['archetypes']:
        for req in arch.get('requires', []):
            if ':' in req:
                family, value = req.split(':', 1)
                if family == 'camera_device' and value not in cam_classes:
                    raise PackError('archetype %s requires unknown device class %s' % (arch['id'], value))
            elif req not in ('timestamp', 'persona', 'location') and req not in software_sections:
                raise PackError('archetype %s requires unknown family %s' % (arch['id'], req))

    # format recipes: aliases resolve, writers known, archetypes exist
    known_writers = {
        'exiftool', 'exiftool_after_mat2_clean_only', 'mutagen',
        'mutagen_preferred_exiftool_read_only_for_id3',
        'mutagen_or_exiftool_after_format_tests',
        'safe_ooxml_core_properties_adapter', 'deferred',
    }
    recipes = pack['format_recipes']
    for ext, recipe in recipes.items():
        if 'alias_of' in recipe:
            target = recipe['alias_of']
            if target not in recipes or 'alias_of' in recipes[target]:
                raise PackError('recipe alias %s -> %s does not resolve' % (ext, target))
            continue
        if recipe.get('writer') not in known_writers:
            raise PackError('recipe %s has unknown writer %r' % (ext, recipe.get('writer')))
        for arch in recipe.get('archetypes', []):
            if arch not in arch_ids:
                raise PackError('recipe %s references unknown archetype %s' % (ext, arch))

    # semantic-field scrub (hard filter + warning; never silent)
    for arch in pack['archetypes']:
        sparsity = arch.get('sparsity', {})
        for key in [k for k in sparsity if _is_semantic(k) and k != 'semantic_music_fields']:
            warnings.append(
                'pack sanity: removed semantic sparsity key %r from archetype %s '
                '(semantic_fields_off rule overrides pack data)' % (key, arch['id']))
            del sparsity[key]
    for ext, recipe in recipes.items():
        if 'alias_of' in recipe:
            continue
        kept = []
        for field in recipe.get('candidate_fields', []):
            if _is_semantic(field):
                warnings.append(
                    'pack sanity: removed semantic candidate field %r from recipe %s' % (field, ext))
            else:
                kept.append(field)
        recipe['candidate_fields'] = kept

    # defaults must be safe as shipped
    d = pack['defaults']
    if d.get('synthetic_mode') != 'off':
        raise PackError('pack defaults must ship synthetic_mode=off')
    if d.get('serial_mode') != 'off':
        raise PackError('pack defaults must ship serial_mode=off')
    if d.get('location_mode') != 'off':
        raise PackError('pack defaults must ship location_mode=off')
    if d.get('rng') != 'os_csprng' or d.get('seed_persistence') != 'none':
        raise PackError('pack defaults must require os_csprng and no seed persistence')

    return pack, warnings


def resolve_recipe(pack, extension):
    """Resolve an extension to its effective recipe (aliases followed).
    Returns (ext, recipe) or (ext, None) when unsupported for synthetic."""
    ext = extension.lower().lstrip('.')
    recipe = pack['format_recipes'].get(ext)
    if recipe is None:
        return ext, None
    if 'alias_of' in recipe:
        target = recipe['alias_of']
        return target, pack['format_recipes'].get(target)
    return ext, recipe


def synthetic_support(pack, extension):
    """Classify synthetic support: 'tier1'|'tier2'|'deferred'|'unsupported'."""
    _, recipe = resolve_recipe(pack, extension)
    if recipe is None:
        return 'unsupported'
    if recipe.get('writer') == 'deferred' or recipe.get('support_tier') == 3:
        return 'deferred'
    return 'tier%d' % recipe.get('support_tier', 3)
