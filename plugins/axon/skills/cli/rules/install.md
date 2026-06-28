# Axon CLI Installation

Axon is normally available as `axon` on PATH:

```bash
axon doctor
```

Inside the Axon source checkout, `./scripts/axon doctor` is also valid when you
specifically want the repo wrapper that loads the local environment. If `axon`
is unavailable, build or install the binary:

```bash
cargo build --release --bin axon
./target/release/axon doctor
```

For a host-level install, use the repo setup/update flow:

```bash
axon setup check
axon update
```

## Verify

Run a small command that does not require fetching untrusted content:

```bash
axon doctor
axon stats
```

Then run one tiny scrape if web access is needed:

```bash
mkdir -p .axon
axon scrape "https://example.com" --output .axon/install-check.md
```

The install is healthy when doctor can reach the configured services and the scrape writes the output file.
