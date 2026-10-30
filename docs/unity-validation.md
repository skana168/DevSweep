# Unity cleanup acceptance gate

Implementation is available; **this gate is still open**. Do not mark ticket 02 resolved or claim
field-validated Editor protection until the following table has real Editor evidence on both
macOS and Windows. Automated process-boundary inputs are simulations, not Editor validation.

## Evidence currently available (2026-10-30)

- Development host: macOS 27.2 (26B123), arm64; Rust target aarch64-apple-darwin.
- `/Applications/Unity/Hub/Editor` is absent. No real Editor version or normal/batch run was tested.
- No Windows machine or Windows Rust target is available; the PowerShell/CIM adapter has not run
  on Windows and no Windows compilation result is claimed.
- A macOS smoke test copies `/bin/sleep` to an executable named `Unity`, starts it without
  a window and observes Busy via the real process query. This is synthetic naming evidence only.
- Automated tests exercise idle/busy/unknown inputs at the process system boundary, through
  discovery → plan → execution; both public execution wrappers retain real process observation.
  No environment variable, CLI flag, public injected-state API or force option bypasses protection.

## Process query timeout and UI behavior

- Each native process query has a 3-second deadline. Timeout, process failure, nonempty stderr
  or malformed output returns Unknown; unfinished query children are killed and waited for.
  Both output pipes are drained concurrently while waiting, so large CIM output cannot fill a
  pipe and stall the query.
- Project draft plans run on a worker using the existing thread/channel pattern. Replacing or
  closing the draft discards its receiver; late results cannot overwrite a newer draft.
  Confirmation displays the completed preview without enumerating processes every frame.
- Confirming starts a fresh real plan on the execution worker; execution still independently
  checks each Unity Project. Preview results never authorize deletion by themselves. Cancel
  remains available while planning/executing; it does not interrupt an in-flight OS query,
  which is bounded by the deadline.
- Automated regressions use a synthetic hung process (including partial Editor output), check
  Unknown/deadline/child reaping, and verify nonblocking draft results and stale-result rejection.
  These tests do not replace real Editor or Windows acceptance evidence.

## Required real-machine record

Use disposable copies of a Unity 2022.3+ project (also repeat with a Unity 6 `6000.x` version),
with known sentinel files in Library, Temp, Assets, Packages and ProjectSettings. Record OS/build,
Editor full version/install location, commands, observed process names, plan/execute results and
surviving sentinels. Test Trash and Direct; restore the fixture between runs. Never use valuable
projects for acceptance. Capture command output without secrets.

| Scenario | macOS | Windows | Expected |
|---|---|---|---|
| All Editors closed | pending | pending | Library unchecked/dangerous, Temp checked; chosen caches clean |
| Ordinary Editor, no reliance on projectPath | pending | pending | Every Unity Project blocked; non-Unity continues |
| `-batchmode -nographics`, with and without explicit projectPath | pending | pending | Blocked even without a window |
| Two Editors, different Projects/versions | pending | pending | All Unity blocked until both exit |
| Hub only, including Hub helpers | pending | pending | Unity cleaning allowed |
| Editor launched after plan, before Execute | pending | pending | Execution fails closed, both caches preserved |
| Enumeration denied/failed | pending | pending | Unknown, no Unity deletion |
| Candidate name/identity unavailable or inconsistent | pending | pending | Unknown, no Unity deletion |
| Permission denied / cache disappears | pending | pending | Existing failure report, other Projects continue |
| Cache replaced by link/junction after plan | pending | pending | Reject linked target; linked destination preserved |

For batch mode, use the official command line arguments for the installed Editor and a long-running
Editor task so it is still running during the checks. Record the exact command/task and observed
process name; a short-lived `-quit` that exits before observation does not validate protection.
For the launch-between-checks scenario keep the confirmation dialog open, start the Editor, then
confirm. Do not infer that a completed preflight prevents later starts.

For reproducible enumeration/identity faults, use a disposable VM/account with policy-denied
process querying, or instrument the OS query in a locally built diagnostic test binary. Record the
fault and observable failure; do not ship a bypass or treat mocked tests as real-machine evidence.
If the OS does not permit reproducibly injecting a fault, leave that row pending with the reason.

## Adapter basis and limits

- macOS `ps -Aww -o ucomm=` includes processes without terminals; local `man ps` identifies
  `ucomm` as the dependable accounting name, unlike mutable command arguments.
  [Apple ps source](https://github.com/apple-oss-distributions/adv_cmds/blob/main/ps/print.c)
  distinguishes accounting names from argument-derived `comm`. Unity is busy; other Unity-prefixed
  names are unknown except Hub and its helpers. Actual Editor/Hub process naming still needs the
  table above. Renamed binaries and hostile process impersonation are not an authenticated identity
  guarantee; do not describe process preflight as a security or mutual-exclusion boundary.
- Windows uses local [Get-CimInstance](https://learn.microsoft.com/en-us/powershell/module/cimcmdlets/get-ciminstance)
  and [Win32_Process Name/ExecutablePath](https://learn.microsoft.com/en-us/windows/win32/cimwin32prov/win32-process).
  Missing candidate executable paths (including access restrictions), malformed output, query
  errors and inconsistent candidate names/paths are unknown. Hub helpers require a Hub path.
- [Unity Editor arguments](https://docs.unity3d.com/2022.3/Documentation/Manual/EditorCommandLineArguments.html)
  include batch mode; projectPath absence cannot demonstrate idleness. Lockfiles are not used.
- Preflight is not a cross-process lock. An Editor can start, or filesystem paths/markers can
  change, after the check. Trash does not release space until the user empties it.
