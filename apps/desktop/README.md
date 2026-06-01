# Axon Palette

`axon-palette` is the desktop command palette for the Axon REST API. It opens from a global hotkey, filters common Axon actions, and calls the running Axon server directly.

## Platforms

- Linux and FreeBSD use the GPUI Linux backend. Linux builds link X11, Wayland, xcb, OpenSSL, and fontconfig development packages.
- Windows uses the native GPUI Windows backend and is built on a native Windows runner.
- macOS is not currently enabled; unsupported platforms fail at compile time.
- Wayland global-hotkey support depends on the compositor and desktop portal behavior. X11 is the most predictable Linux path today.

## Build

The desktop app is intentionally its own Cargo workspace under `apps/desktop`.

```bash
cargo build --manifest-path apps/desktop/Cargo.toml
cargo build --release --locked --manifest-path apps/desktop/Cargo.toml
```

Release artifacts are written under `apps/desktop/target/release/`:

- Linux/FreeBSD: `axon-palette`
- Windows: `axon-palette.exe`

## Test

```bash
cargo test --locked --manifest-path apps/desktop/Cargo.toml
```

The dedicated desktop CI workflow runs this command before release builds on both Linux and Windows.

For Windows operation screenshot testing, use
[`docs/contributing/desktop-palette-testing.md`](/home/jmagar/workspace/axon_rust/docs/contributing/desktop-palette-testing.md).

## Run

```bash
cargo run --manifest-path apps/desktop/Cargo.toml
```

or run the built binary directly:

```bash
apps/desktop/target/release/axon-palette
```

The palette calls the Axon server REST API. It reads `AXON_SERVER_URL` when set and otherwise defaults to `http://127.0.0.1:8001`. If the server requires static bearer auth, set `AXON_MCP_HTTP_TOKEN` in the palette process environment.

```bash
AXON_SERVER_URL=http://127.0.0.1:8001 apps/desktop/target/release/axon-palette
```

## Hotkey

The default global hotkey is `Ctrl+Shift+Space`. The hotkey manager is kept alive for the life of the process; dropping it unregisters the binding.

If the palette starts but the hotkey does not focus the window, check whether another app already owns the shortcut or whether the compositor blocks global hotkeys.

## Smoke Test

Before packaging a desktop release:

1. Build the release binary for the target platform.
2. Start `axon serve` and confirm `/v1/doctor` is reachable.
3. Launch `axon-palette`.
4. Confirm the window accepts typing immediately after launch.
5. Type `doctor`, press Enter, and confirm output appears.
6. Press `Ctrl+Shift+Space` from another focused app and confirm the palette activates.
7. Run `scrape https://docs.rs/serde` and confirm the palette shows the REST response.

## Output Links

Markdown output renders links as their destination URL, not hidden label text. Clicks are user initiated and only `http://` and `https://` destinations are opened; other URI schemes are ignored.
