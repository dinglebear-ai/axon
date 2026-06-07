# Indexed General Knowledge Question Set

Created: 2026-06-07

These questions were drafted from live `axon query` and `axon retrieve` passes against the current Qdrant collection. They intentionally avoid Axon-specific project questions and instead target general technical concepts that are indexed locally.

## Questions

1. In Bun's package manager, what is the security reason installed dependencies do not run arbitrary lifecycle scripts by default, and what must a project do to allow a specific dependency's lifecycle scripts to run?

2. PostgreSQL's heap-only tuple optimization applies only under certain update conditions. What are those conditions, and what two overheads does HOT reduce when the conditions are met?

3. Tailscale subnet routers and exit nodes both route traffic, but they solve different network problems. Explain the practical difference between them, including what SNAT changes about traffic from devices behind a subnet router.

4. Qdrant's sparse-vector documentation contrasts sparse and dense vectors for text search. Why are sparse vectors useful for rare or specialized terms, and where do they fall short compared with dense vectors?

5. OpenZFS special allocation classes can store metadata and optionally small blocks. What has to exist in a pool before special-class devices can be added, how is small-block placement enabled, and why should the special device's redundancy match the pool?

6. In MCP sampling, why does the server send a `sampling/createMessage` request through the client instead of directly calling an LLM provider, and how do model hints and priority values affect model selection without guaranteeing an exact model?

7. vLLM's automatic prefix caching hashes KV-cache blocks using more than just the current block's tokens. What components go into the block hash, why are only full blocks cached, and what changed around hash collision risk as of v0.11?

8. Combine PostgreSQL's HOT update behavior with the visibility-map explanation for index-only scans. How do MVCC row versions create maintenance pressure, how does HOT reduce part of that pressure, and why does the visibility map let some index-only scans avoid heap fetches?

9. OpenZFS dRAID uses distributed hot spares and fixed stripe width. Explain how that design helps resilvering, why the fixed stripe width affects usable capacity, IOPS, and compression, and why small-block-heavy dRAID pools may need a mirrored special vdev.

10. Compare four different safety or trust-boundary mechanisms from the indexed docs: Bun `trustedDependencies`, MCP sampling's human-in-the-loop client control, vLLM prefix-cache salting, and Tailscale subnet-router access control. For each mechanism, identify what risk it is trying to limit, who or what makes the trust decision, and one operational tradeoff it introduces.

## Answer Key

Use these answers as grading references. A strong answer does not need identical wording, but it should preserve the specific mechanisms and constraints.

### Q01

Bun does not run arbitrary lifecycle scripts from installed dependencies by default because dependency scripts such as `postinstall` can execute arbitrary code on the user's machine during install. Bun still runs the root project's own `{pre|post}install` and `{pre|post}prepare` scripts, but it blocks lifecycle scripts belonging to installed dependencies unless the project explicitly trusts those packages.

To allow one dependency's lifecycle scripts, the package must be listed in `trustedDependencies` in `package.json`, then the dependency should be reinstalled so Bun reads the updated trust list. A complete answer should distinguish project scripts from dependency scripts and mention that trusted packages are an explicit opt-in.

### Q02

PostgreSQL HOT updates are possible when the update does not modify any columns referenced by the table's indexes, excluding summarizing indexes, and when there is enough free space on the same heap page that contains the old row to store the updated row version.

When those conditions hold, HOT reduces update overhead in two ways. First, PostgreSQL does not need new index entries for the updated row, though summarizing indexes may still need updates. Second, old updated row versions can be removed during normal operation, including `SELECT`s, instead of waiting for periodic vacuum work. A strong answer should also mention that lowering `fillfactor` can increase the chance of page-local free space and that HOT/non-HOT update counts can be monitored in `pg_stat_all_tables`.

### Q03

A Tailscale subnet router is a tailnet device that advertises routes to one or more private subnets so tailnet clients can reach devices that cannot or do not run the Tailscale client, such as printers, cameras, cloud VPC resources, or legacy network segments. It is for access to specific private networks.

An exit node routes outbound internet traffic from tailnet devices, making that internet-bound traffic appear to come from the exit node's location. It is closer to a VPN egress path for internet traffic, not a bridge into a particular private subnet.

By default, subnet routers use SNAT. With SNAT enabled, traffic from a device behind the subnet router appears to originate from the router, not from the original behind-router device. Disabling SNAT preserves the original source IP when that matters for routing, logging, or policy.

### Q04

Sparse vectors are useful in text retrieval because they represent only selected token dimensions with nonzero weights, making keyword-heavy or term-specific matching efficient and interpretable. They are especially valuable when rare, specialized, or domain-specific terms matter because they can preserve exact or near-exact token evidence that a general dense embedding might blur.

Dense vectors represent meaning-rich semantic relationships across all dimensions and are better at capturing nuanced relationships between words or concepts, such as semantic similarity that does not share exact terms. Sparse vectors fall short there: they are not as good at capturing subtle semantic relationships like analogies or related concepts without lexical overlap.

The best answer should frame hybrid search as using both: sparse vectors help guarantee specific keyword or specialized-term recall, while dense vectors cover semantically similar results.

### Q05

OpenZFS special allocation classes are dedicated to specific block types. By default, the special class includes metadata, indirect blocks of user data, the intent log when no separate log device exists, and deduplication tables. It can also be configured to accept small file blocks or zvol blocks.

A pool must already have at least one normal, non-dedup and non-special vdev before devices can be assigned to the special class. Small file or zvol placement in the special class is opt-in per dataset; for small files, the dataset controls the eligible block size by setting `special_small_blocks` to a nonzero value.

The special device's redundancy should match the redundancy of the normal pool devices because special vdevs can hold critical metadata and other allocation classes. Losing an under-protected special vdev can put the pool at risk even if the normal data vdevs are redundant. A good answer can also mention that if the special class fills, allocations intended for it spill back into the normal class.

### Q06

In MCP sampling, the server asks the client to perform LLM sampling with `sampling/createMessage` so the client remains in control of model access, model selection, permissions, user review, and provider credentials. The server can request generation without needing its own LLM API keys.

The protocol expects clients that support sampling to advertise the `sampling` capability. For trust and safety, applications should keep a human in the loop: users should be able to review and edit prompts, deny sampling requests, and review generated responses before delivery.

Model selection is advisory rather than exact. Servers can send model hints, which are substring-like preferences for model names or families, and normalized priorities such as `costPriority`, `speedPriority`, and `intelligencePriority`. The client may map those hints to an equivalent model from another provider, and the client makes the final selection from models it can actually use.

### Q07

vLLM automatic prefix caching stores and reuses KV-cache blocks for previously processed prefixes so later requests with the same prefix can skip redundant prompt computation. Its block hash is built from a tuple of components: the parent block hash, the exact tokens in the current block, and extra hashes needed to make the block unique, such as LoRA IDs, multimodal input hashes, and cache salts for multi-tenant isolation.

vLLM only caches full blocks. This matters because the cache key and reuse semantics are block-based; partial blocks are not considered stable cache units in this design.

As of v0.11, the default hashing algorithm is `sha256`, which addresses previous collision-risk concerns. `vllm serve` can also choose algorithms such as `sha256_cbor`, `xxhash`, or `xxhash_cbor`; non-cryptographic choices may be faster, but they increase theoretical collision risk, which can lead to undefined behavior or even private-information leakage in multi-tenant environments.

### Q08

MVCC allows PostgreSQL to keep old row versions around so concurrent transactions can still see the versions that are valid for them. The maintenance cost is that `UPDATE` and `DELETE` do not immediately remove old versions; eventually those dead row versions and related index entries must be cleaned up or reused through vacuuming, otherwise tables can bloat.

HOT reduces part of that pressure for qualifying updates. If indexed columns are not changed and there is enough free space on the same page, PostgreSQL can store the new row version as a heap-only tuple without creating new index entries, and old versions can be pruned during normal operation instead of waiting only for periodic vacuum.

The visibility map is a separate per-table structure maintained by vacuum. It records pages whose tuples are known to be visible to all active and future transactions until modified. Because PostgreSQL indexes do not contain tuple visibility information, a normal index scan must fetch heap tuples to check visibility. An index-only scan can first check the visibility map; if the page is marked all-visible, it can skip the heap fetch. This is valuable on large tables because the visibility map is much smaller than the heap and can often stay cached.

### Q09

OpenZFS dRAID is a raidz variant with integrated distributed hot spares. It is built from multiple internal raidz groups, each with `D` data devices and `P` parity devices, distributed across the children. The distributed spare design allows faster resilvering because rebuild work can be spread across the pool rather than bottlenecking on a single physical spare.

dRAID uses a fixed stripe width, padding with zeros as needed. That fixed width enables fully sequential resilvering, but it also affects usable capacity and random IOPS. With the default `D=8` and 4 KiB sectors, the minimum allocation size is 32 KiB. For compressed data or small writes, that relatively large allocation size can reduce effective compression ratio and waste space. Random IOPS can be approximated by the number of full redundancy groups times single-drive IOPS, because reads involve the data disks in a group.

For small-block-heavy dRAID pools, the docs recommend adding a mirrored special vdev to store those blocks. The special vdev helps avoid forcing many small blocks into dRAID's larger fixed allocation pattern. Mirroring matters because the special vdev may hold important metadata or small-block data and should not become a weaker failure point than the rest of the pool.

### Q10

Bun `trustedDependencies` limits the risk of arbitrary dependency lifecycle scripts executing code during install. The trust decision is made by the project maintainer through `package.json`. The tradeoff is compatibility and convenience: packages that genuinely need install scripts may require explicit trust and reinstall steps.

MCP sampling's client-mediated, human-in-the-loop control limits the risk of servers silently invoking models, leaking data, choosing unauthorized models, or spending on LLM calls without user/client oversight. The trust decision is made by the client application and user, with the client retaining final control over provider access and model selection. The tradeoff is extra UI, approval friction, and the need for clients to implement review and permission flows.

vLLM prefix-cache salting limits cross-tenant cache reuse and timing-based inference about cached content. The trust decision is made by the serving system or request issuer by choosing whether requests share a `cache_salt`; only matching salts can reuse cached blocks. The tradeoff is that stronger isolation reduces cache-sharing opportunities and therefore can reduce performance gains from prefix caching.

Tailscale subnet-router access control limits which users or devices can reach advertised private subnets through a routing device. The trust decision is made through tailnet access policies and route approval/configuration. The tradeoff is operational complexity: admins must manage advertised routes, route approval, tags or grants, SNAT behavior, and access rules, and subnet-routed devices do not get the same direct end-to-end client posture as devices running Tailscale themselves.

## Source Basis

The questions above were grounded in retrieved indexed documents from these source families:

- Bun package-manager lifecycle scripts and `trustedDependencies`: `https://bun.sh/docs/pm/cli/install`
- PostgreSQL HOT updates: `https://www.postgresql.org/docs/16/storage-hot.html`
- PostgreSQL visibility map and index-only scans: `https://www.postgresql.org/docs/16/routine-vacuuming.html` and nearby indexed versions
- Tailscale subnet routers and exit nodes: `https://tailscale.com/docs/features/subnet-routers?tab=macos`
- Qdrant sparse vectors and hybrid search: `https://qdrant.tech/articles/sparse-vectors`
- OpenZFS special allocation classes and dRAID concepts: `https://openzfs.github.io/openzfs-docs/man/v2.4/7/zpoolconcepts.7.html`
- MCP sampling: `https://modelcontextprotocol.io/specification/2025-06-18/client/sampling`
- ACP schema/session material was queried for collection coverage, but the final questions favor MCP sampling for the protocol item.
- vLLM automatic prefix caching: `https://docs.vllm.ai/en/latest/design/prefix_caching`

## Timing Log Requirements For The Next Run

When these questions are run through model comparisons, log timing per question, not just per model. Capture at least:

| Field | Meaning |
|---|---|
| `question_id` | `Q01` through `Q10` |
| `provider` | Provider/base URL label, for example `cli-api.tootie.tv` or `llama.cpp` |
| `model` | Exact configured model name |
| `started_at` | ISO-8601 timestamp before invoking `axon ask` |
| `finished_at` | ISO-8601 timestamp after process exit |
| `elapsed_seconds` | Wall-clock runtime for that single `axon ask` call |
| `exit_code` | Process exit code |
| `stdout_file` | Markdown or text file containing the answer |
| `stderr_file` | Captured stderr/log file |

Suggested per-question wrapper shape:

```bash
started_at="$(date --iso-8601=seconds)"
start_ns="$(date +%s%N)"
set +e
target/release/axon ask "$question" >"$stdout_file" 2>"$stderr_file"
exit_code=$?
set -e
end_ns="$(date +%s%N)"
finished_at="$(date --iso-8601=seconds)"
elapsed_seconds="$(awk "BEGIN { printf \"%.3f\", ($end_ns - $start_ns) / 1000000000 }")"
printf '%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\t%s\n' \
  "$question_id" "$provider" "$model" "$started_at" "$finished_at" \
  "$elapsed_seconds" "$exit_code" "$stdout_file" "$stderr_file" >> timing.tsv
```
