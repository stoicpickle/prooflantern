# Writing a Proof Lantern project map

Proof Lantern maps the shortest experience your user must complete. It does not
map every file, subsystem, feature request, or future idea.

Start in your project folder:

```sh
proof-lantern init --guided .
```

Guided setup asks for your promise, 3–5 ordered user actions, and a visible check
for each. It shows a preview and asks you to `save`, `edit`, or `cancel`. Editing
revisits the questions with the current answers as defaults; Enter keeps each
answer. Enter at the final confirmation cancels. `:cancel` at any prompt, Ctrl-C,
or end of input also stops without saving.

Save creates only `.proof-lantern/project.yml`, with IDs `step-1`, `step-2`, etc.
Each step depends on the preceding step. Every step begins `UNKNOWN`: the
questions capture intent, not implementation or proof. No code is scanned or
executed. Full labels and checks are preserved, including Unicode and YAML
punctuation. Keep names/actions under 80 characters and promises/checks under
240 characters. You can edit the resulting YAML later to add branches or extras.

Guided setup requires interactive stdin and stdout and refuses an existing map
before asking questions. It also refuses orphan `manual-evidence.json` or
`observations.json` files, so a fresh map cannot accidentally inherit another
journey's evidence. Restore the original map or choose an empty project folder;
setup never deletes existing files.

For the original noninteractive, commented starter, use `proof-lantern init .`.
That creates three placeholders (`start`, `outcome`, `return`) to edit by hand.
Both modes refuse to overwrite an existing map.

## Human-owned intent

Here is a small project map without evidence:

```yaml
schema_version: 1
project:
  name: Recipe Box
  promise: Save a recipe and find it again tomorrow.
capabilities:
  - id: add
    label: Add a recipe
    map_label: Add
    role:
      kind: core
      order: 1
    proof_needed: Create a recipe and confirm it appears.

  - id: reopen
    label: Reopen saved recipes
    map_label: Reopen
    role:
      kind: core
      order: 2
    depends_on: [add]
    proof_needed: Close the app, reopen it, and confirm the recipe appears.
```

- `promise` is the result the project should deliver for a user.
- Each `core` capability is one required step in that journey.
- `map_label` is a short terminal label; omit it to use the full `label`.
- `depends_on` explains which earlier capabilities must work first.
- `proof_needed` is an observable check, not a coding task.

Use portable IDs matching `[a-z][a-z0-9_-]*`. Core order values must be
unique, but they do not need to be consecutive.

If an existing map used spaces, uppercase letters, or other characters in an
ID, rename that ID and update every reference to it. Check `pinned_keystone`,
`depends_on`, supporting `supports` values, and each `capability_id` in
`observations.json` or `manual-evidence.json`.

## Manual evidence

Evidence changes the displayed state. The easiest way to record a current
human observation is the command line:

```sh
proof-lantern record reopen passed --summary "I closed the app, reopened it, and the saved recipe appeared." .
```

The claim can be `built`, `missing`, `passed`, or `failed`. Proof Lantern stores
these explicit observations in `.proof-lantern/manual-evidence.json` without
rewriting the comments or formatting in `project.yml`. A newer record in the
same implementation or verification category makes the older command-recorded
fact `STALE`; it does not erase history.

You can also author a fact directly under a capability in `project.yml`:

```yaml
    manual_evidence:
      - claim: verification_passed
        freshness: current
        summary: I closed the app, reopened it, and the saved recipe appeared.
```

Supported claims are:

- `implementation_present` → `BUILT / UNPROVEN`
- `implementation_absent` → `MISSING`
- `verification_passed` → `PROVEN`
- `verification_failed` → `PROOF FAILED`

Use `freshness: stale` when a fact is historical and should remain visible but
must not establish the current state. If current facts contradict one another,
the capability becomes `CONFLICTING`. The `record` command will not silently
replace project-authored or machine evidence; if a new fact would leave a
conflict, it asks you to reconcile those current records first.

## Recording-time receipts

Every new `record` saves an RFC3339 timestamp and the exact `proof_needed` text.
Use repeated `--input FILE` arguments to additionally fingerprint selected files:

```sh
proof-lantern record reopen passed --summary "The recipe appeared after restarting." \
  --input src/storage.rs --input tests/reopen.rs .
```

Only name files you actually checked, relative to the map root. Paths must use
canonical `/` separators (no `./`, `..`, absolute paths, backslashes, colons, or
control characters, or components ending in a dot or space). Inputs must be
regular files, not symlinks or directories.
The `.git` and `.proof-lantern` directories are excluded, including aliases, to
avoid tracking the receipt's own output. A receipt supports up to 32 unique input
files, at most 16 MiB each. Inputs are deduplicated and sorted on recording.

The optional `receipt` field belongs to an evidence fact:

```json
"receipt": {
  "recorded_at": "2026-09-07T12:00:00Z",
  "proof_needed": "Close the app, reopen it, and confirm the recipe appears.",
  "inputs": [
    {
      "path": "check.txt",
      "sha256": "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
    }
  ]
}
```

That example digest is for the bytes `hello` without a newline, not a real test
result. Authored/imported receipts must have unique sorted paths and lowercase
64-character SHA-256 digests. Malformed receipts are rejected. Old files without
receipts remain accepted; older Proof Lantern binaries may reject the new field,
so use this version or newer to read receipt-bearing maps.

On each load, selected bytes and the required check are compared with the
receipt. Changed, deleted, unreadable, oversized, or newly unsafe inputs make
the fact effectively `STALE`, with `NEEDS RECHECK` explaining why. A mismatch
does not rewrite the original freshness, receipt, summary, or human intent.
Explicitly stale historical records do not require their files to remain.
To supersede a command-recorded result, perform the check again and record a new
observation. The previous receipt stays in history. Restoring the exact tracked
bytes and check text makes an unsuperseded receipt match again; it is a content
comparison, not an audit log of every intervening change.

Use `proof-lantern explain NODE .` for the full timestamp, tracked paths, hashes,
and recheck reason. The TUI provides a compact tracking label and the same
derived state. Press `r` in an open project map after edits, or reopen the TUI.
Refresh rereads the intent and evidence files, checks receipts, and reevaluates
the map without writing anything. It is not a live watcher or test runner.

Only one refresh runs at a time; repeated `r` presses while it is busy are
ignored. Navigation and quitting stay available. On success the selected ID and
inspector state are retained (a removed ID falls back to the current focus).
On a read, parse, or validation error, the last good map stays visible as an
`OUTDATED SNAPSHOT`; fix the indicated file and press `r` to retry. The error
notice is bounded for terminal safety; `proof-lantern next .` from the map root
prints the full diagnostic. Refresh does not update timestamps or freshness in
the evidence files themselves.

Receipts describe **recording-time context**, not a captured test execution.
They neither execute code nor authenticate who made the claim. A matching hash
does not prove a behavior works. A selected result file also does not bind all
the source code that produced it unless that code is selected separately.
Unselected files, environment changes, and changes before recording are outside
this detection boundary. Empty `inputs` track only the check text; legacy facts
without receipts have no automatic change detection. Imported test results keep
their source attribution and still require an inspectable evidence location.

## Machine observations

Replaceable tool output belongs in `.proof-lantern/observations.json`:

```json
{
  "schema_version": 1,
  "observations": [
    {
      "capability_id": "add",
      "source": "static_scan",
      "fact": {
        "claim": "implementation_present",
        "freshness": "current",
        "summary": "Recipe creation code is present.",
        "location": {
          "path": "src/add_recipe.rs",
          "line_start": 1,
          "line_end": 5
        }
      }
    }
  ]
}
```

Paths must be relative to the project root and remain inside it. If line numbers
are supplied, both `line_start` and `line_end` are required, use one-based line
numbers, and must point to readable lines that exist.

A `static_scan` may record only `implementation_present`. It cannot prove
runtime behavior or infer absence from silence. An `imported_test_result` may
record `verification_passed` or `verification_failed`.

## Supporting and optional capabilities

A supporting capability helps one core step without becoming part of the main
journey:

```yaml
  - id: local-database
    label: Local database
    role:
      kind: supporting
      supports: add
    proof_needed: Write and read one recipe record through the database adapter.
```

An optional capability stays visible without inflating core progress or becoming
the current focus:

```yaml
  - id: share
    label: Share a recipe
    role:
      kind: optional
    depends_on: [reopen]
    proof_needed: Export a saved recipe through the system share surface.
```

## Check the result

```sh
proof-lantern .
proof-lantern next .
proof-lantern explain reopen .
```

If the map does not load, Proof Lantern reports the invalid field, dependency,
evidence path, or line range rather than silently guessing.
