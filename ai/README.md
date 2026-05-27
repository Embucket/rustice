# AI docs

`code-index.md` is generated from the Rust workspace state. Do not edit it by hand.

The `contracts/` and `testing/` directories contain repo-internal guidance for agentic development. They are intentionally outside `docs/` so they are not part of the public Starlight docs site.

Refresh it from the repository root with:

```bash
python3 scripts/ai/build_code_index.py
```

Check whether it is current with:

```bash
python3 scripts/ai/build_code_index.py --check
```

The generator uses Python 3.11+ standard-library `tomllib` and does not require extra Python dependencies.
