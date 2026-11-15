# Models

`registry.json` lists every model the app may load. The app refuses a model unless:

1. its file's SHA-256 matches the registry, and
2. its license record (in `licenses/`) exists and has `"approved": true`.

Small models made in this repository live in `files/`. Third-party model weights are not stored in Git; they are fetched by a script and verified against the registry hash.

| Model | Purpose | Source | License |
| --- | --- | --- | --- |
| test-luma v1 | AI runtime self-test (computes luma) | `scripts/make_test_model.py` | This project's own |

Candidates for the portrait and background features are listed in `CANDIDATES.md`.
