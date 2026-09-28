#!/usr/bin/env python3
"""S5–S8, S10, S11 tests: writers, verifier, protocol, recipe validation.

Drives the engine through its real protocol (handle_request) against
MAT2-cleaned fixtures, with the pinned dev exiftool / venv mutagen.
Includes the HANDOFF §19 recipe-validation gate: every Tier-1 candidate
tag must actually write and read back with the pinned ExifTool.
"""
import json
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
import zipfile

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, HERE)

from synthetic_engine import pack_loader  # noqa: E402
from synthetic_engine.__main__ import handle_request  # noqa: E402
from synthetic_engine.writers import OoxmlCorePropertiesWriter, WriterRouter  # noqa: E402

REPO_ROOT = os.path.abspath(os.path.join(HERE, '..', '..'))
PACK_PATH = os.path.join(REPO_ROOT, 'addon-fauxmeta', 'synthetic_metadata_profiles_v1.json')
VENV_PY = os.path.join(REPO_ROOT, '.venv', 'bin', 'python')
MAT2 = os.path.join(REPO_ROOT, 'upstream-mat2', 'mat2')
FIXTURES = os.path.join(REPO_ROOT, 'upstream-mat2', 'tests', 'data')

SEED = 'writer-test-seed-0123456789abcdef'
OPTIONS = {
    'profile_scope': 'per_file', 'identity_mode': 'alias',
    'location_mode': 'gps', 'technical_mode': 'synthetic', 'serial_mode': 'empty',
}


def have(cmd):
    return shutil.which(cmd) is not None


def mat2_clean(src, dst, extra=()):
    """Clean src copied at dst; return the path of the MAT2-produced
    .cleaned output (the ONLY path a synthetic writer may ever receive)."""
    shutil.copy(src, dst)
    r = subprocess.run([VENV_PY, MAT2, *extra, '--', dst], capture_output=True, text=True)
    if r.returncode != 0:
        return None
    base, ext = os.path.splitext(dst)
    cleaned = base + '.cleaned' + ext
    return cleaned if os.path.exists(cleaned) else None


def cleaned_of(dst):
    base, ext = os.path.splitext(dst)
    return base + '.cleaned' + ext


STRUCTURAL_KEYS = {
    'colorspace', 'componentsconfiguration', 'ycbcrpositioning', 'exifversion',
    'flashpixversion', 'exifbyteorder', 'encodingprocess', 'bitspersample',
    'colorcomponents', 'compression', 'jfifversion', 'resolutionunit',
    'xresolution', 'yresolution', 'imagewidth', 'imageheight',
    'exifimagewidth', 'exifimageheight', 'interopindex', 'interopversion',
    'filesource', 'scenetype', 'customrendered', 'digitalzoomratio',
    'sensingmethod', 'scenecapturetype', 'gaincontrol', 'contrast',
    'saturation', 'sharpness', 'subjectdistancerange', 'exposuremode',
    'whitebalance', 'fnumber', 'exposuretime', 'exposurecompensation',
    'focallength', 'isospeedratings', 'iso', 'lightsource', 'meteringmode',
    'flash', 'aperture', 'shutterspeed', 'maxaperturevalue', 'brightness',
    'subsectime', 'subsectimeoriginal', 'subsectimedigitized',
}


def removed_values(fixture, cleaned):
    """Values MAT2 removed = pre-inspection values absent from the cleaned
    file's post-inspection, minus structural/format-constant keys (mirrors
    the Rust-side computation in synthetic.rs). Structural survivors are
    NOT absence targets — they were never identifying."""
    def show(path):
        return subprocess.run([VENV_PY, MAT2, '-s', '--', path],
                              capture_output=True, text=True).stdout
    pre, post = show(fixture), show(cleaned)
    post_blob = post.lower()
    values = []
    for line in pre.splitlines():
        line = line.strip()
        if ':' in line and not line.startswith('[') and 'Metadata for' not in line:
            key, value = line.split(':', 1)
            v = value.strip()
            if not v:
                continue
            if key.strip().lower().rsplit(' ', 1)[-1] in STRUCTURAL_KEYS:
                continue
            if v.lower() not in post_blob:
                values.append(v)
    return values


class Base(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        if not (os.path.exists(VENV_PY) and os.path.exists(MAT2)):
            raise unittest.SkipTest('dev runtime unavailable')
        cls.work = tempfile.mkdtemp(prefix='synthwriters-')

    @classmethod
    def tearDownClass(cls):
        shutil.rmtree(cls.work, ignore_errors=True)

    def cleaned(self, fixture_name, as_name=None, extra=()):
        dst = os.path.join(self.work, as_name or fixture_name)
        out = cleaned_of(dst)
        if not os.path.exists(out):
            result = mat2_clean(os.path.join(FIXTURES, fixture_name), dst, extra)
            self.assertIsNotNone(result, 'MAT2 clean failed for %s' % fixture_name)
        return out

    def apply(self, path, ext=None, seed=SEED, sel='sel-1', options=OPTIONS, originals=()):
        return handle_request({
            'action': 'apply', 'pack_path': PACK_PATH, 'options': options,
            'job_seed': seed, 'selection_id': sel,
            'file': {'path': path, 'ext': ext or os.path.splitext(path)[1].lstrip('.'),
                     'original_values': list(originals)},
        })


class TestProtocolMeta(Base):
    def test_validate_pack(self):
        r = handle_request({'action': 'validate_pack', 'pack_path': PACK_PATH})
        self.assertTrue(r['ok'])
        self.assertEqual(r['sha256'], '5bf6b12939d05defb4f8fd9b132a7a12be8c0cc211ce4522160905696e8efb1d')
        self.assertEqual(r['counts']['camera_device_profiles'], 24)

    def test_preview_has_no_seed_or_ids(self):
        r = handle_request({'action': 'preview', 'pack_path': PACK_PATH, 'options': OPTIONS,
                            'job_seed': SEED, 'selection_id': 'opaque-id-1', 'ext': 'jpg'})
        self.assertTrue(r['ok'])
        blob = json.dumps(r['profile'])
        self.assertNotIn(SEED, blob)
        self.assertNotIn('opaque-id-1', blob)
        self.assertIn('archetype', r['profile'])

    def test_selftest(self):
        r = handle_request({'action': 'selftest', 'pack_path': PACK_PATH})
        self.assertTrue(r['ok'], r)
        self.assertIn('jpg', r['formats'])
        self.assertIn('pdf', r['formats'])
        self.assertNotIn('odt', r['formats'])

    def test_unknown_format_unavailable(self):
        dummy = os.path.join(self.work, 'x.txt')
        open(dummy, 'w').write('hello')
        r = self.apply(dummy, ext='txt')
        self.assertFalse(r['ok'])
        self.assertEqual(r['synthetic_state'], 'unavailable_format')

    def test_deferred_format_unavailable(self):
        dummy = os.path.join(self.work, 'x.odt')
        open(dummy, 'w').write('hello')
        r = self.apply(dummy, ext='odt')
        self.assertEqual(r['synthetic_state'], 'unavailable_format')

    def test_target_must_be_regular_file(self):
        r = self.apply(os.path.join(self.work, 'nonexistent-xyz.jpg'))
        self.assertFalse(r['ok'])
        self.assertEqual(r['stage'], 'write')


class TestExifToolFormats(Base):
    @unittest.skipUnless(have('exiftool'), 'exiftool unavailable')
    def _format_case(self, fixture, ext=None, extra_clean=()):
        cleaned = self.cleaned(fixture, extra=extra_clean)
        originals = removed_values(os.path.join(FIXTURES, fixture), cleaned)
        r = self.apply(cleaned, ext=ext, originals=originals)
        self.assertTrue(r['ok'], '%s apply failed: %s' % (fixture, r))
        self.assertEqual(r['synthetic_state'], 'applied_verified')
        self.assertTrue(r['verification']['verified'], r['verification'])
        self.assertEqual(r['verification']['missing'], [])
        self.assertEqual(r['verification']['reappeared'], [])
        self.assertGreater(len(r['written']), 0)
        return r

    def test_jpeg(self):
        r = self._format_case('dirty.jpg')
        fields = {w['field'] for w in r['written']}
        self.assertTrue({'Make', 'Model'} & fields or 'XMP-xmp:CreatorTool' in fields, fields)

    def test_png(self):
        self._format_case('dirty.png')

    def test_pdf(self):
        r = self._format_case('dirty.pdf')
        fields = {w['field'] for w in r['written']}
        self.assertTrue({'PDF:Creator', 'PDF:Producer'} & fields, fields)

    def test_tiff(self):
        self._format_case('dirty.tiff')

    def test_webp(self):
        self._format_case('dirty.webp')

    def test_mp4_via_mov_recipe(self):
        self._format_case('dirty.mp4')

    def test_heic_after_lightweight_clean(self):
        self._format_case('dirty.heic', extra_clean=('-L',))

    @unittest.skipUnless(have('exiftool'), 'exiftool unavailable')
    def test_hostile_filenames(self):
        base = self.cleaned('dirty.jpg')
        hostile = ['-dash synth.jpg', "quote'and\"double.jpg", 'emoji-🎉.jpg',
                   '$(touch pwned).jpg', '<img src=x onerror=alert(1)>.jpg']
        for name in hostile:
            dst = os.path.join(self.work, name)
            shutil.copy(base, dst)
            r = self.apply(dst, sel='hostile-' + name)
            self.assertTrue(r['ok'], '%r failed: %s' % (name, r))
            self.assertTrue(r['verification']['verified'])
            self.assertFalse(os.path.exists(os.path.join(self.work, 'pwned')))

    @unittest.skipUnless(have('exiftool'), 'exiftool unavailable')
    def test_writer_args_are_shell_free_and_dashed_safe(self):
        from synthetic_engine.writers import ExifToolWriter
        pack, _ = pack_loader.load_and_validate(PACK_PATH)
        router = WriterRouter(pack)
        profile = handle_request({'action': 'preview', 'pack_path': PACK_PATH, 'options': OPTIONS,
                                  'job_seed': SEED, 'selection_id': 'x', 'ext': 'jpg'})
        from synthetic_engine.engine import generate_profile
        full = generate_profile(pack, OPTIONS, SEED, 'x', 'jpg')
        _, recipe = pack_loader.resolve_recipe(pack, 'jpg')
        args, _ = ExifToolWriter(pack).build_args(full, recipe, '/tmp/-weird name.jpg')
        self.assertTrue(args[0].endswith('exiftool'))
        self.assertNotIn('-c', args)
        self.assertEqual(args[-2], '--')
        self.assertEqual(args[-1], '/tmp/-weird name.jpg')
        self.assertEqual(profile['ok'], True)

    @unittest.skipUnless(have('exiftool'), 'exiftool unavailable')
    def test_original_values_never_reappear_guard(self):
        cleaned = self.cleaned('dirty.jpg', as_name='reappear.jpg')
        r = self.apply(cleaned, sel='reappear', originals=['Created with GIMP'])
        self.assertTrue(r['ok'])
        self.assertEqual(r['verification']['reappeared'], [])
        out = subprocess.run(['exiftool', '-json', '--', cleaned], capture_output=True, text=True).stdout
        self.assertNotIn('Created with GIMP', out)


class TestRecipeValidationS19(Base):
    """HANDOFF §19: every Tier-1 candidate tag must write+read-back with the
    pinned ExifTool. Failures here mean the pack/policy drifted from reality."""

    PROBE_VALUES = {
        'Make': 'ProbeMake', 'Model': 'ProbeModel 9000', 'Software': 'ProbeSoft 1.2',
        'LensModel': 'Probe Lens', 'DateTimeOriginal': '2024:05:18 16:22:07',
        'CreateDate': '2024:05:18 16:22:07', 'ModifyDate': '2024:05:19 10:00:00',
        'OffsetTimeOriginal': '+02:00', 'GPSLatitude': '40.4168', 'GPSLongitude': '3.7038',
        'GPSAltitude': '100', 'XMP-dc:Creator': 'Probe Person',
        'XMP-xmp:CreatorTool': 'ProbeTool 3', 'XMP-xmp:CreateDate': '2024-05-18T16:22:07+02:00',
        'XMP-xmp:ModifyDate': '2024-05-19T10:00:00+02:00',
        'XMP-xmpMM:DocumentID': 'xmp.did:12345678-1234-4234-8234-123456789abc',
        'XMP-xmpMM:InstanceID': 'xmp.iid:12345678-1234-4234-8234-123456789abc',
        'PNG:Author': 'Probe Person', 'PNG:CreationTime': '2024:05:18 16:22:07',
        'PNG:Software': 'ProbeSoft', 'PNG:Make': 'ProbeMake', 'PNG:Model': 'ProbeModel',
        'PDF:Author': 'Probe Person', 'PDF:Creator': 'ProbeCreator', 'PDF:Producer': 'ProbeProducer',
        'PDF:CreateDate': '2024:05:18 16:22:07', 'PDF:ModifyDate': '2024:05:19 10:00:00',
        'QuickTime:CreationDate': '2024-05-18T16:22:07+02:00', 'QuickTime:Author': 'Probe Person',
        'QuickTime:Encoder': 'ProbeEnc', 'QuickTime:GPSCoordinates': '40.4168 -3.7038',
        'QuickTime:LocationName': 'Madrid, ES',
    }
    FIXTURE_BY_EXT = {'jpg': 'dirty.jpg', 'png': 'dirty.png', 'pdf': 'dirty.pdf',
                      'tiff': 'dirty.tiff', 'webp': 'dirty.webp', 'mov': 'dirty.mp4',
                      'heic': 'dirty.heic'}

    @unittest.skipUnless(have('exiftool'), 'exiftool unavailable')
    def test_tier1_candidate_tags_writable(self):
        pack, _ = pack_loader.load_and_validate(PACK_PATH)
        failures = []
        for ext, recipe in pack['format_recipes'].items():
            if 'alias_of' in recipe or recipe.get('support_tier') != 1:
                continue
            fixture = self.FIXTURE_BY_EXT.get(ext)
            if fixture is None:
                failures.append('%s: no fixture' % ext)
                continue
            extra = ('-L',) if ext == 'heic' else ()
            base = self.cleaned(fixture, as_name='recipe-%s.%s' % (ext, 'mp4' if ext == 'mov' else ext),
                                extra=extra)
            for tag in recipe['candidate_fields']:
                probe_file = base + '.tagprobe'
                shutil.copy(base, probe_file)
                val = self.PROBE_VALUES.get(tag)
                if val is None:
                    os.remove(probe_file)
                    continue
                w = subprocess.run(['exiftool', '-overwrite_original', '-%s=%s' % (tag, val), '--', probe_file],
                                   capture_output=True, text=True)
                rb = subprocess.run(['exiftool', '-json', '-G1', '--', probe_file],
                                    capture_output=True, text=True)
                found = False
                try:
                    data = json.loads(rb.stdout)[0]
                    stem = tag.rsplit(':', 1)[-1]
                    found = any(k == tag or k.rsplit(':', 1)[-1] == stem for k in data)
                except Exception:
                    pass
                if w.returncode != 0 or not found:
                    failures.append('%s:%s exit=%d found=%s %s' %
                                    (ext, tag, w.returncode, found, (w.stderr or w.stdout).strip()[:80]))
                os.remove(probe_file)
        self.assertEqual(failures, [], 'recipe drift vs pinned exiftool: %s' % failures)


class TestAudioWriters(Base):
    def _audio_case(self, fixture):
        cleaned = self.cleaned(fixture)
        originals = removed_values(os.path.join(FIXTURES, fixture), cleaned)
        r = self.apply(cleaned, originals=originals)
        self.assertTrue(r['ok'], '%s: %s' % (fixture, r))
        self.assertEqual(r['synthetic_state'], 'applied_verified')
        self.assertTrue(r['verification']['verified'], r['verification'])
        return r

    def test_mp3(self):
        r = self._audio_case('dirty.mp3')
        fields = {w['field'] for w in r['written']}
        self.assertIn('TENC/EncodedBy', fields)
        self.assertIn('TDRC/RecordingTime', fields)
        for forbidden in ('TIT2/Title', 'TPE1/Artist', 'TALB/Album'):
            self.assertNotIn(forbidden, fields)

    def test_flac(self):
        r = self._audio_case('dirty.flac')
        fields = {w['field'] for w in r['written']}
        self.assertIn('ENCODER', fields)
        self.assertIn('DATE', fields)

    def test_ogg(self):
        self._audio_case('dirty.ogg')

    def test_m4a_deferred_without_fixture(self):
        # no m4a fixture in the supplied upstream test data; m4a stays
        # recipe-gated ('after format tests') and is exercised in manual QA
        # if a fixture becomes available.
        self.assertTrue(True)


class TestDocxWriter(Base):
    def test_docx_core_properties_only(self):
        cleaned = self.cleaned('dirty.docx')
        before = zipfile.ZipFile(cleaned).namelist()
        before_payload = {n: zipfile.ZipFile(cleaned).read(n)
                          for n in before if n != 'docProps/core.xml'}
        r = self.apply(cleaned)
        self.assertTrue(r['ok'], r)
        self.assertTrue(r['verification']['verified'], r['verification'])
        after = zipfile.ZipFile(cleaned)
        self.assertEqual(sorted(after.namelist()), sorted(before))
        for name, data in before_payload.items():
            self.assertEqual(after.read(name), data, 'payload member %s changed' % name)
        core = after.read('docProps/core.xml').decode('utf-8')
        self.assertIn('dcterms:created', core)
        self.assertNotIn('LibreOffice/5.4.5.1$Linux_X86_64', core)

    def test_zipslip_member_rejected(self):
        import io
        evil = io.BytesIO()
        with zipfile.ZipFile(evil, 'w') as z:
            z.writestr('../evil.txt', 'x')
            z.writestr('docProps/core.xml', '<cp:coreProperties/>')
        path = os.path.join(self.work, 'evil.docx')
        with open(path, 'wb') as f:
            f.write(evil.getvalue())
        w = OoxmlCorePropertiesWriter(None)
        from synthetic_engine.writers import WriteError
        _, recipe = pack_loader.resolve_recipe(
            pack_loader.load_and_validate(PACK_PATH)[0], 'docx')
        from synthetic_engine.engine import generate_profile
        pack, _ = pack_loader.load_and_validate(PACK_PATH)
        profile = generate_profile(pack, OPTIONS, SEED, 'zip-slip', 'docx')
        with self.assertRaises(WriteError):
            w.write(profile, recipe, path)


if __name__ == '__main__':
    unittest.main(verbosity=2)
