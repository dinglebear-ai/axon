# Local source containment contract

This contract applies whenever Axon reads `SourceKind::Local`. It defines the
security boundary shared by inline source creation, detached creation, worker
execution, retry/recovery, discovery, and acquisition.

## Caller modes

| Mode | Admission | Filesystem boundary |
|---|---|---|
| Authenticated server caller | Requires `axon:local` and an absolute existing path beneath `AXON_SOURCE_LOCAL_ALLOWED_ROOTS` | Linux-only descriptor-relative containment |
| Loopback server caller without a snapshot | Requires a configured allowed root | Linux-only descriptor-relative containment |
| `AuthMode::TrustedLocal` CLI/system caller | Scope-compatible without the server allowlist | Explicit trusted-local root descriptor |

An empty server allowlist disables server local-source submission. A detached
server request is denied before its durable job row is created, and the
persisted request is checked again when a worker executes or recovers it.
Trusted-local mode is not a server-containment escape hatch: transports must
not mint `TrustedLocal` snapshots for remote callers.

## Configuration contract

`AXON_SOURCE_LOCAL_ALLOWED_ROOTS` is a comma-separated list of absolute,
existing directories. Matching is component-aware: an exact root and nested
paths match, while sibling prefixes, relative paths, `..`, missing roots, and
symlink components do not.

Configured roots and every ancestor are an operator trust boundary. They must
be root-owned, non-writable by the Axon service account, and mounted read-only
when used for generated projections. Do not allow `/`, `/proc`, a workspace,
or a general data mount.

## Descriptor model

On Linux, `LocalSourceAdapter::new_contained` opens the configured allowed-root
directory, resolves the requested source relative to that descriptor, and
retains the resulting directory descriptor for the whole discover/acquire
operation. Every file is opened relative to it with:

```text
RESOLVE_BENEATH
RESOLVE_NO_SYMLINKS
RESOLVE_NO_MAGICLINKS
RESOLVE_NO_XDEV
```

The adapter never turns a canonical pathname back into an uncontrolled read.
A directory rename or pathname replacement after discovery therefore cannot
redirect acquisition. Overlapping jobs retain distinct descriptors keyed by
`JobId`.

Contained server mode fails closed with
`adapter.local.containment_unsupported` on non-Linux systems. Trusted-local
mode remains explicit and separately implemented for cross-platform CLI use.

## Symlink compatibility

`follow_symlinks=true` is no longer supported for local sources in any caller
mode. The selected containment model is `NO_SYMLINKS`; accepting the option for
trusted callers would make local behavior mode-dependent and would reintroduce
unsafe recursive enumeration. Callers must supply the real directory or an
operator-controlled fixed read-only mount.

## Handle lifecycle and capacity

A root handle is retained only after discovery succeeds. Discovery errors and
cancellation drop their operation-owned descriptor without adding state.
Every acquisition batch clones the matching `JobId` handle, so a generation
with more than 64 changed items keeps the same descriptor boundary across all
batches. The shared pipeline calls the adapter's terminal `release` hook on
success and failure, including unchanged generations that never acquire a
batch. Successful discoveries that never reach a terminal pipeline outcome
are bounded to 64 retained handles per adapter; the next discovery fails with
`adapter.local.root_state_capacity`. Dropping the adapter releases all
remaining descriptors.

## Completeness map

| Axis | Contract or implementation |
|---|---|
| Behavioral specification | This document defines caller modes, admission timing, descriptor reads, compatibility, lifecycle, and stable failures. |
| Configuration and API contract | `Config::source_local_allowed_roots` is populated only from `AXON_SOURCE_LOCAL_ALLOWED_ROOTS`; MCP and REST apply it before detached enqueue, and worker execution rechecks the persisted caller snapshot. No request field or public transport schema changed. |
| Typed model | `Vec<PathBuf>` carries configured roots; `LocalRootHandle` owns the opened descriptor; `LocalSourceAdapter` keys retained handles by `JobId`; `SourceAdapter::release` is the terminal cleanup hook. |
| Persistence | No migration is required. Allowed roots are deployment configuration, descriptors are process-local capabilities that cannot be serialized, and durable jobs already persist the source request and authorization snapshot used for the execution-time recheck. |
| Fixtures and tests | Tests create real temporary directories, files, symlinks, replacements, and more-than-one-batch corpora dynamically so each run exercises the host kernel rather than a mocked fixture tree. |
| Developer and operator documentation | This reference is the developer contract; `docs/guides/configuration.md`, `docs/reference/runtime/security.md`, and `config.example.toml` define deployment and operations. |

## Stable errors

| Code | Meaning | Retry |
|---|---|---|
| `auth.scope_required` | Caller lacks `axon:local` | No, obtain correct authorization |
| `security.local_root_denied` | Server source is outside/invalid for the configured allowlist | No, fix source or configuration |
| `security.local_secret_denied` | Source path is secret-like | No |
| `adapter.local.root_unsafe` | Trusted-local source root is missing, wrong type, or symlinked | No |
| `adapter.local.item_key.escape` | Relative open was denied by containment | No |
| `adapter.local.symlinks_unsupported` | `follow_symlinks=true` was requested | No |
| `adapter.local.root_state_failed` | A contained acquisition has no matching retained operation handle | No; restart the source operation |
| `adapter.local.root_state_capacity` | 64 successful discoveries await terminal release | Retry only after outstanding operations finish |
| `adapter.local.containment_unsupported` | Server containment requested off Linux | No; run the server on Linux |

Public errors use stable text and filename-only `path_hint` values. They never
include an absolute requested path, configured allowed root, resolved target,
or operating-system error string.

## Executable coverage

- `source_security_tests`: exact/nested roots, empty allowlist, relative and
  parent paths, sibling-prefix ambiguity, symlink roots, stable redaction.
- `source::enqueue::tests`: deny-before-create and accepted detached roots.
- `local_tests`: item traversal, source/file/directory symlink swaps, forbidden
  followed symlinks, descriptor retention across pathname replacement,
  overlapping `JobId` isolation, a 130-file multi-batch generation, 128
  unchanged terminal-release cycles, repeated failed-discovery cleanup, binary
  and size policy, and error redaction.
- `source_runner_tests`: persisted caller snapshots are reused on execution and
  recovery rather than upgraded.
