// Stable Palette import boundary for the shared presentation-token system.
//
// The canonical token source is `xtask/src/presentation/source.json`.
// `cargo xtask presentation generate` projects it to
// `apps/palette-tauri/src/styles/axon-tokens.css`, alongside the web,
// Chrome-extension, Android, CLI, and documentation outputs. Palette consumes
// the generated CSS through its application stylesheet; this module remains a
// stable TypeScript boundary for future typed token helpers.
export {};
