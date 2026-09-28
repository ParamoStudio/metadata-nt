#!/usr/bin/env python3
"""Frozen MAT2 runtime entry (MAT2 Wrapper packaging — Task 19).

Dispatch:
  mat2-runtime mat2 <args...>            upstream MAT2 CLI semantics
  mat2-runtime inspect <upstream> <file> read-only libmat2 JSON inspection

Design notes (documented deviation per IMPLEMENTATION_PLAN Task 9/19 rules —
sanitisation semantics are NEVER altered; only the process model is):

- Diagnostics (-v/--version, -l/--list, --check-dependencies, -h/--help, no
  files) execute the bundled upstream `mat2` script verbatim via runpy.
- File-processing modes (--show and cleaning) call the upstream script's OWN
  functions (show_meta / clean_meta / create_arg_parser / UnknownMemberPolicy)
  imported as a module, sequentially, instead of upstream's
  ProcessPoolExecutor. Reason: PyInstaller-frozen processes cannot re-exec
  spawn-based pool workers (the bootloader re-enters this dispatcher). The
  wrapper invokes exactly one file per process by design (HANDOFF §16:
  explicit file lists, no directories), so per-file semantics, messages and
  exit codes are identical; only intra-process parallelism differs.
- Bundled helper binaries (ffmpeg, exiftool) are discovered via PATH,
  matching upstream's shutil.which lookup. exiftool runs on system perl.
- The bundled upstream tree is the supplied source verbatim (pruned of
  tests/CI/desktop-integration files; recorded in the package manifest).
"""
import logging
import multiprocessing
import os
import runpy
import sys


def _internal() -> str:
    return getattr(sys, '_MEIPASS', os.path.dirname(os.path.abspath(__file__)))


def _load_mat2_module(upstream: str):
    import importlib.util
    from importlib.machinery import SourceFileLoader
    script = os.path.join(upstream, 'mat2')
    loader = SourceFileLoader('mat2cli', script)
    spec = importlib.util.spec_from_loader('mat2cli', loader)
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _run_mat2(rest, internal: str, upstream: str) -> int:
    script = os.path.join(upstream, 'mat2')
    if not os.path.exists(script):
        print('bundled mat2 entrypoint missing')
        return 1

    processing_flags = ('-v', '--version', '-l', '--list', '--check-dependencies', '-h', '--help')
    if not rest or any(a in processing_flags for a in rest):
        sys.argv = ['mat2'] + rest
        runpy.run_path(script, run_name='__main__')
        return 0

    mat2cli = _load_mat2_module(upstream)
    args = mat2cli.create_arg_parser().parse_args(rest)

    if getattr(args, 'sandbox', False):
        import warnings
        warnings.warn('sandboxing support has been removed', DeprecationWarning)
    if args.verbose:
        logging.getLogger().setLevel(logging.DEBUG)
        logging.getLogger('mat2cli').setLevel(logging.DEBUG)

    if not args.files:
        sys.argv = ['mat2'] + rest
        runpy.run_path(script, run_name='__main__')
        return 0

    if args.show:
        for f in args.files:
            mat2cli.show_meta(f)
        return 0

    policy = mat2cli.UnknownMemberPolicy(args.unknown_members)
    if policy == mat2cli.UnknownMemberPolicy.KEEP:
        logging.warning('Keeping unknown member files may leak metadata in the resulting file!')
    no_failure = True
    for f in args.files:
        no_failure &= mat2cli.clean_meta(f, args.lightweight, args.inplace, policy)
    return 0 if no_failure is True else -1


def _probe() -> int:
    """Packaging QA: verify bundled native stacks actually load and render."""
    import json
    info = {}
    try:
        import cairo
        info['cairo'] = getattr(cairo, 'version', 'unknown')
    except Exception as exc:
        info['cairo'] = 'ERROR: %s' % exc
    try:
        import gi
        gi.require_foreign('cairo', 'Context')
        info['cairo_foreign'] = 'ok'
    except Exception as exc:
        info['cairo_foreign'] = 'ERROR: %s' % exc
    try:
        import gi
        gi.require_version('Poppler', '0.18')
        from gi.repository import Poppler
        Poppler.Document.new_from_file('file:///nonexistent-probe.pdf', None)
        info['poppler'] = 'unexpected: no error'
    except Exception as exc:
        msg = str(exc)
        if 'Could not locate' in msg or 'Failed to load shared library' in msg:
            info['poppler'] = 'ERROR (dylib missing): %s' % msg[:160]
        else:
            info['poppler'] = 'ok (symbols resolved; open failed as expected)'
    try:
        import cairo as _c
        import gi
        gi.require_version('Rsvg', '2.0')
        from gi.repository import Rsvg
        surface = _c.ImageSurface(_c.FORMAT_ARGB32, 8, 8)
        ctx = _c.Context(surface)
        handle = Rsvg.Handle.new_from_data(
            b'<svg xmlns="http://www.w3.org/2000/svg" width="8" height="8">'
            b'<rect width="8" height="8" fill="red"/></svg>')
        handle.render_cairo(ctx)
        info['rsvg_render'] = 'ok'
    except Exception as exc:
        info['rsvg_render'] = 'ERROR: %s' % str(exc)[:160]
    print(json.dumps(info, indent=1))
    failed = any(str(v).startswith('ERROR') for v in info.values())
    return 1 if failed else 0


def main() -> int:
    internal = _internal()

    bindir = os.path.join(internal, 'bin')
    path_dirs = [d for d in (bindir, internal) if os.path.isdir(d)]
    if path_dirs:
        os.environ['PATH'] = os.pathsep.join(path_dirs) + os.pathsep + os.environ.get('PATH', '')
    if '/usr/bin' not in os.environ.get('PATH', ''):
        os.environ['PATH'] = os.environ.get('PATH', '') + os.pathsep + '/usr/bin:/bin:/usr/sbin:/sbin'
    exifdir = os.path.join(internal, 'exiftool')
    if os.path.isdir(os.path.join(exifdir, 'lib')):
        os.environ['PERL5LIB'] = os.path.join(exifdir, 'lib') + os.pathsep + os.environ.get('PERL5LIB', '')
    upstream = os.path.join(internal, 'upstream')
    if os.path.isdir(upstream) and upstream not in sys.path:
        sys.path.insert(0, upstream)

    try:
        import gi
        gi.require_foreign('cairo', 'Context')
    except Exception:
        pass

    argv = sys.argv[1:]
    if not argv:
        print('usage: mat2-runtime mat2 <args...> | inspect <upstream_dir> <file>')
        return 2
    cmd, rest = argv[0], argv[1:]
    if cmd == 'mat2':
        return _run_mat2(rest, internal, upstream)
    if cmd == 'inspect':
        adapter = os.path.join(internal, 'mat2_inspect.py')
        sys.argv = ['mat2_inspect.py'] + rest
        runpy.run_path(adapter, run_name='__main__')
        return 0
    if cmd == 'probe':
        return _probe()
    print('unknown subcommand: %s' % cmd)
    return 2


if __name__ == '__main__':
    multiprocessing.freeze_support()
    try:
        sys.exit(main())
    except SystemExit:
        raise
    except Exception as exc:
        print('runtime error: %s' % exc)
        sys.exit(1)
