#!/usr/bin/env python3
"""S5–S7, S10, S11: WriterRouter + format writers.

Security rules (HANDOFF §23, owner constraints):
- writer arguments are constructed here, from typed profiles only — never
  from frontend input, never a shell, always argv arrays;
- writers receive ONLY the MAT2-cleaned staged file path (the Rust pipeline
  enforces this; the apply protocol additionally re-checks that the target
  is a regular file);
- recipe candidate_fields are the authority: a tag not in the recipe is
  never written; semantic fields are never written (loader already scrubbed
  candidates; writers re-check defensively);
- ExifTool/Mutagen are the copies bundled with the packaged runtime
  (PATH-injected by runtime_entry) or the dev environment's — never
  downloaded, never a second bundled copy.
"""
import io
import json
import os
import re
import shutil
import subprocess
import zipfile

from .pack_loader import _is_semantic, resolve_recipe


class WriteError(Exception):
    pass


def _run(argv, stdin_data=None):
    proc = subprocess.run(argv, input=stdin_data, capture_output=True, text=True)
    return proc


def _exiftool():
    path = shutil.which('exiftool')
    if not path:
        raise WriteError('exiftool not available in runtime PATH')
    return path


def _check_target(path):
    if not os.path.isfile(path) or os.path.islink(path):
        raise WriteError('synthetic target must be an existing regular file')


class ExifToolWriter:
    """Tier-1 writer: JPEG/PNG/TIFF/WebP/HEIC/PDF(after clean)/MOV/MP4."""

    def __init__(self, pack):
        self.pack = pack

    def handles(self, recipe):
        return recipe.get('writer') in ('exiftool', 'exiftool_after_mat2_clean_only')

    def build_args(self, profile, recipe, target):
        candidates = set(recipe.get('candidate_fields', []))
        args = [_exiftool(), '-overwrite_original', '-charset', 'filename=utf8']
        written = []

        def add(tag, value):
            if tag in candidates and value is not None:
                if _is_semantic(tag):
                    return
                args.append('-%s=%s' % (tag, value))
                written.append({'field': tag, 'value': str(value)})

        dev = profile.get('device')
        if dev:
            add('Make', dev['make'])
            add('Model', dev['model'])
        if profile.get('lens'):
            add('LensModel', profile['lens'])
        sw = profile.get('software') or {}
        software_name = sw.get('software') or sw.get('creator')
        add('Software', software_name)
        add('PNG:Software', software_name)
        add('XMP-xmp:CreatorTool', software_name)
        add('QuickTime:Encoder', sw.get('encoder') or software_name)
        producer = sw.get('producer')
        add('PDF:Producer', producer)
        add('PDF:Creator', software_name)

        persona = profile.get('persona')
        author = persona['full'] if persona else None
        add('XMP-dc:Creator', author)
        add('PNG:Author', author)
        add('PDF:Author', author)
        add('QuickTime:Author', author)

        created, modified = profile['created'], profile['modified']
        created_iso, modified_iso = profile['created_iso'], profile['modified_iso']
        add('DateTimeOriginal', created)
        add('CreateDate', created)
        add('ModifyDate', modified)
        add('PNG:CreationTime', created)
        add('PDF:CreateDate', created)
        add('PDF:ModifyDate', modified)
        add('QuickTime:CreationDate', created_iso)
        add('XMP-xmp:CreateDate', created_iso)
        add('XMP-xmp:ModifyDate', modified_iso)
        add('OffsetTimeOriginal', profile.get('utc_offset'))

        gps = profile.get('gps')
        if gps:
            lat, lon = gps['lat'], gps['lon']
            if 'GPSLatitude' in candidates:
                args.append('-GPSLatitude=%s' % abs(lat))
                args.append('-GPSLatitudeRef=%s' % ('N' if lat >= 0 else 'S'))
                args.append('-GPSLongitude=%s' % abs(lon))
                args.append('-GPSLongitudeRef=%s' % ('E' if lon >= 0 else 'W'))
                written.append({'field': 'GPSLatitude', 'value': str(lat)})
                written.append({'field': 'GPSLongitude', 'value': str(lon)})
            if 'QuickTime:GPSCoordinates' in candidates:
                add('QuickTime:GPSCoordinates', '%s %s' % (lat, lon))
        loc = profile.get('location')
        if loc and 'QuickTime:LocationName' in candidates:
            add('QuickTime:LocationName', '%s, %s' % (loc['city'], loc['country_code']))

        ids = profile.get('identifiers', {})
        add('XMP-xmpMM:DocumentID', ids.get('document_id'))
        add('XMP-xmpMM:InstanceID', ids.get('instance_id'))

        if profile.get('serial'):
            for tag in ('BodySerialNumber', 'SerialNumber'):
                add(tag, profile['serial'])

        args.append('--')
        args.append(target)
        return args, written

    def write(self, profile, recipe, target):
        _check_target(target)
        args, written = self.build_args(profile, recipe, target)
        if not written:
            raise WriteError('profile maps to zero writable fields for this recipe')
        proc = _run(args)
        if proc.returncode != 0:
            raise WriteError('exiftool failed (exit %d): %s' %
                             (proc.returncode, (proc.stderr or proc.stdout).strip()[:300]))
        out = (proc.stdout or '') + (proc.stderr or '')
        error_lines = [l.strip() for l in out.splitlines() if re.search(r'\bError\b', l)]
        if error_lines:
            raise WriteError('exiftool reported: %s' % error_lines[0][:300])
        return written

    def read_back(self, target):
        proc = _run([_exiftool(), '-json', '-G1', '-charset', 'filename=utf8', '--', target])
        if proc.returncode != 0:
            raise WriteError('exiftool read-back failed: %s' % proc.stderr.strip()[:200])
        try:
            data = json.loads(proc.stdout)
            return data[0] if data else {}
        except (ValueError, IndexError):
            raise WriteError('exiftool read-back produced invalid JSON')


class MutagenWriter:
    """Tier-2 audio writer: MP3 (ID3 TENC/TSSE/TDRC/TDTG), FLAC/OGG
    (Vorbis ENCODER/DATE). Technical provenance only — never
    Artist/Album/Title/Composer (HANDOFF §17)."""

    def __init__(self, pack):
        self.pack = pack

    def handles(self, recipe):
        return str(recipe.get('writer', '')).startswith('mutagen')

    def write(self, profile, recipe, target):
        _check_target(target)
        ext = os.path.splitext(target)[1].lower().lstrip('.')
        sw = profile.get('software') or {}
        encoded_by = sw.get('encoded_by') or sw.get('software')
        written = []
        if ext == 'mp3':
            from mutagen.id3 import ID3, TENC, TDRC, TDTG, TSSE
            from mutagen.mp3 import MP3
            audio = MP3(target)
            if audio.tags is None:
                audio.add_tags()
            tags = audio.tags
            if encoded_by:
                tags.add(TENC(encoding=3, text=[encoded_by]))
                written.append({'field': 'TENC/EncodedBy', 'value': encoded_by})
                tags.add(TSSE(encoding=3, text=[encoded_by]))
                written.append({'field': 'TSSE/EncoderSettings', 'value': encoded_by})
            tags.add(TDRC(encoding=3, text=[profile['created_iso']]))
            written.append({'field': 'TDRC/RecordingTime', 'value': profile['created_iso']})
            tags.add(TDTG(encoding=3, text=[profile['modified_iso']]))
            written.append({'field': 'TDTG/TaggingTime', 'value': profile['modified_iso']})
            audio.save()
        elif ext in ('flac', 'ogg'):
            if ext == 'flac':
                from mutagen.flac import FLAC
                audio = FLAC(target)
            else:
                from mutagen.oggvorbis import OggVorbis
                audio = OggVorbis(target)
            if audio.tags is None:
                audio.add_tags()
            if encoded_by:
                audio.tags['ENCODER'] = [encoded_by]
                written.append({'field': 'ENCODER', 'value': encoded_by})
            audio.tags['DATE'] = [profile['created'][:10].replace(':', '-')]
            written.append({'field': 'DATE', 'value': profile['created'][:10].replace(':', '-')})
            audio.save()
        elif ext == 'm4a':
            from mutagen.mp4 import MP4
            audio = MP4(target)
            if audio.tags is None:
                audio.add_tags()
            if encoded_by:
                audio.tags['\xa9too'] = [encoded_by]
                written.append({'field': 'encoder', 'value': encoded_by})
            audio.save()
        else:
            raise WriteError('mutagen writer: unsupported extension %r' % ext)
        if not written:
            raise WriteError('no audio fields written')
        return written

    def read_back(self, target):
        ext = os.path.splitext(target)[1].lower().lstrip('.')
        out = {}
        if ext == 'mp3':
            from mutagen.mp3 import MP3
            audio = MP3(target)
            if audio.tags:
                mapping = {'TENC': 'TENC/EncodedBy', 'TSSE': 'TSSE/EncoderSettings',
                           'TDRC': 'TDRC/RecordingTime', 'TDTG': 'TDTG/TaggingTime'}
                for fid, key in mapping.items():
                    frame = audio.tags.getall(fid)
                    if frame and frame[0].text:
                        out[key] = str(frame[0].text[0])
        elif ext in ('flac', 'ogg'):
            if ext == 'flac':
                from mutagen.flac import FLAC
                audio = FLAC(target)
            else:
                from mutagen.oggvorbis import OggVorbis
                audio = OggVorbis(target)
            if audio.tags:
                for k in ('ENCODER', 'DATE'):
                    v = audio.tags.get(k)
                    if v:
                        out[k] = v[0]
        elif ext == 'm4a':
            from mutagen.mp4 import MP4
            audio = MP4(target)
            if audio.tags and '\xa9too' in audio.tags:
                out['encoder'] = audio.tags['\xa9too'][0]
        return out


class OoxmlCorePropertiesWriter:
    """Tier-2 DOCX writer: rewrites ONLY docProps/core.xml inside a fresh
    in-memory archive. Never extracts entries to disk; rejects archives
    with unsafe member names (ZipSlip defense even though we don't extract).
    Document payload bytes are copied verbatim."""

    NAMESPACES = (
        'xmlns:cp="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" '
        'xmlns:dc="http://purl.org/dc/elements/1.1/" '
        'xmlns:dcterms="http://schemas.openxmlformats.org/package/2006/metadata/core-properties" '
        'xmlns:dcmitype="http://purl.org/dc/dcmitype/" '
        'xmlns:xsi="http://www.w3.org/2001/XMLSchema-instance"'
    )

    def __init__(self, pack):
        self.pack = pack

    def handles(self, recipe):
        return recipe.get('writer') == 'safe_ooxml_core_properties_adapter'

    @staticmethod
    def _safe_member(name):
        if name.startswith('/') or name.startswith('\\'):
            return False
        if ':' in name.replace(':', '', 0) and re.match(r'^[A-Za-z]:', name):
            return False
        parts = re.split(r'[\\/]', name)
        return not any(p == '..' for p in parts)

    def _core_xml(self, profile, recipe):
        persona = profile.get('persona')
        creator = persona['full'] if persona else None
        created = profile['created_iso']
        modified = profile['modified_iso']
        candidates = recipe.get('candidate_fields', [])
        parts = ['<?xml version="1.0" encoding="UTF-8" standalone="yes"?>',
                 '<cp:coreProperties %s>' % self.NAMESPACES]
        if creator and 'dc:creator' in candidates:
            parts.append('<dc:creator>%s</dc:creator>' % _xml_escape(creator))
        if creator and 'cp:lastModifiedBy' in candidates:
            parts.append('<cp:lastModifiedBy>%s</cp:lastModifiedBy>' % _xml_escape(creator))
        if created and 'dcterms:created' in candidates:
            parts.append('<dcterms:created xsi:type="dcterms:W3CDTF">%s</dcterms:created>' % created)
        if modified and 'dcterms:modified' in candidates:
            parts.append('<dcterms:modified xsi:type="dcterms:W3CDTF">%s</dcterms:modified>' % modified)
        parts.append('</cp:coreProperties>')
        return ''.join(parts).encode('utf-8')

    def write(self, profile, recipe, target):
        _check_target(target)
        with open(target, 'rb') as f:
            original = f.read()
        try:
            src = zipfile.ZipFile(io.BytesIO(original), 'r')
        except zipfile.BadZipFile:
            raise WriteError('docx is not a valid zip archive')
        with src:
            for name in src.namelist():
                if not self._safe_member(name):
                    raise WriteError('docx archive contains unsafe member name; refusing')
            core_xml = self._core_xml(profile, recipe)
            buf = io.BytesIO()
            with zipfile.ZipFile(buf, 'w', compression=zipfile.ZIP_DEFLATED) as dst:
                for info in src.infolist():
                    data = core_xml if info.filename == 'docProps/core.xml' else src.read(info.filename)
                    if info.filename == 'docProps/core.xml':
                        zi = zipfile.ZipInfo(info.filename, date_time=info.date_time)
                        zi.compress_type = zipfile.ZIP_DEFLATED
                        dst.writestr(zi, data)
                    else:
                        dst.writestr(info, data)
                if 'docProps/core.xml' not in src.namelist():
                    dst.writestr('docProps/core.xml', core_xml)
        tmp = target + '.synthtmp'
        with open(tmp, 'wb') as f:
            f.write(buf.getvalue())
            f.flush()
            os.fsync(f.fileno())
        os.replace(tmp, target)

        written = []
        persona = profile.get('persona')
        if persona and 'dc:creator' in recipe.get('candidate_fields', []):
            written.append({'field': 'dc:creator', 'value': persona['full']})
        if 'dcterms:created' in recipe.get('candidate_fields', []):
            written.append({'field': 'dcterms:created', 'value': profile['created_iso']})
        if 'dcterms:modified' in recipe.get('candidate_fields', []):
            written.append({'field': 'dcterms:modified', 'value': profile['modified_iso']})
        if not written:
            raise WriteError('docx writer produced no fields')
        return written

    def read_back(self, target):
        out = {}
        with zipfile.ZipFile(target, 'r') as z:
            if 'docProps/core.xml' not in z.namelist():
                return out
            xml = z.read('docProps/core.xml').decode('utf-8', 'replace')
        for tag in ('dc:creator', 'cp:lastModifiedBy', 'dcterms:created', 'dcterms:modified'):
            m = re.search(r'<%s[^>]*>([^<]*)</%s>' % (re.escape(tag), re.escape(tag)), xml)
            if m:
                out[tag] = m.group(1)
        return out


def _xml_escape(s):
    return (s.replace('&', '&amp;').replace('<', '&lt;').replace('>', '&gt;')
             .replace('"', '&quot;').replace("'", '&apos;'))


class WriterRouter:
    def __init__(self, pack):
        self.pack = pack
        self.exiftool = ExifToolWriter(pack)
        self.mutagen = MutagenWriter(pack)
        self.ooxml = OoxmlCorePropertiesWriter(pack)

    def route(self, extension):
        """Return (writer, ext, recipe) or (None, ext, None) when synthetic
        mode is unavailable for the format (MAT2 cleaning still applies)."""
        ext, recipe = resolve_recipe(self.pack, extension)
        if recipe is None or recipe.get('writer') == 'deferred':
            return None, ext, recipe
        for w in (self.exiftool, self.mutagen, self.ooxml):
            if w.handles(recipe):
                return w, ext, recipe
        return None, ext, recipe
