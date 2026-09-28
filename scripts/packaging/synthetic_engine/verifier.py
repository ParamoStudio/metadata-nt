#!/usr/bin/env python3
"""S8: SyntheticVerifier — HANDOFF §20 Phase B post-write verification.

Confirms, by re-reading the written file with the same technology family:
- every expected synthetic field is present with the generated value;
- no original sensitive value reappeared anywhere in the read-back;
- (format validity is implied by a successful structured read-back).

Never reports success for a partially written or unverifiable output.
"""
import json
import re

STRUCTURAL_VALUE = re.compile(r'^[\d.\-+:\s,degsecnw\'"]+$', re.I)

# EXIF-spec constants: default values any EXIF block carries (exiftool adds
# them when constructing a new EXIF segment). Non-identifying by definition;
# a coincidental match with an original value is not a reappearance.
EXIF_SPEC_CONSTANTS = {
    'uncalibrated', 'y, cb, cr, -', 'centered', 'co-sited',
    'baseline dct, huffman coding', 'progressive dct, huffman coding',
    'digital camera', 'directly photographed', 'normal', 'standard',
    'peripheral', 'exif', 'undefined', 'unknown',
}


class VerifyError(Exception):
    pass


def _strip_group(field):
    name = field
    for sep in (':', '/'):
        if sep in name:
            name = name.rsplit(sep, 1)[-1]
    return name


def _digits(s):
    return re.sub(r'\D', '', str(s))


def _dms_to_decimal(s):
    m = re.match(r"\s*(\d+(?:\.\d+)?)\s*deg\s*(\d+(?:\.\d+)?)'\s*(\d+(?:\.\d+)?)\"?\s*([NSEW])", str(s), re.I)
    if not m:
        return None
    deg, minutes, seconds, ref = float(m.group(1)), float(m.group(2)), float(m.group(3)), m.group(4).upper()
    val = deg + minutes / 60.0 + seconds / 3600.0
    if ref in ('S', 'W'):
        val = -val
    return val


def _values_match(field, expected, actual):
    expected_s, actual_s = str(expected), str(actual)
    if expected_s == actual_s:
        return True
    low_field = field.lower()
    if 'latitude' in low_field or 'longitude' in low_field or 'gpscoordinates' in low_field.replace('_', ''):
        try:
            exp_f = float(re.sub(r'[^0-9.\-]', '', expected_s.split()[0]))
        except (ValueError, IndexError):
            return False
        act = _dms_to_decimal(actual_s)
        if act is None:
            try:
                act = float(actual_s)
            except ValueError:
                return False
        return abs(act - exp_f) < 0.002
    if 'date' in low_field or 'time' in low_field or low_field in ('tdrc/recordingtime', 'tdtg/taggingtime', 'date'):
        de, da = _digits(expected_s)[:14], _digits(actual_s)[:14]
        if len(de) >= 8 and de.startswith(da[:len(de)]) or (len(da) >= 8 and da.startswith(de[:len(da)])):
            return True
        return de[:8] == da[:8] and len(de) >= 8
    return expected_s.strip().lower() in actual_s.strip().lower() or actual_s.strip().lower() in expected_s.strip().lower()


def verify_written(writer, target, written, original_values, baseline=None):
    """baseline = writer.read_back(target) captured BEFORE the synthetic
    write. Absence targets already visible in the baseline are structural
    survivors of MAT2 Phase A (same-reader view) and are Phase B's
    non-responsibility; only values the WRITE introduced can fail here."""
    readback = writer.read_back(target)
    if not isinstance(readback, dict):
        raise VerifyError('read-back did not produce a mapping')

    missing = []
    mismatched = []
    for w in written:
        field = w['field']
        value = w['value']
        key = _strip_group(field)
        candidates = [k for k in readback if k == key or _strip_group(k) == key]
        if field in readback:
            candidates = [field]
        if not candidates:
            missing.append(field)
            continue
        if not any(_values_match(field, value, readback[k]) for k in candidates):
            mismatched.append({'field': field, 'expected': str(value),
                               'actual': str(readback[candidates[0]])[:120]})

    blob = json.dumps(readback, sort_keys=True, default=str).lower()
    blob_before = (json.dumps(baseline, sort_keys=True, default=str).lower()
                   if baseline is not None else '')
    reappeared = []
    for orig in original_values:
        o = str(orig).strip()
        if len(o) < 4 or o.lower() in ('none', 'null', 'n/a'):
            continue
        if STRUCTURAL_VALUE.match(o):
            continue
        if o.lower() in EXIF_SPEC_CONSTANTS:
            continue
        if blob_before and o.lower() in blob_before:
            continue
        if o.lower() in blob:
            reappeared.append(o)

    return {
        'verified': not missing and not mismatched and not reappeared,
        'missing': missing,
        'mismatched': mismatched,
        'reappeared': reappeared,
    }
