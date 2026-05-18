# Axon Palette

`axon-palette` is the desktop command palette for the `axon` CLI. It opens from a global hotkey, filters common Axon actions, and shells out to the `axon` binary.

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

## Run

```bash
cargo run --manifest-path apps/desktop/Cargo.toml
```

or run the built binary directly:

```bash
apps/desktop/target/release/axon-palette
```

The palette resolves `axon` through `PATH` and executes it as a subprocess. Install or symlink the intended Axon CLI binary before launching the palette:

```bash
which axon
axon --version
```

If the wrong binary is found, fix the shell or desktop-session `PATH` before testing the palette. The palette forces local CLI execution for its actions, so a configured `AXON_SERVER_URL` should not redirect palette commands to a remote server.

## Hotkey

The default global hotkey is `Ctrl+Shift+Space`. The hotkey manager is kept alive for the life of the process; dropping it unregisters the binding.

If the palette starts but the hotkey does not focus the window, check whether another app already owns the shortcut or whether the compositor blocks global hotkeys.

## Smoke Test

Before packaging a desktop release:

1. Build the release binary for the target platform.
2. Ensure `which axon` points at the expected Axon CLI.
3. Launch `axon-palette`.
4. Confirm the window accepts typing immediately after launch.
5. Type `doctor`, press Enter, and confirm output appears.
6. Press `Ctrl+Shift+Space` from another focused app and confirm the palette activates.
7. Run an `ask` command, close and reopen the palette within 30 minutes, then confirm the next ask continues the restored conversation.

## Output Links

Markdown output renders links as their destination URL, not hidden label text. Clicks are user initiated and only `http://` and `https://` destinations are opened; other URI schemes are ignored.
