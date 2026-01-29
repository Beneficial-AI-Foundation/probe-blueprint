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
   - `\lean{abc}` → `code-name`
   - `\leanok` → `spec-ok: true`
   - `\uses{r,s,t}` → `spec-dependencies: ["r","s","t"]`
4. If a `\begin{proof}...\end{proof}` immediately follows, also extracts:
   - `\label{...}` → appended to `labels` list
   - `\leanok` → `proof-ok: true`
   - `\uses{...}` → `proof-dependencies`
5. If an environment has no label, generates one in the form `a0000000000`
6. Errors if duplicate labels are found

**Output format:**

```json
{
  "chapter/implications.tex/thm_proof_label": {
    "labels": ["thm_label", "thm_proof_label"],
    "code-name": "Subgraph.Equation387_implies_Equation43",
    "spec-ok": true,
    "spec-dependencies": ["eq387", "eq43"],
    "proof-ok": true,
    "proof-dependencies": ["lemma1", "lemma2"]
  },
  "chapter/equations.tex/eq1": {
    "labels": ["eq1"],
    "code-name": "Equation1",
    "spec-ok": true,
    "spec-dependencies": ["magma-def"]
  }
}
```

**Field descriptions:**
- **Key (stub-name)**: Relative path from `blueprint/src` + `/` + last label
- **`labels`**: All labels found in the environment and its proof (in order of appearance)
- **`code-name`**: Lean declaration name from `\lean{...}` (null if not specified)
- **`spec-ok`**: `true` if `\leanok` is present in the statement, `false` otherwise
- **`spec-dependencies`**: List of labels from `\uses{...}` in the statement (empty list if not specified)
- **`proof-ok`**: `true` if `\leanok` is present in the proof (omitted if no proof or no `\leanok`)
- **`proof-dependencies`**: List of labels from `\uses{...}` in the proof (omitted if no proof or no `\uses`)

---

### `atomize` - Generate Call Graph Data

Generate call graph atoms with line numbers.

```bash
probe-blueprint atomize <PROJECT_PATH> [OPTIONS]

Options:
  -o, --output <FILE>     Output file path (default: atoms.json)
```

---

### `specify` - Extract Function Specifications

Extract function specifications from source files.

```bash
probe-blueprint specify <PATH> [OPTIONS]

Options:
  -o, --output <FILE>        Output file path (default: specs.json)
  -a, --with-atoms <FILE>    Path to atoms.json for code-name lookup
```

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
