# code.claude.com cold-crawl optimization report

Date: 2026-08-12

Target: `https://code.claude.com/`

Hardware: NVIDIA GeForce RTX 4070, 12 GB VRAM

Embedding service: TEI 1.9.3, `Qwen/Qwen3-Embedding-0.6B`, FP16, last-token pooling

## Definition and benchmark controls

A **cold crawl** in this report means:

- a new `AXON_HOME` and `AXON_DATA_DIR` on `/dev/shm`;
- Axon's HTTP cache disabled with `--cache false`;
- a new source generation with no prior manifest or validators;
- the TEI model already resident on the GPU, as it is in the deployed service;
- either a fresh Qdrant collection (isolated harness semantics) or an empty,
  pre-created compatible collection (production-relevant semantics), called
  out explicitly below.

Every full run used `--scope site --wait true --json --quiet`, a unique Qdrant
collection, and verified document, chunk, and point counts. TEI and Qdrant
metrics were sampled around selected runs. The live site can change, so compare
only runs that report the same workload.

## Root cause: duplicate discovery representations

The upstream sitemap contained 2,013 URLs: 187 English URLs and 166 URLs for
each of 11 non-English locales. Axon's default locale exclusions already
removed the non-English paths correctly. The excess workload came from merging
two English representations:

- extensionless sitemap URLs such as `/docs/en/overview`;
- direct Markdown alternatives from `llms.txt`, such as
  `/docs/en/overview.md`.

Axon fetched and embedded both. The implemented fix prefers the direct
`.md`/`.markdown` URL when its extensionless equivalent exists and retains
HTML-only and Markdown-only pages. The discovery sufficiency threshold for an
`llms.txt`-backed map was set to 100 so the resulting 186 technical-document
URLs do not trigger unrelated root-page anchor expansion. Sitemap-only
discovery retains its original 200-URL safety threshold.

Verified map after the fix:

| Measurement | Result |
| --- | ---: |
| Sitemap URLs in scope | 187 |
| Final mapped URLs | 186 |
| Markdown URLs | 185 |
| Unrelated non-doc URLs | 0 |

The full cold workload became 187 documents, 3,878 prepared chunks, and 3,801
stored points. The point difference is caused by fail-closed secret-detector
rejections; the detector policy was not weakened.

## Crawler and language-filter evidence

On the corrected corpus, instrumented phase timing attributed 1.17 seconds to
discovery and 3.78 seconds to fetching. There were no fetch retries. The
pre-fix 370-document runs spent 1.29-2.87 seconds discovering and 5.18-8.52
seconds fetching; their dominant costs were embedding and upserting the
duplicate representation, not crawling itself. This bounds the realistic gain
from further HTTP concurrency, retry, render, or asset-blocking tuning to less
than the gap needed for the 35-second target.

The site is served directly as Markdown from the discovered `.md` URLs, so the
optimized runs stayed on the HTTP path and did not pay for Chrome rendering.
Asset blocking therefore cannot reduce this workload. Cache and conditional
requests were disabled by definition; enabling them would measure a warm
recrawl instead of the requested cold crawl. Lowering page limits or depth
would make the corpus incomplete and was not counted as an optimization.

Language filtering was verified at both boundaries:

- discovery reduced the 2,013-entry sitemap to the English scope before
  acquisition;
- a scroll over all 3,801 retained points found 187 unique source URLs: 186
  under `/docs/en/` plus the site root and zero non-English locale paths.

## End-to-end results

| Configuration | Collection state | Wall time | Notes |
| --- | --- | ---: | --- |
| Before representation dedupe | fresh | 187.8 s | About 370 documents |
| Dedupe fix, normal filesystem state | fresh | 68.31 s | 187 documents |
| Dedupe fix, `/dev/shm` state | fresh | 54.60 s | Removes slow SQLite/ZFS initialization |
| TEI 163,840 tokens / 8 tokenizer workers | fresh | **52.81 s** | Best isolated full-crawl result |
| Fresh repeat 1, same profile | fresh | about 81.8 s | One 6.32 s SQLite stall |
| Fresh repeat 2, same profile | fresh | 81.10 s | Same 187 / 3,878 / 3,801 workload |
| Fresh repeat 3, same profile | fresh | 66.04 s | Same 187 / 3,878 / 3,801 workload |
| Pre-created indexed collection | pre-created | 65.29 s | Zero TEI overloads; machine load varied |
| Pre-created bulk-load collection | pre-created, HNSW deferred | **55.83 s** | 9.46 s faster than indexed pre-created run |

The requested sub-35-second end-to-end target was not reached on this live
hardware and corpus. The limiting lower bound was measured directly rather
than inferred: embedding the exact 3,801 stored chunk texts through TEI takes
approximately 29.6–31.7 seconds before discovery, HTTP acquisition, redaction,
Qdrant writes, ledger publication, or CLI startup are included.

The three final fresh repeats are deliberately reported even though they are
slower than the best run. They demonstrate that 52.81 seconds is observed
best-case capability, not a stable median. Host-side SQLite and Qdrant work and
live-site/network conditions introduce material variance outside the much
tighter direct-TEI replay band.

## TEI and client tuning matrix

Full cold-crawl results used identical 187-document workloads unless noted:

| Knob | Candidate | Result |
| --- | ---: | ---: |
| Max batch tokens / tokenizer workers | 196,608 / 20 | 54.60 s |
| Max batch tokens / tokenizer workers | **163,840 / 8** | **52.81 s** |
| Max batch tokens / tokenizer workers | 131,072 / 8 | 55.68 s |
| Max batch tokens / tokenizer workers | 163,840 / 4 | 72.27 s |
| Axon concurrent TEI requests | 16 | 76.60 s |
| Axon concurrent TEI requests | 8 | baseline/best region |
| Axon concurrent TEI requests | 4 | 56.32 s in the initial matrix |
| In-flight inputs | 480 | 57.05 s |
| Client batch | 128 | 54.70 s versus 54.60 s at 96 |

Exact-corpus TEI replay, 3,801 inputs and about 1.23 million tokens:

| Client shape | Wall time | Outcome |
| --- | ---: | --- |
| Batch 32, concurrency 1 | 34.64 s | success |
| Batch 32, concurrency 4 | 34.06 s | success |
| Batch 64, concurrency 1 | 35.63 s | success |
| Batch 64, concurrency 4 | 31.55 s | success |
| Batch 96, concurrency 1 | 31.30 s | success |
| Batch 96, concurrency 2-5 | 30.18-30.46 s | success |
| Batch 96, concurrency 6 | 3.92 s | failed with overload |
| Batch 128, concurrency 1 | 33.23 s | success |
| Batch 128, concurrency 4 | 31.01 s | success |
| Globally length-sorted, batch 96 / concurrency 4 | 29.62 s | success |
| Per-512-pool length-sorted | 30.05 s | success |

Increasing concurrency beyond the server's token capacity causes TEI `429`
responses. Axon's retries make those runs slower and more variable. Length
sorting produced only a marginal improvement and does not justify reordering
the production pipeline by itself.

The source executor forms 512-chunk vector batches. The TEI transport now
overlaps its client-sized requests while preserving response order. Concurrency
is bounded by both `AXON_TEI_MAX_CONCURRENT` and
`AXON_TEI_MAX_IN_FLIGHT_INPUTS / TEI_MAX_CLIENT_BATCH_SIZE`; the default
`8 / 320 / 96` profile therefore admits three requests, staying below the
measured overload boundary. Exact-corpus replay before this implementation
showed a realistic upside of about one second; a new end-to-end benchmark is
still required before attributing a production wall-time gain to the change.

TEI 1.9.3 exposes no quantized or FP8 mode for this backend. Its available
dtype choices are FP16 and FP32; FP16 is already in use. The deployed server
already uses FlashQwen3, last-token pooling, automatic truncation, a 512-request
admission ceiling, and a 128-input client ceiling.

The persisted best measured server profile is:

```dotenv
TEI_MAX_BATCH_TOKENS=163840
TEI_MAX_BATCH_REQUESTS=512
TEI_SERVER_MAX_CLIENT_BATCH_SIZE=128
TEI_TOKENIZATION_WORKERS=8
```

## GPU evidence

During exact-corpus replay the RTX 4070 sustained 98-100% SM utilization,
174-190 W, 2.61-2.76 GHz core clocks, and 10,251 MHz memory clocks. It reached
82-84 C and intermittently reported thermal limiting.

Reversible tests were restored after measurement:

| GPU setting | TEI replay | Observation |
| --- | ---: | --- |
| 200 W, target 84 C | 30.68 s | baseline; intermittent thermal limiting |
| 170 W, target 84 C | 31.01 s | no thermal samples, but slower |
| 200 W, target 87 C | 31.75 s | slower; thermal samples remained |

The GPU is compute-bound. CPU tokenization, PCIe bandwidth, and client request
fanout are not the primary limit. Better physical cooling may improve sustained
clocks, but neither the tested power cap nor a higher temperature target did.

## Chunking and content experiments

The indexed corpus contains 3,801 point texts totaling about 5.01 million
characters. Chunk lengths range from 4 to 52,114 characters; 1,144 are under
500 characters and 29 exceed 8,000 characters.

Losslessly splitting only oversized texts before direct TEI replay did not
improve throughput:

| Maximum characters | Inputs after split | TEI wall time |
| --- | ---: | ---: |
| 2,000 | 4,749 | 30.49 s |
| 4,000 | 4,025 | 31.00 s |
| 8,000 | 3,850 | 31.32 s |
| 16,000 | 3,814 | 30.74 s |

Cross-document exact duplicates account for only 211 inputs. Of those, 183
are the short repeated `Documentation Index` block. Reusing their embeddings
would save little relative to the 1.23-million-token corpus and was not used to
claim an artificial speedup.

## Qdrant findings

The measured baseline created 25 required payload indexes sequentially and
cost roughly 9-10 seconds. Payload-index requests are now issued with bounded
parallelism controlled by `AXON_QDRANT_PAYLOAD_INDEX_PARALLELISM`. The profile
selector still requires separate collection-spec wiring. Manual parallel index
creation showed Qdrant serializes much of the work internally:

| Client parallelism | 25 indexes |
| ---: | ---: |
| 1 | 7.97 s |
| 4 | 7.85 s |
| 8 | 6.78 s |
| 16 | 6.22 s |
| 25 | 6.20 s |

Deferring HNSW construction during ingestion is valuable. A compatible
pre-created collection with a bulk indexing threshold reduced a cold crawl
from 65.29 seconds to 55.83 seconds with the same document/chunk/point counts.
This is a throughput profile, not permission to omit required indexes or leave
the collection permanently unindexed. Production bulk-load flows must restore
the normal indexing threshold and verify optimizer health before declaring the
collection ready for retrieval.

## Conclusions and remaining options

The discovery fix is a large, quality-preserving improvement and ensures Axon
ingests the English technical corpus once rather than ingesting both HTML and
Markdown representations. With the current model and one RTX 4070, however,
the measured TEI-only floor leaves less than five seconds for the rest of a
sub-35-second crawl. The live acquisition and persistence pipeline cannot meet
that budget without a material change beyond tuning existing knobs.

Credible next options, in descending order:

1. Use a faster embedding model and validate retrieval quality on a fixed
   relevance corpus. This changes vector compatibility and requires a new
   collection; it must not silently overwrite the Qwen3 index.
2. Improve GPU cooling and repeat the exact-corpus replay. The current card is
   already compute-saturated and touches its thermal target.
3. Re-run the isolated benchmark with bounded TEI request overlap and Qdrant
   write/index fanout enabled; retain the settings only where the measured
   workload improves.
4. Make bulk-load collection configuration effective for both new and
   explicitly opted-in existing collections, then restore and verify HNSW
   indexing after upload.
5. Wire or remove the remaining ineffective payload-index profile setting so
   configuration accurately describes runtime behavior.

No test weakened secret redaction, dropped non-duplicate English pages, reused
a warm Axon crawl cache, or counted a failed TEI overload run as a speed result.

## Reproduction and primary references

Run the repository benchmark harness against an isolated local corpus with:

```bash
TEI_URL=http://127.0.0.1:52000 \
QDRANT_URL=http://127.0.0.1:6333 \
cargo xtask bench-embed CORPUS_PATH --axon-bin target/release/axon --json
```

The harness now creates a unique `AXON_HOME`/`AXON_DATA_DIR` for every run so
the global source ledger cannot turn a repeated corpus into a misleading
zero-document result. A smoke run against `README.md` prepared 3 documents,
18 chunks, and 18 points and removed its temporary collection successfully.

The full-crawl control can be reproduced with a unique state directory and
collection (substitute the deployed Qdrant URL):

```bash
state_dir=$(mktemp -d /dev/shm/axon-cold.XXXXXX)
collection="axon_cold_$(date +%s)"
AXON_HOME="$state_dir" \
AXON_DATA_DIR="$state_dir" \
AXON_COLLECTION="$collection" \
TEI_URL=http://127.0.0.1:52000 \
QDRANT_URL=http://127.0.0.1:6333 \
/usr/bin/time -f 'wall_seconds=%e' \
  target/release/axon source https://code.claude.com/ \
  --scope site --cache false --wait true --json --quiet
```

Upstream behavior and current-version claims were checked against primary
sources:

- [TEI README and runtime flags](https://github.com/huggingface/text-embeddings-inference/blob/main/README.md)
- [TEI releases; v1.9.3 is the current release](https://github.com/huggingface/text-embeddings-inference/releases)
- [Qdrant bulk upload guidance](https://qdrant.tech/documentation/database-tutorials/bulk-upload/)
- [Qdrant indexing optimization guidance](https://qdrant.tech/articles/indexing-optimization/)
