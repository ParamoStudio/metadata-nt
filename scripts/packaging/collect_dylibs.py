#!/usr/bin/env python3
"""BFS dylib closure collector for the frozen MAT2 runtime (Task 19).

Copies /opt/homebrew libraries referenced by the GObject typelibs and helper
binaries into a flat staging dir, rewrites every /opt/homebrew install-name to
@loader_path/<leaf> (so the flat dir resolves at runtime without Homebrew),
and ad-hoc re-signs each rewritten binary (required on arm64 macOS after
install_name_tool invalidates the original signature).

Usage: collect_dylibs.py <out_dir> <root_binary_or_lib>...
"""
import os
import shutil
import subprocess
import sys


def deps_of(path):
    out = subprocess.run(["otool", "-L", path], capture_output=True, text=True).stdout
    deps = []
    for line in out.splitlines()[1:]:
        dep = line.strip().split(" ")[0]
        if dep:
            deps.append(dep)
    return deps


def resolve_dep(dep, origin_lib):
    """Map an install-name reference to a real file we can bundle, or None."""
    if dep.startswith("/opt/homebrew/"):
        return dep if os.path.isfile(dep) else None
    leaf = dep.rsplit("/", 1)[-1]
    if dep.startswith(("@rpath/", "@loader_path/", "@executable_path/")):
        for d in ("/opt/homebrew/lib", os.path.dirname(origin_lib)):
            cand = os.path.join(d, leaf)
            if os.path.isfile(cand):
                return cand
    return None


def main(argv):
    if len(argv) < 3:
        print("usage: collect_dylibs.py <out_dir> <root...>")
        return 2
    out_dir = argv[1]
    os.makedirs(out_dir, exist_ok=True)
    seen = set()
    queue = list(argv[2:])
    missing = []
    while queue:
        lib = queue.pop(0)
        base = os.path.basename(lib)
        if base in seen:
            continue
        seen.add(base)
        if not os.path.isfile(lib):
            real = shutil.which(base)
            if real and os.path.isfile(real):
                lib = real
            else:
                missing.append(lib)
                continue
        dest = os.path.join(out_dir, base)
        shutil.copy2(lib, dest)
        for dep in deps_of(lib):
            if dep == os.path.join("/usr/lib", base):
                continue
            resolved = resolve_dep(dep, lib)
            if resolved is None:
                continue
            dep_leaf = os.path.basename(resolved)
            subprocess.run(
                ["install_name_tool", "-change", dep,
                 "@loader_path/" + dep_leaf, dest],
                capture_output=True)
            queue.append(resolved)
        subprocess.run(["codesign", "--force", "-s", "-", dest], capture_output=True)
    print("collected %d libraries into %s" % (len(seen) - len(missing), out_dir))
    if missing:
        print("MISSING (skipped): %s" % ", ".join(missing))
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
