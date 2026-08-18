# Writing a Proof Lantern project map

Proof Lantern maps the shortest experience your user must complete. It does not
map every file, subsystem, feature request, or future idea.

Start in your project folder:

```sh
proof-lantern init .
```

The command creates `.proof-lantern/project.yml` with comments and three
placeholder core capabilities. It does not create machine evidence and never
overwrites an existing map.

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

Use stable lowercase IDs with hyphens or underscores. Core order values must be
unique, but they do not need to be consecutive.

## Manual evidence

Evidence changes the displayed state. The easiest way to record a current
human observation is the command line:

```sh
proof-lantern record reopen passed \
  --summary "I closed the app, reopened it, and the saved recipe appeared." .
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
