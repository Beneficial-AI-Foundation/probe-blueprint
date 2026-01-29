# probe-blueprint

Probe Blueprint projects: generate call graph atoms and analyze verification results for Lean 4 with Blueprint.

## Installation

```bash
cargo install --path .
```

## Commands

```
probe-blueprint <COMMAND>

Commands:
  stubify   Extract Blueprint stubs from LaTeX files
  atomize   Generate call graph atoms with line numbers
  specify   Extract function specifications
  verify    Run Blueprint verification and analyze results
```

---

### `stubify` - Extract Blueprint Stubs from LaTeX

Extract mathematical stubs from a Blueprint project's LaTeX files. Parses `blueprint/src/*.tex` files for theorem-like environments (theorem, lemma, definition, etc.) and extracts their labels, Lean declarations, and dependencies.

```bash
probe-blueprint stubify <PROJECT_PATH> [OPTIONS]

Options:
  -o, --output <FILE>    Output file path (default: .verilib/stubs.json)
```

**Examples:**
```bash
probe-blueprint stubify ./my-lean-project
probe-blueprint stubify ./my-lean-project -o stubs.json
```

**How it works:**

1. Reads `blueprint/src/web.tex` to find the `thms` option (defaults to: definition, lemma, proposition, theorem, corollary)
2. Scans all `.tex` files in `blueprint/src/` for those environments
3. For each environment, extracts:
   - All `\label{...}` → `labels` list (uses the last one for stub-name)
   - `\lean{a,b,c}` → `code-name` (first), `lean-names` (full list if multiple)
   - `\leanok` → `spec-ok: true`
   - `\mathlibok` → `mathlib-ok: true`
   - `\notready` → `not-ready: true`
   - `\discussion{123}` → `discussion: ["123"]` (can appear multiple times)
   - `\uses{r,s,t}` → `spec-dependencies: ["r","s","t"]`
4. If a `\begin{proof}...\end{proof}` immediately follows, also extracts:
   - `\label{...}` → appended to `labels` list
   - `\leanok` → `proof-ok: true`
   - `\mathlibok` → `proof-mathlib-ok: true`
   - `\notready` → `proof-not-ready: true`
   - `\discussion{...}` → `proof-discussion`
   - `\uses{...}` → `proof-dependencies`
   - `\lean{...}` → `proof-lean-names`
5. If a proof contains `\proves{label}`, it is merged into the corresponding stub (for proofs not immediately following their statement)
6. If an environment has no label, generates one in the form `a0000000000`
7. Errors if duplicate labels are found
8. Extracts project config macros (`\home`, `\github`, `\dochome`) and writes them to `.verilib/config.json`

**Output format:**

```json
{
  "chapter/implications.tex/thm_proof_label": {
    "stub-type": "theorem",
    "stub-path": "chapter/implications.tex",
    "stub-spec": { "lines-start": 10, "lines-end": 15 },
    "stub-proof": { "lines-start": 17, "lines-end": 22 },
    "labels": ["thm_label", "thm_proof_label"],
    "code-name": "Subgraph.Equation387_implies_Equation43",
    "lean-names": ["Subgraph.Equation387_implies_Equation43", "Subgraph.Equation387_implies_Equation43'"],
    "spec-ok": true,
    "mathlib-ok": false,
    "not-ready": false,
    "discussion": ["123"],
    "spec-dependencies": ["eq387", "eq43"],
    "proof-ok": true,
    "proof-mathlib-ok": true,
    "proof-dependencies": ["lemma1", "lemma2"],
    "proof-lean-names": ["ProofDecl"]
  },
  "chapter/equations.tex/eq1": {
    "stub-type": "definition",
    "stub-path": "chapter/equations.tex",
    "stub-spec": { "lines-start": 5, "lines-end": 8 },
    "labels": ["eq1"],
    "code-name": "Equation1",
    "spec-ok": true,
    "mathlib-ok": true,
    "not-ready": false,
    "spec-dependencies": ["magma-def"]
  }
}
```

**Field descriptions:**

*Statement fields:*
- **Key (stub-name)**: Relative path from `blueprint/src` + `/` + last label
- **`stub-type`**: The LaTeX environment type (e.g., "theorem", "lemma", "definition", "dfn")
- **`stub-path`**: Relative path of the .tex file from `blueprint/src`
- **`stub-spec`**: Line range of the statement environment (`lines-start` and `lines-end`)
- **`labels`**: All labels found in the environment and its proof (in order of appearance)
- **`code-name`**: First Lean declaration name from `\lean{...}` (null if not specified)
- **`lean-names`**: Full list of Lean declarations if multiple specified (omitted if single or none)
- **`spec-ok`**: `true` if `\leanok` is present in the statement
- **`mathlib-ok`**: `true` if `\mathlibok` is present in the statement
- **`not-ready`**: `true` if `\notready` is present in the statement
- **`discussion`**: List of GitHub issue numbers from `\discussion{...}` (omitted if empty)
- **`spec-dependencies`**: List of labels from `\uses{...}` in the statement

*Proof fields (omitted if no proof):*
- **`stub-proof`**: Line range of the proof environment
- **`proof-ok`**: `true` if `\leanok` is present in the proof
- **`proof-mathlib-ok`**: `true` if `\mathlibok` is present in the proof
- **`proof-not-ready`**: `true` if `\notready` is present in the proof
- **`proof-discussion`**: List of issue numbers from `\discussion{...}` in the proof
- **`proof-dependencies`**: List of labels from `\uses{...}` in the proof
- **`proof-lean-names`**: List of Lean declarations from `\lean{...}` in the proof

**Config output (`.verilib/config.json`):**

If any of the project-level macros `\home`, `\github`, or `\dochome` are found in the LaTeX files, they are written to `.verilib/config.json`:

```json
{
  "home": "https://example.com/project",
  "github": "https://github.com/user/repo",
  "dochome": "https://docs.example.com/"
}
```

Fields are omitted if not found. If the config file already exists, new values are merged with existing ones.

---

### `atomize` - Generate Call Graph Atoms

Transform stubs into call graph atoms with dependency information. This command reads `stubs.json` and generates an `atoms.json` file compatible with probe-verus tooling.

```bash
probe-blueprint atomize <PROJECT_PATH> [OPTIONS]

Options:
  -o, --output <FILE>     Output file path (default: .verilib/atoms.json)
      --regenerate-stubs  Regenerate stubs.json even if it exists
```

**Examples:**
```bash
probe-blueprint atomize ./my-lean-project
probe-blueprint atomize ./my-lean-project --regenerate-stubs
probe-blueprint atomize ./my-lean-project -o atoms.json
```

**How it works:**

1. Checks if `.verilib/stubs.json` exists; if not, runs `stubify` to generate it
2. If `--regenerate-stubs` is specified, regenerates stubs even if they exist
3. Transforms each stub into an atom with:
   - **Key**: Same as the stub name (`path/label`)
   - **`display-name`**: The last label from the stub
   - **`dependencies`**: Concatenation of `spec-dependencies` and `proof-dependencies`
   - **`stub-path`**: Path to the LaTeX source file
   - **`stub-text`**: Line range from `stub-spec`

**Output format:**

```json
{
  "387_implies_43": {
    "display-name": "387_implies_43",
    "dependencies": ["eq387", "eq43", "lemma1"],
    "stub-path": "chapter/implications.tex",
    "stub-text": { "lines-start": 10, "lines-end": 15 }
  },
  "eq1": {
    "display-name": "eq1",
    "dependencies": ["magma-def"],
    "stub-path": "chapter/equations.tex",
    "stub-text": { "lines-start": 5, "lines-end": 8 }
  }
}
```

**Field descriptions:**

- **Key**: The label (last part of the stub name after `/`)
- **`display-name`**: The label used for display purposes
- **`dependencies`**: All dependencies (spec + proof) that this atom relies on
- **`stub-path`**: Relative path of the .tex file from `blueprint/src`
- **`stub-text`**: Line range of the specification (`lines-start` and `lines-end`)

---

### `specify` - Extract Function Specifications

Extract specification status from stubs. This command reads `stubs.json` and generates a `specs.json` file indicating which stubs have been formalized.

```bash
probe-blueprint specify <PROJECT_PATH> [OPTIONS]

Options:
  -o, --output <FILE>     Output file path (default: .verilib/specs.json)
      --regenerate-stubs  Regenerate stubs.json even if it exists
```

**Examples:**
```bash
probe-blueprint specify ./my-lean-project
probe-blueprint specify ./my-lean-project --regenerate-stubs
probe-blueprint specify ./my-lean-project -o specs.json
```

**How it works:**

1. Checks if `.verilib/stubs.json` exists; if not, runs `stubify` to generate it
2. If `--regenerate-stubs` is specified, regenerates stubs even if they exist
3. For each stub, extracts:
   - **`specified`**: `true` if `spec-ok` is `true` in the stub (i.e., `\leanok` was present)

**Output format:**

```json
{
  "387_implies_43": {
    "specified": true
  },
  "eq1": {
    "specified": false
  }
}
```

**Field descriptions:**

- **Key**: The label (last part of the stub name after `/`)
- **`specified`**: `true` if the stub has been formalized in Lean (`\leanok` present)

---

### `verify` - Run Blueprint Verification

Run Blueprint verification on a project and analyze results.

```bash
probe-blueprint verify [PROJECT_PATH] [OPTIONS]

Options:
  -o, --output <FILE>            Write JSON results to file (default: proofs.json)
  -a, --with-atoms [FILE]        Enrich results with code-names from atoms.json
```

---

## License

MIT
