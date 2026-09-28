#!/usr/bin/env python3
"""Engine CLI protocol — stdin JSON request, stdout JSON response.

Actions:
  validate_pack {pack_path}                       → {ok, sha256, warnings, counts}
  preview       {pack_path, options, job_seed,
                 selection_id, ext}               → {ok, profile}
  apply         {pack_path, options, job_seed,
                 selection_id, file:{path, ext,
                 original_values}}                → {ok, written, verification,
                                                     synthetic_state, error?, stage?}
  selftest      {pack_path}                       → {ok, formats}

Exit code 0 whenever a JSON response was produced (including ok:false);
non-zero only for protocol-level failures. stderr carries diagnostics for
the wrapper log; stdout is exclusively the JSON response.
"""
import json
import os
import re
import shutil
import sys

from .engine import ProfileError, generate_profile, validate_profile
from .pack_loader import PackError, load_and_validate, resolve_recipe, sha256_file, synthetic_support
from .verifier import VerifyError, verify_written
from .writers import WriteError, WriterRouter

_SOURCE_URL_OK = re.compile(r'^https?://[^\s\x00-\x1f\x7f]{1,2000}$')


def _plant_tripwire(writer, recipe, path, source_url):
    """Write the Canary token URL as XMP-dc:Source on the already
    synthetic-verified file. Snapshot/restore guarantees a failed plant
    falls back to the valid synthetic output (spec §20). The URL is only
    ever written and compared as a literal string — never fetched."""
    if not _SOURCE_URL_OK.match(source_url):
        return 'failed_kept_synthetic', 'invalid tripwire source URL', []
    if not writer.supports_source(recipe):
        return 'unavailable_format', None, []
    snapshot = path + '.pre-tripwire'
    try:
        shutil.copy2(path, snapshot)
        written = writer.write_source(path, source_url)
        actual = writer.read_source(path)
        if actual != source_url:
            os.replace(snapshot, path)
            return 'failed_kept_synthetic', 'Source read-back mismatch', []
        os.remove(snapshot)
        return 'planted_verified', None, written
    except Exception as exc:
        try:
            os.replace(snapshot, path)
        except OSError:
            pass
        return 'failed_kept_synthetic', '%s: %s' % (type(exc).__name__, exc), []


def _fail(stage, message):
    return {'ok': False, 'stage': stage, 'error': str(message)[:500]}


def _validate_pack(req):
    pack_path = req['pack_path']
    pack, warnings = load_and_validate(pack_path)
    return {
        'ok': True,
        'sha256': sha256_file(pack_path),
        'warnings': warnings,
        'counts': {
            'camera_device_profiles': len(pack['camera_device_profiles']),
            'location_profiles': len(pack['location_profiles']),
            'archetypes': len(pack['archetypes']),
            'format_recipes': len(pack['format_recipes']),
            'name_locales': len(pack['name_pools']),
        },
    }


def _preview(pack, req):
    profile = generate_profile(pack, req['options'], req['job_seed'],
                               req['selection_id'], req['ext'])
    view = {
        'archetype': profile['archetype'],
        'format': profile['format'],
        'device': ('%s / %s' % (profile['device']['make'], profile['device']['model'])
                   if profile.get('device') else None),
        'software': ((profile.get('software') or {}).get('software')
                     or (profile.get('software') or {}).get('creator')
                     or (profile.get('software') or {}).get('encoded_by')),
        'author': profile['persona']['full'] if profile.get('persona') else None,
        'created': profile['created'],
        'timezone': profile['timezone'],
        'utc_offset': profile['utc_offset'],
        'location': ('%s, %s' % (profile['location']['city'], profile['location']['country_code'])
                     if profile.get('location') else None),
        'gps': ('%.4f %.4f' % (profile['gps']['lat'], profile['gps']['lon'])
                if profile.get('gps') else None),
        'serial': profile.get('serial'),
        'sparsity_present': profile['sparsity_present'],
    }
    return {'ok': True, 'profile': view}


def _apply(pack, req):
    options = req['options']
    job_seed = req['job_seed']
    selection_id = req['selection_id']
    finfo = req['file']
    path = finfo['path']
    ext = finfo['ext']
    original_values = finfo.get('original_values', [])

    router = WriterRouter(pack)
    writer, resolved_ext, recipe = router.route(ext)
    if writer is None:
        return {'ok': False, 'stage': 'route', 'synthetic_state': 'unavailable_format',
                'error': 'Synthetic mode unavailable for this format'}

    try:
        profile = generate_profile(pack, options, job_seed, selection_id, ext)
    except ProfileError as exc:
        return {'ok': False, 'stage': 'generate', 'synthetic_state': 'unavailable_format',
                'error': str(exc)}

    violations = validate_profile(pack, profile, original_values=original_values,
                                  job_seed=job_seed, selection_id=selection_id)
    errors = [v for v in violations if v['severity'] == 'error']
    if errors:
        return {'ok': False, 'stage': 'validate', 'synthetic_state': 'failed_kept_clean',
                'error': 'consistency validation failed', 'violations': errors}

    try:
        baseline = writer.read_back(path)
    except Exception:
        baseline = None

    try:
        written = writer.write(profile, recipe, path)
    except Exception as exc:  # writer failures must never destroy the clean output (Rust restores)
        return {'ok': False, 'stage': 'write', 'synthetic_state': 'failed_kept_clean',
                'error': '%s: %s' % (type(exc).__name__, exc)}

    try:
        verification = verify_written(writer, path, written, original_values, baseline=baseline)
    except (VerifyError, OSError) as exc:
        return {'ok': False, 'stage': 'verify', 'synthetic_state': 'failed_kept_clean',
                'error': 'verification read-back failed: %s' % exc}

    if not verification['verified']:
        return {'ok': False, 'stage': 'verify', 'synthetic_state': 'failed_kept_clean',
                'error': 'synthetic verification failed', 'verification': verification}

    tripwire = req.get('tripwire') or {}
    source_url = tripwire.get('source_url')
    tripwire_state, tripwire_error, tripwire_written = 'not_requested', None, []
    if source_url:
        tripwire_state, tripwire_error, tripwire_written = _plant_tripwire(
            writer, recipe, path, source_url)

    return {'ok': True, 'written': written + tripwire_written,
            'verification': verification,
            'synthetic_state': 'applied_verified',
            'tripwire_state': tripwire_state,
            'tripwire_error': tripwire_error}


def _selftest(pack, req):
    formats = []
    for ext in sorted(pack['format_recipes'].keys()):
        support = synthetic_support(pack, ext)
        if support in ('tier1', 'tier2'):
            profile = generate_profile(pack, {
                'profile_scope': 'per_file', 'identity_mode': 'alias',
                'location_mode': 'gps', 'technical_mode': 'synthetic',
                'serial_mode': 'empty',
            }, 'selftest-seed', 'selftest-%s' % ext, ext)
            violations = [v for v in validate_profile(pack, profile) if v['severity'] == 'error']
            if violations:
                return {'ok': False, 'format': ext, 'violations': violations}
            formats.append(ext)
    return {'ok': True, 'formats': formats}


def handle_request(req):
    action = req.get('action')
    try:
        if action == 'validate_pack':
            return _validate_pack(req)
        pack, warnings = load_and_validate(req['pack_path'])
        if warnings:
            sys.stderr.write('pack warnings: %s\n' % '; '.join(warnings))
        if action == 'preview':
            return _preview(pack, req)
        if action == 'apply':
            return _apply(pack, req)
        if action == 'selftest':
            return _selftest(pack, req)
        return _fail('protocol', 'unknown action: %r' % action)
    except PackError as exc:
        return _fail('pack', exc)
    except KeyError as exc:
        return _fail('protocol', 'missing request field: %s' % exc)
    except json.JSONDecodeError as exc:
        return _fail('protocol', 'invalid JSON: %s' % exc)


def main():
    try:
        req = json.load(sys.stdin)
    except json.JSONDecodeError as exc:
        print(json.dumps(_fail('protocol', 'stdin is not valid JSON: %s' % exc)))
        return 1
    response = handle_request(req)
    print(json.dumps(response))
    return 0


if __name__ == '__main__':
    sys.exit(main())
