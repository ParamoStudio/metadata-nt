#!/usr/bin/env python3
"""Tests for the synthetic metadata engine (HANDOFF §24 checklist).

Run: .venv/bin/python -m unittest discover -s scripts/packaging -p 'tests_synthetic*.py' -v
(or with the packaged runtime: mat2-runtime synthetic --selftest)
"""
import json
import os
import sys
import unittest

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

from synthetic_engine import pack_loader  # noqa: E402

REPO_ROOT = os.path.abspath(os.path.join(HERE, '..', '..'))
PACK_PATH = os.path.join(REPO_ROOT, 'addon-fauxmeta', 'synthetic_metadata_profiles_v1.json')
EXPECTED_PACK_SHA256 = '5bf6b12939d05defb4f8fd9b132a7a12be8c0cc211ce4522160905696e8efb1d'


class TestPackLoader(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.pack, cls.warnings = pack_loader.load_and_validate(PACK_PATH)

    def test_checksum_matches_release_record(self):
        self.assertEqual(pack_loader.sha256_file(PACK_PATH), EXPECTED_PACK_SHA256)

    def test_schema_and_sections(self):
        self.assertEqual(self.pack['schema_version'], '1.0.0')
        for section in pack_loader.REQUIRED_SECTIONS:
            self.assertIn(section, self.pack)

    def test_semantic_sparsity_key_filtered_with_warning(self):
        pdf_arch = next(a for a in self.pack['archetypes'] if a['id'] == 'office_pdf_export')
        self.assertNotIn('title', pdf_arch['sparsity'])
        self.assertTrue(any('title' in w for w in self.warnings), self.warnings)

    def test_no_semantic_candidate_fields_survive(self):
        for ext, recipe in self.pack['format_recipes'].items():
            if 'alias_of' in recipe:
                continue
            for field in recipe.get('candidate_fields', []):
                self.assertFalse(pack_loader._is_semantic(field),
                                 'semantic field %r survived in %s' % (field, ext))

    def test_aliases_resolve(self):
        self.assertEqual(pack_loader.resolve_recipe(self.pack, 'jpeg')[0], 'jpg')
        self.assertEqual(pack_loader.resolve_recipe(self.pack, 'MP4')[0], 'mov')
        self.assertIsNone(pack_loader.resolve_recipe(self.pack, 'xyz')[1])

    def test_support_classification(self):
        self.assertEqual(pack_loader.synthetic_support(self.pack, 'jpg'), 'tier1')
        self.assertEqual(pack_loader.synthetic_support(self.pack, 'pdf'), 'tier1')
        self.assertEqual(pack_loader.synthetic_support(self.pack, 'mp3'), 'tier2')
        self.assertEqual(pack_loader.synthetic_support(self.pack, 'docx'), 'tier2')
        self.assertEqual(pack_loader.synthetic_support(self.pack, 'odt'), 'deferred')
        self.assertEqual(pack_loader.synthetic_support(self.pack, 'txt'), 'unsupported')

    def test_malformed_pack_rejected(self):
        import copy
        import tempfile
        bad = copy.deepcopy(self.pack)
        bad['schema_version'] = '2.0.0'
        with tempfile.NamedTemporaryFile('w', suffix='.json', delete=False) as f:
            json.dump(bad, f)
            p = f.name
        try:
            with self.assertRaises(pack_loader.PackError):
                pack_loader.load_and_validate(p)
        finally:
            os.unlink(p)

        bad2 = copy.deepcopy(self.pack)
        bad2['defaults']['synthetic_mode'] = 'on'
        with tempfile.NamedTemporaryFile('w', suffix='.json', delete=False) as f:
            json.dump(bad2, f)
            p2 = f.name
        try:
            with self.assertRaises(pack_loader.PackError):
                pack_loader.load_and_validate(p2)
        finally:
            os.unlink(p2)

        bad3 = copy.deepcopy(self.pack)
        bad3['location_profiles'][0]['center_lat'] = 999
        with tempfile.NamedTemporaryFile('w', suffix='.json', delete=False) as f:
            json.dump(bad3, f)
            p3 = f.name
        try:
            with self.assertRaises(pack_loader.PackError):
                pack_loader.load_and_validate(p3)
        finally:
            os.unlink(p3)


if __name__ == '__main__':
    unittest.main(verbosity=2)
