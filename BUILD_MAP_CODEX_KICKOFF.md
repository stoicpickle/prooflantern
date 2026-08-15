# Codex Kickoff: Evidence-Backed Project Build Map

## Working instruction

Treat **Build Map** as a working product name only. Do not spend time naming or branding it yet.

Act as a senior Rust/TUI product engineer, information-visualization designer, and skeptical systems architect. Your first job is to inspect the existing repository and produce a concrete implementation plan. Do **not** begin a broad rewrite or build the entire product in the first pass.

## The product in one sentence

Build Map is a local-first terminal tool that shows a beginner what a project has **decided**, **implemented**, **proven**, **not built**, or **not yet understood** across its core user journey, then identifies the single most consequential gap preventing the project’s promise from being true.

## The product question

The tool should answer:

> Given the promise of this project, what must a user be able to do, what evidence says those capabilities exist or work, and which unresolved capability currently breaks the journey?

This is not primarily a repository architecture diagram. It is not a file graph, task manager, sprint board, issue tracker, or AI-generated progress percentage.

## Important shift from the current prototype

The existing Rust/Ratatui prototype is valuable foundation. It already includes terminal rendering, navigation, responsive layout, restrained themes, reduced-motion support, terminal restoration, static import scanning for JavaScript/TypeScript/Python, path-and-line evidence, unresolved nodes, and deterministic graph traversal.

Preserve the useful terminal engineering and evidence-linking work. However, do not assume that a static dependency graph is the product model.

The prior prototype answers:

> What code appears connected to what?

Build Map must answer:

> What does the user need to be able to do, and how certain are we that each part actually works?

A repository graph may later contribute evidence, but it must not define the core journey by itself.

## Target user

The initial user is a solo builder or early learner who can make software with AI assistance but struggles to answer:

- What have I actually finished?
- What did I merely decide?
- What code exists but has never been verified?
- What is still missing from the main experience?
- What should I prove or build next?

The interface should help the user form a trustworthy mental model without requiring them to understand architecture diagrams, graph theory, or formal project-management language.

## Core example

Use this as the first fixture and vertical-slice example:

```text
Project: Recipe Box
Promise: “Save a recipe and find it again tomorrow.”

✓ ADD ━━━━━ ◐ SAVE ━━━━━ ╳ REOPEN ┄┄┄◇ FIND
proven      built          missing       decided
                │
                └──── ✓ LOCAL DATABASE

KEYSTONE GAP
Reopen saved recipes
The core journey stops here.

Proof needed: close app → reopen → recipe appears
```

The exact appearance may evolve, but the meaning must remain clear.

## Product invariants

1. **Map the user journey, not the file tree.**
2. **Code existing is never treated as proof that behavior works.**
3. **Unknown stays visible.** The system must not convert uncertainty into a confident claim.
4. **The human owns the promise and core journey.** Machine proposals require explicit acceptance.
5. **Every machine-generated claim must point to evidence.** Prefer file path plus line range, imported test result, recorded manual check, or another inspectable artifact.
6. **A test file existing is not the same as a test having passed.**
7. **Human decisions and annotations must not be silently overwritten during refresh.**
8. **Core capabilities and optional extras must be visually separated.** Optional work must not inflate perceived progress.
9. **Missing core capabilities should physically interrupt the path.**
10. **Do not show a completion percentage.** The product should explain state, not manufacture precision.
11. **Local-first and non-executing by default.** No account, cloud upload, telemetry requirement, or automatic execution of project code.
12. **The keystone gap must be deterministic and explainable.** No unexplained AI ranking.

## Domain model direction

Do not make a single status enum the only source of truth. “Decided,” “implemented,” and “proven” are different dimensions and can overlap.

Propose a small model along these lines:

```yaml
version: 1
project:
  name: Recipe Box
  promise: Save a recipe and find it again tomorrow.

nodes:
  - id: add
    label: Add a recipe
    importance: core
    order: 1
    decision: confirmed
    implementation: present
    verification: proven
    depends_on: []
    proof_needed: null
    evidence:
      - kind: test_result
        path: artifacts/test-results.json
        detail: recipe creation flow passed

  - id: save
    label: Save recipe locally
    importance: core
    order: 2
    decision: confirmed
    implementation: present
    verification: unproven
    depends_on: [add]
    proof_needed: Save a recipe, close the app, and confirm the record persists.
    evidence:
      - kind: source
        path: src/storage.rs
        line_start: 18
        line_end: 61
        detail: local persistence implementation exists

  - id: reopen
    label: Reopen saved recipes
    importance: core
    order: 3
    decision: confirmed
    implementation: absent
    verification: unknown
    depends_on: [save]
    proof_needed: Close app → reopen → saved recipe appears.
    evidence: []
```

The exact schema is yours to recommend after inspecting the repository. Preserve these separations:

- **Human intent:** project promise, accepted journey, node importance, manual notes, overrides.
- **Machine observations:** files, symbols, imports, test artifacts, and other refreshable evidence.
- **Derived presentation:** the glyph and label shown in the map.

A possible display derivation is:

- `✓ Proven`: implementation is present and verification is proven.
- `◐ Built, unproven`: implementation is present but proof is absent, stale, or failing.
- `◇ Decided`: the capability is confirmed as part of the journey, but implementation state has not yet been established.
- `╳ Missing`: evidence or explicit human confirmation establishes that a required implementation is absent.
- `? Unknown`: there is not enough evidence to make a responsible claim.

Codex should challenge or refine this model where necessary, but must preserve the conceptual distinction.

## Keystone gap

The keystone gap is not simply “the next task.” It is the unresolved core capability with the strongest blocking effect on the project promise.

For the first deterministic heuristic, consider:

1. Only accepted `core` nodes are eligible.
2. A human-pinned keystone gap wins.
3. Otherwise prioritize explicit missing or failing capabilities over unknown capabilities, and unknown capabilities over merely unproven capabilities.
4. Within the same severity, prefer the node that blocks the largest number of downstream core nodes.
5. Break remaining ties by core-journey order.
6. Always render the reason the node was selected.

Do not expose a fake numerical confidence or priority score in the interface.

## Intended interaction

The smallest coherent flow is:

1. The user states one plain-language project promise.
2. A journey of roughly five to nine core capabilities is proposed.
3. The user accepts, edits, reorders, or rejects the proposal.
4. Build Map reads repository evidence without executing the project.
5. The TUI renders the accepted journey and supporting capabilities.
6. Selecting a node shows only:
   - why it has its current state,
   - the evidence supporting that state,
   - what evidence would advance it.
7. The interface identifies one keystone gap and explains why it blocks the promise.
8. Refresh updates machine evidence while preserving human-owned decisions.

## Visual grammar

Reuse the established visual direction from the current terminal prototype:

- near-black background;
- restrained amber, green, cyan, and red phosphor-like accents;
- thin rules and dense monospace hierarchy;
- deliberate selected-node emphasis;
- purposeful, subtle motion only;
- reduced-motion and motion-off modes;
- no rainbow graph, gratuitous glitching, or decorative animation that obscures meaning.

For the MVP, prefer a readable ordered journey over a general free-form graph:

- Core journey runs left to right when space permits.
- Supporting capabilities branch beneath the relevant core node.
- Optional or future ideas sit faintly outside the core path.
- A missing capability creates a visible break rather than a normal connector.
- A decided but unbuilt capability uses a lighter or dashed connector.
- Unknown must look unresolved, not nearly complete.
- The inspector should show `Why`, `Evidence`, and `Proof needed`.
- Compact terminals may use stacked panes or drawers rather than compressing the map into confetti.

## Provisional command surface

Do not implement every command immediately. Use this to shape the architecture:

```text
buildmap                 # open the visual map
buildmap init            # establish project promise and seed a map file
buildmap refresh         # rescan machine-readable evidence
buildmap next            # print the keystone gap and why it was selected
buildmap explain <node>  # show status, evidence, and proof needed
```

The repository-local source of truth should remain small and reviewable, such as `.buildmap.yml` or `.buildmap/project.yml`. Generated scan data may be stored separately so it cannot overwrite human-authored intent.

## Architecture direction

Recommend the smallest architecture that cleanly separates:

1. **Model and persistence**
   - project promise;
   - accepted journey nodes and dependencies;
   - human annotations;
   - machine observations;
   - derived display state.

2. **Evidence adapters**
   - existing static repository scanner;
   - documentation and decision records;
   - imported test-result artifacts;
   - explicit manual verification records.

3. **Deterministic reasoning**
   - display-state derivation;
   - stale evidence handling;
   - keystone-gap selection;
   - explanations.

4. **Presentation**
   - TUI layout;
   - navigation and inspector;
   - responsive behavior;
   - motion settings;
   - plain CLI output for `next` and `explain`.

5. **Optional conversational adapter later**
   - A Codex/ChatGPT skill may propose a journey and explain evidence.
   - It must not become the source of truth.
   - Its proposals must be reviewable and explicitly accepted before modifying human-owned project intent.

Prefer refactoring and reusing the existing Rust/Ratatui foundation over rewriting it. Preserve terminal restoration and cross-platform safety work.

## Bounded implementation sequence

### Phase 0: Repository audit and product-model plan

Do this first. Do not broadly edit code yet.

- Inspect the current files, tests, architecture, and dependency choices.
- Identify what can be reused unchanged, what should be adapted, and what belongs only to the old repository-graph mode.
- Decide whether Build Map should be the primary mode, a separate mode within the same binary, or a sibling binary/library. Recommend one and explain the tradeoff.
- Propose the domain model and persistence boundary.
- Propose exact files/modules to add, change, move, or retire.
- Identify any current claims the software cannot honestly support.

### Phase 1: Fixture-first vertical slice

After the plan is accepted, build one excellent path using the Recipe Box fixture.

Required slice:

- load a hand-authored local Build Map file;
- derive display states rather than storing only a presentation enum;
- render the core journey and one supporting branch;
- visibly break the path at a missing core node;
- navigate nodes;
- show `Why`, `Evidence`, and `Proof needed` in the inspector;
- deterministically identify and explain the keystone gap;
- support reduced/off motion;
- preserve terminal restoration behavior.

Do not add automatic journey generation, a broad scanner rewrite, cloud services, accounts, or runtime instrumentation in this phase.

### Phase 2: Evidence refresh without silent authority

- Adapt the existing scanner into an evidence provider rather than the product model.
- Keep static-code claims explicitly labeled as static observations.
- Preserve human decisions during refresh.
- Treat test source as implementation evidence, not passing proof.
- Add import support for at least one recorded proof artifact or explicit manual verification record.
- Show stale or conflicting evidence honestly.

### Phase 3: Beginner workflow

- Add initialization and edit/review flow for the promise and core journey.
- Make proposed changes preview-first.
- Keep the accepted journey small and legible.
- Add helpful empty, unknown, and no-evidence states.

### Phase 4: Optional Codex skill

Only after the local model and visual grammar are trustworthy:

- propose a five-to-nine-node journey from the promise and repository;
- cite evidence for every proposed classification;
- write only after explicit user acceptance;
- use the same core model and deterministic rules as the CLI/TUI.

## Explicit non-goals for the initial release

Do not build:

- project-management accounts, teams, tickets, sprints, assignments, or notifications;
- a hosted dashboard;
- repository upload;
- a browser UI;
- automatic execution of untrusted project code;
- live runtime instrumentation;
- an IDE extension;
- broad language support;
- a free-form architecture graph as the primary screen;
- AI-generated completion percentages;
- autonomous rewriting of the project plan;
- support for every possible evidence source.

## First-pass acceptance criteria

The fixture-first vertical slice is successful when:

- A beginner can state the project promise after looking at the screen for a few seconds.
- They can tell which nodes are proven, merely built, decided, missing, and unknown without reading documentation.
- The missing node visibly interrupts the core journey.
- Selecting a node reveals the evidence and the exact proof still needed.
- The same input always produces the same keystone gap and explanation.
- Refreshable machine observations are structurally separated from human-authored intent.
- No static scan is described as proof of runtime behavior.
- The TUI remains usable at common compact and wide terminal sizes.
- Motion can be reduced or disabled.
- Terminal state is restored after normal exit, error, and panic paths.
- Formatting, linting, unit tests, and existing terminal-safety tests pass.

## What I want from your first response

Do not start broad implementation yet. Return a concise but concrete planning report with these sections:

1. **Product interpretation**: the five most important truths you believe the tool must preserve.
2. **Repository audit**: reusable foundations, mismatches, and technical debt relevant to this pivot.
3. **Recommended architecture**: including whether to use one mode, multiple modes, or a separate binary.
4. **Proposed domain model**: human intent, machine observations, derived states, evidence, and keystone-gap logic.
5. **Exact implementation plan**: ordered steps with the files/modules you would touch.
6. **Testing plan**: model tests, gap-selection tests, rendering/snapshot tests, responsive-layout tests, and terminal-restoration tests.
7. **Risks and open decisions**: only issues that materially affect the first vertical slice.
8. **Scope check**: explicitly list anything you are declining to build in the first slice.

Be candid. Challenge the concept where its evidence model, terminology, or interaction could mislead a beginner. Prefer a small trustworthy vertical slice over a broad impressive demo.
