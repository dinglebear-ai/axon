# Repository-reference maintenance

Keep this directory an index of Axon-specific tooling and conventions, not a copy of personal/global agent instructions.

## Read the related pages

[Repository layout](repo.md) · [recipes](recipes.md) · [scripts](scripts.md) · [coding rules](rules.md) · [memory/tracking background](memory.md)

[Contributing](../contributing.md) · [testing](../testing.md) · [release checklist](../release-checklist.md) · [documentation maintenance](../documentation.md)

## Verify references against execution

Check recipes against [Justfile](../../../Justfile), hooks against [lefthook.yml](../../../lefthook.yml), and tooling against [scripts](../../../scripts/) and [xtask](../../../xtask/). Distinguish a command's documented capability from evidence that it actually ran.

Do not prescribe full builds for documentation or assume every workstation uses the same task tracker, compiler cache, deployment host, or global Cargo settings. Keep task-specific host observations in the local override.

When a recipe or path moves, update its links and workflow-shape tests together. Preserve the difference between provider infrastructure, development binaries, and supported production deployment. Follow [deployment](../../operations/deployment.md) instead of duplicating service lifecycle instructions here.

Track shipped source changes separately from development-only tooling for release decisions. Links to live code and generated contracts must use current paths, not retired uppercase filenames or old docs/contributing locations.
