# MAT2 Option Parity Checklist

Generated from the **actual supplied** `mat2 --help` (upstream 0.15.0 @ `70c17d3`,
verbatim capture in `docs/UPSTREAM_SNAPSHOT.md` §3). Every option the supplied CLI
exposes is accounted for below in one of the plan's categories (Task 13):
processing option / diagnostic action / deprecated-non-operative / implicit-internal.

| Upstream option | UI location | Wrapper behavior | Tests |
|---|---|---|---|
| `files` (positional) | Files panel (pickers, drag-drop, folder enumeration) | Registry resolves opaque IDs → explicit regular-file paths; **always one file per invocation**, passed after `--`; directories are never passed (wrapper enumerates instead, symlink-safe) | `hostile_filenames_are_single_data_arguments`, `invocation_is_never_a_shell`, `integration_dash_filename_is_cleaned_as_data`, selection-registry suite |
| `-L, --lightweight` | Settings → Cleaning mode → "Lightweight" radio | `CleanOptions.lightweight` → `--lightweight` flag | `clean_argv_lightweight_verbose_policy_inplace`; QA protocol Test E (Task 18) |
| (normal cleaning) | Settings → Cleaning mode → "Maximum removal" radio (default) | No `-L` flag; MAT2 default behavior | full pipeline + run_job suites |
| `-s, --show` | Right panel inspection (row select), "Inspect only" button, and pre/post verification inside every job | Structured path: read-only libmat2 JSON adapter (`resources/mat2_inspect.py`) using the same API the CLI's `--show` uses; plain `--show` retained for logs/diagnostics. Show-mode exit code is never trusted (always 0 upstream) | `integration_adapter_matches_cli_show`, `integration_show_mode_exit_code_is_not_trustworthy`, model parse/diff suite |
| `-V, --verbose` | Advanced → "Verbose MAT2 output" checkbox | `--verbose` on clean invocations; sanitized MAT2 stdout/stderr lines streamed to Process details log | `clean_argv_lightweight_verbose_policy_inplace`; log sanitizer suite |
| `--unknown-members {abort,omit,keep}` | Advanced → "Unknown members in archives" dropdown (default Abort (recommended)) | Always passed explicitly (deterministic); selecting `keep` shows "Keeping unknown archive members may preserve metadata." | `clean_argv_lightweight_verbose_policy_inplace`; QA protocol (Task 18) |
| `--inplace` | Advanced → "Replace original files" (see destructive-mode rules below) | Dedicated in-place pipeline: no staging, no output root; source modified by MAT2 then **verified** (exists/regular/non-empty/post-inspected) before Processed/Warning | `run_job_inplace_modifies_source_and_leaves_no_output_dir`, `run_job_inplace_unsupported_leaves_source_untouched`, `run_job_inplace_cancel_before_start_leaves_source_untouched`, `clean_one_inplace_requires_inplace_option`, `normal_pipeline_refuses_inplace_option` |
| `--no-sandbox` | **Not exposed** (deprecated/non-operative) | Bubblewrap sandboxing was removed upstream in 0.14.0; the flag only emits a DeprecationWarning. Exposing a meaningless toggle is forbidden by INTERFACE §12. Documented here and in UPSTREAM_SNAPSHOT §4.3. | n/a (never constructed by the runner — no code path exists) |
| `-v, --version` | Advanced → "Show MAT2 version"; startup diagnostics (Task 15); top-bar version info | `Mat2Runner::version()`; output sanitized into Process details | `integration_diagnostics_and_clean_roundtrip` |
| `-l, --list` | Advanced → "List supported formats" | `Mat2Runner::list_formats()`; runtime output is the authority (never hard-coded format lists) | `integration_diagnostics_and_clean_roundtrip` |
| `--check-dependencies` | Advanced → "Check dependencies"; startup gate (Task 15) | `Mat2Runner::check_dependencies()`; fatal diagnostics disable PROCESS | `integration_diagnostics_and_clean_roundtrip` |
| `-h, --help` | Advanced → "Show MAT2 help" | `Mat2Runner::help()` into Process details | `integration_diagnostics_and_clean_roundtrip` |

No upstream option is unreviewed. Options the supplied version does **not** have
(and the wrapper therefore does not fake): `--check`, sandbox selection, any
spoofing/fake-metadata option.

## In-place (destructive) mode rules — implementation status

Per HANDOFF §9.5, INTERFACE §12/§22, plan Task 13:

| Rule | Implementation | Verification |
|---|---|---|
| Off by default on **every launch** | Frontend checkbox renders unchecked (no persistence anywhere — no localStorage, no settings file); backend arm flag `AppState.inplace_armed` is process-memory only, false at launch | Manual QA (Task 18): toggle on → restart app → observe OFF |
| Not persisted | Same as above; nothing writes UI state to disk | Code review + QA |
| Explicit warning on activation | Checking the box opens a modal: "Replace original files? / Normal mode preserves the original and produces a cleaned copy. In-place mode changes the selected original files." — Cancel reverts the checkbox; amber warning text appears under the toggle | Visual QA |
| Second confirmation on job start | Modal: "Confirm destructive processing of N original files / This changes the selected originals. No wrapper-created backup is guaranteed." with [Cancel] [Replace originals] | Visual QA |
| Disables normal output routing | Output fieldset disabled while armed; backend ignores output mode and creates **no** staging workspace and **no** output directory (`run_job` in-place branch) | `run_job_inplace_modifies_source_and_leaves_no_output_dir` |
| Verifies the modified source afterward | `clean_one_inplace`: exists + regular + non-empty + structured post-inspection; failure ⇒ Failed/OutputInvalid, never green | pipeline tests |
| Stale frontend state cannot silently re-enable | `start_clean_job(settings.inplace=true)` is **rejected** unless `set_inplace_armed(true)` was called in the same process session; the arm is consumed (atomic swap) by the first destructive job — one confirmation per job; after job finish the UI resets the toggle to OFF | Backend gate (`lib.rs`); Manual QA (Task 18): arm → restart → attempt job with crafted IPC → expect rejection |
