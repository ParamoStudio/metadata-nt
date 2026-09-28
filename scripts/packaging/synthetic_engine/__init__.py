"""Synthetic metadata add-on engine (owner-approved scope extension).

Runs inside the frozen MAT2 runtime (bundled) or the dev venv. MAT2 remains
the only sanitisation engine; this package only writes optional decoy
metadata to MAT2-cleaned staged files, per addon-fauxmeta/SYNTHETIC_METADATA_HANDOFF.md.
"""
