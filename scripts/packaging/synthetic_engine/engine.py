#!/usr/bin/env python3
"""S2/S3: ProfileGenerator + ConsistencyValidator.

Randomness contract (HANDOFF §8): the job seed comes from the Rust side
(OS CSPRNG, memory-only). Per-file choices derive deterministically from
sha256(job_seed | selection_id) so a job is internally reproducible while
remaining unpredictable across jobs/users. Seeds are never embedded in
output and never logged.

Profiles are coherent combinations (archetype + device + software +
persona + location + timestamps + sparsity), not independently randomized
tags (HANDOFF §3). Semantic fields are never generated (HANDOFF §11).
"""
import hashlib
import random
import re
import uuid
from datetime import datetime, timedelta, timezone

from .pack_loader import resolve_recipe, _is_semantic

LOCALE_TIMEZONES = {
    'es-ES': 'Europe/Madrid',
    'en-GB': 'Europe/London',
    'en-US': 'America/New_York',
    'fr-FR': 'Europe/Paris',
    'de-DE': 'Europe/Berlin',
    'it-IT': 'Europe/Rome',
    'pt-BR': 'America/Sao_Paulo',
    'nl-NL': 'Europe/Amsterdam',
    'pl-PL': 'Europe/Warsaw',
    'tr-TR': 'Europe/Istanbul',
}

EMAIL_RE = re.compile(r'@[^@\s]+\.[a-z]{2,}', re.I)
PHONE_RE = re.compile(r'\+?\d[\d\s\-()]{7,}\d')


def derive_rng(job_seed, selection_id):
    digest = hashlib.sha256(('%s|%s' % (job_seed, selection_id)).encode('utf-8')).digest()
    return random.Random(digest)


def _tz_offset_minutes(tz_name, dt_naive):
    """UTC offset minutes for tz at a naive datetime; None when tzdata unavailable."""
    try:
        from zoneinfo import ZoneInfo
        z = ZoneInfo(tz_name)
        return int(dt_naive.replace(tzinfo=z).utcoffset().total_seconds() // 60)
    except Exception:
        return None


FALLBACK_OFFSETS = {
    'Europe/Madrid': 60, 'Europe/London': 0, 'America/New_York': -300,
    'Europe/Paris': 60, 'Europe/Berlin': 60, 'Europe/Rome': 60,
    'America/Sao_Paulo': -180, 'Europe/Amsterdam': 60, 'Europe/Warsaw': 60,
    'Europe/Istanbul': 180,
}


def _offset_string(minutes):
    sign = '+' if minutes >= 0 else '-'
    m = abs(minutes)
    return '%s%02d:%02d' % (sign, m // 60, m % 60)


class ProfileError(Exception):
    pass


def _pick_archetype(rng, pack, recipe):
    allowed = recipe.get('archetypes', [])
    archs = [a for a in pack['archetypes'] if a['id'] in allowed]
    if not archs:
        raise ProfileError('no archetype available for recipe')
    return rng.choice(archs)


def _device_for(rng, pack, archetype, technical_on, year_floor, year_ceiling):
    requires_device_class = None
    for req in archetype.get('requires', []):
        if req.startswith('camera_device:'):
            requires_device_class = req.split(':', 1)[1]
    wants_device = requires_device_class or ('camera_identity' in archetype.get('sparsity', {})
                                             and rng.random() < archetype['sparsity'].get('camera_identity', 0))
    if not technical_on or not wants_device:
        return None
    pool = pack['camera_device_profiles']
    if requires_device_class:
        pool = [c for c in pool if c.get('class') == requires_device_class]
    pool = [c for c in pool if c.get('earliest_plausible_year', 9999) <= year_ceiling]
    if not pool:
        return None
    return rng.choice(pool)


def _software_for(rng, pack, archetype, technical_on):
    family_key = None
    for req in archetype.get('requires', []):
        if req.endswith('_software'):
            family_key = req + '_profiles'
    if family_key is None:
        for opt in archetype.get('optional', []):
            if opt.endswith('_software'):
                family_key = opt + '_profiles'
                break
    if family_key is None or not technical_on:
        return None
    pool = pack.get(family_key, [])
    if not pool:
        return None
    sparsity_key = 'software' if 'software' in archetype.get('sparsity', {}) else 'encoder'
    p = archetype.get('sparsity', {}).get(sparsity_key, 0.5)
    if family_key in ('document_software_profiles',) or archetype.get('requires', []):
        required = any(r.endswith('_software') for r in archetype.get('requires', []))
        if required or rng.random() < p:
            return rng.choice(pool)
        return None
    return rng.choice(pool) if rng.random() < p else None


def _persona_for(rng, pack, archetype, identity_alias):
    if not identity_alias:
        return None
    p = archetype.get('sparsity', {}).get('author', 0)
    if 'persona' not in archetype.get('optional', []) or rng.random() >= p:
        return None
    locales = sorted(pack['name_pools'].keys())
    locale = rng.choice(locales)
    pool = pack['name_pools'][locale]
    return {
        'locale': locale,
        'given': rng.choice(pool['given']),
        'surname': rng.choice(pool['surname']),
        'full': None,  # filled below to keep same-locale construction explicit
    }


def _location_for(rng, pack, archetype, location_mode):
    if location_mode == 'off':
        return None, False
    wants_gps = location_mode == 'gps'
    p = archetype.get('sparsity', {}).get('gps', 0)
    gps = wants_gps and rng.random() < p
    loc = rng.choice(pack['location_profiles'])
    if not gps and location_mode == 'city':
        return loc, False
    return loc, gps


def _jitter_gps(rng, loc):
    radius_km = float(loc.get('gps_jitter_radius_km', 5))
    dlat = rng.uniform(-radius_km, radius_km) / 111.0
    import math
    dlon = rng.uniform(-radius_km, radius_km) / (111.0 * max(0.2, math.cos(math.radians(loc['center_lat']))))
    lat = round(loc['center_lat'] + dlat, 4)
    lon = round(loc['center_lon'] + dlon, 4)
    return lat, lon


def generate_profile(pack, options, job_seed, selection_id, extension, now=None):
    """Build one coherent SyntheticProfile dict for a file.

    options keys: profile_scope ('per_file'|'batch'), identity_mode
    ('alias'|'empty'), location_mode ('off'|'city'|'gps'), technical_mode
    ('synthetic'|'empty'), serial_mode ('empty'|'generate').
    """
    ext, recipe = resolve_recipe(pack, extension)
    if recipe is None or recipe.get('writer') == 'deferred':
        raise ProfileError('synthetic mode unavailable for this format')

    scope = options.get('profile_scope', 'per_file')
    ctx_id = 'batch-context' if scope == 'batch' else selection_id
    ctx_rng = derive_rng(job_seed, ctx_id)
    rng = derive_rng(job_seed, selection_id)

    archetype = _pick_archetype(ctx_rng, pack, recipe)
    rules = pack['generation_rules']
    min_year = int(rules['timestamp']['min_year'])
    max_year = min(int(rules['timestamp']['max_year']), (now or datetime.now()).year)

    identity_alias = options.get('identity_mode', 'alias') == 'alias'
    technical_on = options.get('technical_mode', 'synthetic') == 'synthetic'
    location_mode = options.get('location_mode', 'off')
    serial_on = options.get('serial_mode', 'empty') == 'generate'

    year_floor = min_year
    if technical_on:
        pool = pack['camera_device_profiles']
        year_floor = min_year  # device picked next may raise the floor

    device = _device_for(ctx_rng, pack, archetype, technical_on, year_floor, max_year)
    if device is not None:
        year_floor = max(year_floor, int(device.get('earliest_plausible_year', min_year)))
    if year_floor > max_year:
        year_floor = max_year

    software = _software_for(ctx_rng, pack, archetype, technical_on)
    persona = _persona_for(ctx_rng, pack, archetype, identity_alias)
    if persona:
        persona['full'] = '%s %s' % (persona['given'], persona['surname'])

    location, gps_on = _location_for(ctx_rng, pack, archetype, location_mode)

    # one coherent timezone for the whole profile (HANDOFF §14)
    if location is not None:
        tz_name = location['timezone']
    elif persona is not None and persona['locale'] in LOCALE_TIMEZONES:
        tz_name = LOCALE_TIMEZONES[persona['locale']]
    else:
        tz_name = ctx_rng.choice(sorted({l['timezone'] for l in pack['location_profiles']}))

    start = datetime(year_floor, 1, 1)
    end = datetime(max_year, 12, 31, 23, 59, 59)
    if now is not None:
        end = min(end, now.replace(tzinfo=None))
    if end <= start:
        end = start + timedelta(days=1)
    span = int((end - start).total_seconds())
    created = start + timedelta(seconds=rng.randrange(span))
    modified = created + timedelta(minutes=rng.randrange(0, 60 * 24 * 45))
    offset_min = _tz_offset_minutes(tz_name, created)
    if offset_min is None:
        offset_min = FALLBACK_OFFSETS.get(tz_name, 0)

    gps = None
    if gps_on and location is not None:
        lat, lon = _jitter_gps(rng, location)
        gps = {'lat': lat, 'lon': lon}

    identifiers = {
        'document_id': 'xmp.did:%s' % uuid.UUID(bytes=rng.randbytes(16), version=4),
        'instance_id': 'xmp.iid:%s' % uuid.UUID(bytes=rng.randbytes(16), version=4),
    }

    serial = None
    if serial_on and device is not None and rng.random() < archetype.get('sparsity', {}).get('serial', 0.5):
        fmt = rng.choice(['%s-%05d-%s', '%02d%s%05d', 'C%d%06d'])
        if fmt == '%s-%05d-%s':
            serial = ('%s-%05d-%s' % (device['make'][:2].upper(), rng.randrange(100000),
                                      ''.join(ctx_rng.choice('ABCDEFGHJKLMNPQRSTUVWXYZ0123456789') for _ in range(4))))
        elif fmt == '%02d%s%05d':
            serial = ('%02d%s%05d' % (rng.randrange(1, 99), ''.join(rng.choice('0123456789') for _ in range(3)),
                                      rng.randrange(100000)))
        else:
            serial = ('C%d%06d' % (rng.randrange(1, 9), rng.randrange(1000000)))

    sparsity = archetype.get('sparsity', {})
    lens = None
    if device is not None and 'lens' in sparsity and rng.random() < sparsity['lens']:
        lens = _lens_for_device(rng, device)

    profile = {
        'archetype': archetype['id'],
        'format': ext,
        'writer': recipe.get('writer'),
        'device': ({'make': device['make'], 'model': device['model'],
                    'earliest_plausible_year': device['earliest_plausible_year']}
                   if device else None),
        'lens': lens,
        'software': software,
        'persona': persona,
        'location': ({'city': location['city'], 'country_code': location['country_code'],
                      'timezone': location['timezone']} if location is not None and location_mode != 'off' else None),
        'gps': gps,
        'timezone': tz_name,
        'utc_offset': _offset_string(offset_min),
        'utc_offset_minutes': offset_min,
        'created': created.strftime('%Y:%m:%d %H:%M:%S'),
        'created_iso': created.replace(tzinfo=timezone(_minutes_to_td(offset_min))).isoformat(),
        'modified': modified.strftime('%Y:%m:%d %H:%M:%S'),
        'modified_iso': modified.replace(tzinfo=timezone(_minutes_to_td(offset_min))).isoformat(),
        'identifiers': identifiers,
        'serial': serial,
        'sparsity_present': {
            'software': software is not None,
            'gps': gps is not None,
            'author': persona is not None,
            'serial': serial is not None,
            'lens': lens is not None,
        },
    }
    return profile


def _minutes_to_td(minutes):
    return timedelta(minutes=minutes)


def _lens_for_device(rng, device):
    cls = device.get('class')
    if cls == 'smartphone':
        return rng.choice(['built-in', device['make'] + ' built-in lens'])
    return rng.choice([
        'EF50mm f/1.8 STM', 'RF24-105mm F4 L IS USM', 'E 35mm F1.8 OSS',
        'NIKKOR Z 50mm f/1.8 S', 'XF35mmF2 R WR',
    ])


# ---------------------------------------------------------------------------
# S3 — ConsistencyValidator (pack 'consistency_checks', HANDOFF §24)
# ---------------------------------------------------------------------------

def validate_profile(pack, profile, original_values=(), job_seed=None, selection_id=None):
    """Return list of violations [{id, severity, detail}]; errors block writing."""
    violations = []

    def err(check_id, detail):
        violations.append({'id': check_id, 'severity': 'error', 'detail': detail})

    def warn(check_id, detail):
        violations.append({'id': check_id, 'severity': 'warning', 'detail': detail})

    # date_after_device_release
    if profile.get('device'):
        year = int(profile['created'][:4])
        if year < int(profile['device']['earliest_plausible_year']):
            err('date_after_device_release',
                'capture year %d < device year %d' % (year, profile['device']['earliest_plausible_year']))

    # date_order
    if profile['created'] > profile['modified']:
        err('date_order', 'created later than modified')

    # same_locale_persona
    persona = profile.get('persona')
    if persona:
        pool = pack['name_pools'].get(persona['locale'], {})
        if persona['given'] not in pool.get('given', []) or persona['surname'] not in pool.get('surname', []):
            err('same_locale_persona', 'persona components not from the same locale pool')
        if EMAIL_RE.search(persona['full']) or PHONE_RE.search(persona['full']):
            err('no_real_identifiers', 'persona contains contact-data pattern')

    # location_timezone_match
    if profile.get('location') and profile.get('gps') is not None:
        expected_tz = profile['location']['timezone']
        if profile.get('timezone') != expected_tz:
            warn('location_timezone_match',
                 'profile timezone %s != location timezone %s' % (profile.get('timezone'), expected_tz))

    # no_real_identifiers / semantic_fields_off / provenance — over the flattened field set
    fields = flatten_profile_fields(pack, profile)
    for key, value in fields.items():
        if _is_semantic(key):
            err('semantic_fields_off', 'semantic field generated: %s' % key)
        sval = str(value)
        for orig in original_values:
            o = str(orig).strip()
            if len(o) >= 4 and o.lower() in sval.lower():
                err('no_real_identifiers',
                    'generated value for %s reuses original value fragment' % key)
    if job_seed:
        blob = json_dumps(fields)
        seed_fragment = job_seed[:16]
        if (job_seed in blob or seed_fragment in blob
                or (selection_id and len(selection_id) >= 8 and selection_id in blob)):
            err('profile_provenance_internal_only', 'seed/selection id leaked into field values')

    # identifiers uniqueness is structural (uuid4 per file); batch InstanceID reuse is
    # prevented by per-file derivation — asserted in engine tests.
    return violations


def json_dumps(obj):
    import json
    return json.dumps(obj, sort_keys=True)


def flatten_profile_fields(pack, profile):
    """Format-independent view of what the profile will write (for validation
    and preview). Concrete per-format tag mapping lives in writers.py."""
    ext, recipe = resolve_recipe(pack, profile['format'])
    candidates = recipe.get('candidate_fields', []) if recipe else []
    out = {}

    def want(*names):
        return any(c.split(':')[-1] in names or c in names for c in candidates)

    if profile.get('device') and want('Make'):
        out['Make'] = profile['device']['make']
        out['Model'] = profile['device']['model']
    if profile.get('lens') and want('LensModel'):
        out['LensModel'] = profile['lens']
    if profile.get('software'):
        sw = profile['software'].get('software') or profile['software'].get('creator')
        if sw and (want('Software') or want('PNG:Software') or want('QuickTime:Encoder')
                   or want('PDF:Creator') or want('PDF:Producer')):
            out['Software'] = sw
        producer = profile['software'].get('producer')
        if producer and want('PDF:Producer'):
            out['Producer'] = producer
        encoded_by = profile['software'].get('encoded_by')
        if encoded_by:
            out['EncodedBy'] = encoded_by
    if profile.get('persona') and (want('XMP-dc:Creator') or want('PNG:Author')
                                   or want('PDF:Author') or want('QuickTime:Author')
                                   or want('dc:creator')):
        out['Author'] = profile['persona']['full']
    if want('DateTimeOriginal') or want('CreateDate') or want('PNG:CreationTime') \
            or want('QuickTime:CreationDate') or want('PDF:CreateDate') or want('dcterms:created'):
        out['CreateDate'] = profile['created']
    if want('ModifyDate') or want('PDF:ModifyDate') or want('dcterms:modified'):
        out['ModifyDate'] = profile['modified']
    if want('OffsetTimeOriginal'):
        out['OffsetTimeOriginal'] = profile['utc_offset']
    if profile.get('gps') and want('GPSLatitude'):
        out['GPSLatitude'] = profile['gps']['lat']
        out['GPSLongitude'] = profile['gps']['lon']
    if profile.get('location') and want('QuickTime:LocationName'):
        out['LocationName'] = '%s, %s' % (profile['location']['city'], profile['location']['country_code'])
    if want('XMP-xmpMM:DocumentID'):
        out['XMPDocumentID'] = profile['identifiers']['document_id']
    if want('XMP-xmpMM:InstanceID'):
        out['XMPInstanceID'] = profile['identifiers']['instance_id']
    if profile.get('serial'):
        out['SerialNumber'] = profile['serial']
    return out
