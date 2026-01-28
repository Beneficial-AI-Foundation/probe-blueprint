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
  stubify   Convert .md files with YAML frontmatter to JSON
  atomize   Generate call graph atoms with line numbers
  specify   Extract function specifications
  verify    Run Blueprint verification and analyze results
```

---

### `stubify` - Convert Stub Files to JSON

Convert a directory hierarchy of `.md` files with YAML frontmatter to a JSON file.

```bash
probe-blueprint stubify <PATH> [OPTIONS]

Options:
  -o, --output <FILE>    Output file path (default: stubs.json)
```

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
