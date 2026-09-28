#!/usr/bin/env python3
"""S2/S3 tests: ProfileGenerator + ConsistencyValidator (HANDOFF §24)."""
import os
import sys
import unittest
from datetime import datetime

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

from synthetic_engine import pack_loader  # noqa: E402
from synthetic_engine.engine import (  # noqa: E402
    derive_rng, flatten_profile_fields, generate_profile, validate_profile,
)

REPO_ROOT = os.path.abspath(os.path.join(HERE, '..', '..'))
PACK_PATH = os.path.join(REPO_ROOT, 'addon-fauxmeta', 'synthetic_metadata_profiles_v1.json')

SEED = 'test-job-seed-0123456789abcdef'

BASE_OPTIONS = {
    'profile_scope': 'per_file',
    'identity_mode': 'alias',
    'location_mode': 'off',
    'technical_mode': 'synthetic',
    'serial_mode': 'empty',
}


def opts(**over):
    o = dict(BASE_OPTIONS)
    o.update(over)
    return o


class TestProfileGenerator(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.pack, _ = pack_loader.load_and_validate(PACK_PATH)

    def gen(self, ext, sel, options=None, now=None):
        return generate_profile(self.pack, options or BASE_OPTIONS, SEED, sel, ext,
                                now=now or datetime(2026, 9, 1))

    def test_deterministic_within_job(self):
        a = self.gen('jpg', 'sel-1')
        b = self.gen('jpg', 'sel-1')
        self.assertEqual(a, b, 'same (seed, selection) must reproduce the profile')

    def test_independent_profiles_differ(self):
        a = self.gen('jpg', 'sel-1')
        b = self.gen('jpg', 'sel-2')
        self.assertNotEqual(a['identifiers'], b['identifiers'])
        self.assertNotEqual(a['created'], b['created'])

    def test_timestamps_ordered_and_in_range(self):
        for i in range(25):
            p = self.gen('jpg', 'sel-%d' % i)
            self.assertLessEqual(p['created'], p['modified'])
            year = int(p['created'][:4])
            self.assertGreaterEqual(year, 2019)
            self.assertLessEqual(year, 2026)

    def test_device_release_guard(self):
        for i in range(40):
            p = self.gen('jpg', 'dev-%d' % i)
            if p['device']:
                self.assertGreaterEqual(int(p['created'][:4]),
                                        int(p['device']['earliest_plausible_year']))

    def test_same_locale_persona(self):
        seen = 0
        for i in range(60):
            p = self.gen('pdf', 'pers-%d' % i)
            if p['persona']:
                seen += 1
                pool = self.pack['name_pools'][p['persona']['locale']]
                self.assertIn(p['persona']['given'], pool['given'])
                self.assertIn(p['persona']['surname'], pool['surname'])
                self.assertEqual(p['persona']['full'],
                                 p['persona']['given'] + ' ' + p['persona']['surname'])
        self.assertGreater(seen, 0, 'expected some personas for pdf archetype')

    def test_serial_off_by_default(self):
        for i in range(40):
            p = self.gen('jpg', 'ser-%d' % i)
            self.assertIsNone(p['serial'])

    def test_serial_only_when_explicitly_enabled(self):
        found = 0
        for i in range(80):
            p = self.gen('jpg', 'seron-%d' % i, opts(serial_mode='generate'))
            if p['serial']:
                found += 1
                self.assertTrue(p['device'], 'serial requires a device context')
        self.assertGreater(found, 0)

    def test_semantic_fields_never_generated(self):
        for ext in ('jpg', 'png', 'pdf', 'mov', 'mp3', 'flac', 'docx', 'tiff', 'webp'):
            for i in range(12):
                p = self.gen(ext, 'sem-%s-%d' % (ext, i))
                fields = flatten_profile_fields(self.pack, p)
                for k in fields:
                    self.assertFalse(pack_loader._is_semantic(k),
                                     '%s generated semantic field %r' % (ext, k))

    def test_location_off_means_no_gps_no_location(self):
        for i in range(30):
            p = self.gen('jpg', 'loc-%d' % i)
            self.assertIsNone(p['gps'])
            self.assertIsNone(p['location'])

    def test_gps_opt_in_jitters_within_radius_and_rounds(self):
        got_gps = 0
        for i in range(120):
            p = self.gen('jpg', 'gps-%d' % i, opts(location_mode='gps'))
            if p['gps']:
                got_gps += 1
                self.assertIsNotNone(p['location'])
                city = next(l for l in self.pack['location_profiles']
                            if l['city'] == p['location']['city'])
                dlat = abs(p['gps']['lat'] - city['center_lat']) * 111.0
                self.assertLessEqual(dlat, city['gps_jitter_radius_km'] + 1)
                self.assertLessEqual(len(str(p['gps']['lat']).split('.')[-1]), 4)
        self.assertGreater(got_gps, 0)

    def test_location_timezone_consistency(self):
        for i in range(60):
            p = self.gen('jpg', 'tz-%d' % i, opts(location_mode='gps'))
            if p['gps'] and p['location']:
                self.assertEqual(p['timezone'], p['location']['timezone'])

    def test_identity_empty_means_no_persona(self):
        for i in range(30):
            p = self.gen('pdf', 'noid-%d' % i, opts(identity_mode='empty'))
            self.assertIsNone(p['persona'])

    def test_technical_empty_means_no_device_no_software(self):
        for i in range(30):
            p = self.gen('jpg', 'notech-%d' % i, opts(technical_mode='empty'))
            self.assertIsNone(p['device'])
            self.assertIsNone(p['software'])

    def test_consistent_batch_shares_context_not_uuids(self):
        o = opts(profile_scope='batch')
        a = self.gen('jpg', 'b-1', o)
        b = self.gen('jpg', 'b-2', o)
        self.assertEqual(a['archetype'], b['archetype'])
        self.assertEqual(a['device'], b['device'])
        self.assertEqual(a['persona'], b['persona'])
        self.assertEqual(a['timezone'], b['timezone'])
        self.assertNotEqual(a['identifiers']['instance_id'], b['identifiers']['instance_id'])
        self.assertNotEqual(a['identifiers']['document_id'], b['identifiers']['document_id'])

    def test_instance_ids_never_repeat_across_files(self):
        seen = set()
        for i in range(200):
            p = self.gen('jpg', 'uniq-%d' % i, opts(profile_scope='batch'))
            iid = p['identifiers']['instance_id']
            self.assertNotIn(iid, seen, 'XMP InstanceID reuse across files')
            seen.add(iid)

    def test_unsupported_and_deferred_formats_raise(self):
        from synthetic_engine.engine import ProfileError
        with self.assertRaises(ProfileError):
            self.gen('txt', 'x')
        with self.assertRaises(ProfileError):
            self.gen('odt', 'x')

    def test_created_not_in_future(self):
        now = datetime(2026, 9, 28)
        for i in range(30):
            p = self.gen('jpg', 'fut-%d' % i, now=now)
            self.assertLessEqual(p['created'], '2026:09:28 23:59:59')


class TestConsistencyValidator(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.pack, _ = pack_loader.load_and_validate(PACK_PATH)

    def gen(self, ext, sel, options=None):
        return generate_profile(self.pack, options or BASE_OPTIONS, SEED, sel, ext,
                                now=datetime(2026, 9, 1))

    def test_generated_profiles_pass_validation(self):
        for ext in ('jpg', 'png', 'pdf', 'mov', 'mp3', 'flac', 'docx', 'tiff', 'webp'):
            for i in range(10):
                p = self.gen(ext, 'val-%s-%d' % (ext, i))
                v = validate_profile(self.pack, p, original_values=('Alice Example', 'NIKON COOLPIX'),
                                     job_seed=SEED, selection_id='val-%s-%d' % (ext, i))
                errors = [x for x in v if x['severity'] == 'error']
                self.assertEqual(errors, [], '%s: %s' % (ext, errors))

    def test_validator_rejects_impossible_date_device(self):
        p = self.gen('jpg', 'bad-1')
        p['device'] = {'make': 'X', 'model': 'Y', 'earliest_plausible_year': 2100}
        v = validate_profile(self.pack, p)
        self.assertTrue(any(x['id'] == 'date_after_device_release' and x['severity'] == 'error' for x in v))

    def test_validator_rejects_reversed_dates(self):
        p = self.gen('jpg', 'bad-2')
        p['created'], p['modified'] = p['modified'], p['created']
        if p['created'] == p['modified']:
            p['modified'] = '2019:01:01 00:00:00'
            p['created'] = '2020:01:01 00:00:00'
        v = validate_profile(self.pack, p)
        self.assertTrue(any(x['id'] == 'date_order' for x in v))

    def test_validator_rejects_original_value_reuse(self):
        p = self.gen('jpg', 'bad-3')
        p['device'] = {'make': 'AliceCam', 'model': 'Example', 'earliest_plausible_year': 2019}
        v = validate_profile(self.pack, p, original_values=('AliceCam',))
        self.assertTrue(any(x['id'] == 'no_real_identifiers' for x in v))

    def test_validator_rejects_seed_leak(self):
        p = self.gen('jpg', 'bad-4')
        p['serial'] = SEED[:20]
        v = validate_profile(self.pack, p, job_seed=SEED, selection_id='bad-4')
        self.assertTrue(any(x['id'] == 'profile_provenance_internal_only' for x in v))

    def test_validator_rejects_cross_locale_persona(self):
        p = self.gen('pdf', 'bad-5')
        p['persona'] = {'locale': 'es-ES', 'given': 'Hans', 'surname': 'Müller', 'full': 'Hans Müller'}
        v = validate_profile(self.pack, p)
        self.assertTrue(any(x['id'] == 'same_locale_persona' for x in v))

    def test_derive_rng_is_csprng_seeded_and_stable(self):
        r1 = derive_rng(SEED, 'x')
        r2 = derive_rng(SEED, 'x')
        self.assertEqual(r1.randbytes(16), r2.randbytes(16))
        r3 = derive_rng(SEED + '0', 'x')
        self.assertNotEqual(derive_rng(SEED, 'x').randbytes(16), r3.randbytes(16))


if __name__ == '__main__':
    unittest.main(verbosity=2)
