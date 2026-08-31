---
title: E2E release qualification manifest
created: 2026-08-30
updated: 2026-08-30
---

# E2E release qualification manifest

`scripts/e2e/build-qualification-manifest.py` is a deterministic projection over
saved E2E evidence. It does not run tests, contact providers, or turn absent
evidence into a pass. The evidence index binds every input to its SHA-256,
byte length, tested commit, producing workflow run and attempt, completion time,
retention location, and redaction class. The catalog and corpus are copied into
the evidence bundle and content-bound as subject inputs.

Profiles are versioned in `config/e2e/qualification-policy.json`. Each evidence
family is `required`, `optional`, or `not_applicable`. A required missing,
unavailable, stale, mismatched, or unverified artifact makes qualification
incomplete or fails closed. `not_applicable` requires both policy authorization
and a bounded rationale. Tested-binary live evidence and deployed-compatibility
evidence are separate families.

The output set is a canonical JSON manifest, a human Markdown projection, and a
checksum file. It intentionally contains no credentials, private hosts or IPs,
raw source content, database snapshots, or unnecessary tailnet metadata. It is
bounded and ready to be signed, but is explicitly **unsigned and not release
eligible**. A future signing job must use an approved workload identity with
minimal permissions, consume only this checksum-bound output, and remain
separate from live-provider credentials. Signing is deferred until the unsigned
format and policy are stable.

Example:

```sh
python3 scripts/e2e/build-qualification-manifest.py \
  --index evidence/qualification-index.json --evidence-root evidence \
  --out artifacts/qualification.json --summary artifacts/qualification.md \
  --checksums artifacts/SHA256SUMS
```

CI consumers call `.github/workflows/e2e-qualification.yml` after producing a
single `e2e-qualification-input` artifact containing
`qualification-index.json` and its checksum-bound evidence tree. The caller
supplies the successful evidence workflow run ID, full tested commit SHA, and
policy profile. The qualification job checks out that exact SHA, downloads only
the named artifact from that run, verifies that the index names the same SHA and
profile, and uploads the JSON, Markdown, and `SHA256SUMS` projections. It has
read-only repository and Actions permissions and deliberately performs neither
signing nor release mutation.
