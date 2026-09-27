#!/usr/bin/env python3
"""Read-only structured inspection adapter for the MAT2 Wrapper.

Uses the supplied upstream libmat2 API (parser_factory.get_parser / get_meta) —
exactly what `mat2 --show` uses internally — and emits the detected metadata
as one JSON object on stdout. Never modifies any file, never alters
sanitisation semantics. Lives OUTSIDE the upstream tree by design.

Usage: mat2_inspect.py <upstream_dir> <file>

Exit codes:
  0 — a JSON protocol object was printed (including unsupported/invalid files)
  1 — fatal: libmat2 could not be imported (runtime broken)
  2 — usage error
"""
import json
import sys
import unicodedata


def _strip_controls(value):
    # Mirrors the CLI: remove Unicode category C* (control/format) characters.
    return ''.join(ch for ch in value if not unicodedata.category(ch).startswith('C'))


def _clean(obj):
    if isinstance(obj, dict):
        return {str(k): _clean(v) for k, v in obj.items()}
    if isinstance(obj, (list, tuple)):
        return [_clean(v) for v in obj]
    if isinstance(obj, str):
        return _strip_controls(obj)
    return _strip_controls(str(obj))


def main(argv):
    if len(argv) != 3:
        print(json.dumps({"ok": False, "error": "usage: mat2_inspect.py <upstream_dir> <file>"}))
        return 2
    upstream_dir, filename = argv[1], argv[2]
    sys.path.insert(0, upstream_dir)
    try:
        from libmat2 import parser_factory
    except Exception as exc:
        print(json.dumps({"ok": False, "error": "libmat2 import failed: %s" % exc}))
        return 1
    try:
        parser, mtype = parser_factory.get_parser(filename)
    except ValueError as exc:
        print(json.dumps({"ok": False, "error": "invalid file: %s" % exc}))
        return 0
    except Exception as exc:
        print(json.dumps({"ok": False, "error": "unexpected error: %s" % exc}))
        return 0
    if parser is None:
        print(json.dumps({"ok": True, "supported": False, "mimetype": mtype}))
        return 0
    try:
        meta = parser.get_meta()
    except Exception as exc:
        print(json.dumps({"ok": False, "error": "get_meta failed: %s" % exc}))
        return 0
    print(json.dumps(
        {"ok": True, "supported": True, "mimetype": mtype, "metadata": _clean(meta)},
        ensure_ascii=True, sort_keys=True))
    return 0


if __name__ == '__main__':
    sys.exit(main(sys.argv))
