# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [0.3.1](https://github.com/jmagar/axon/compare/chrome-ext-v0.3.0...chrome-ext-v0.3.1) (2026-07-14)


### Fixed

* **release:** sync component versions after release PRs ([4d023e7](https://github.com/jmagar/axon/commit/4d023e72b5951c7468c843a906ca9ceb10336a09))

## [0.3.0](https://github.com/jmagar/axon/compare/chrome-ext-v0.2.2...chrome-ext-v0.3.0) (2026-07-14)


### Added

* **#298:** post-smoke followups — scope=page, watch create, mutates_if, presentation tokens ([e01592f](https://github.com/jmagar/axon/commit/e01592ff278bcd5543924a9e87c2072d346d7878))
* **apps:** web token hardening, palette unified job polling, android memory/session client ([a17dc86](https://github.com/jmagar/axon/commit/a17dc864dafb67064819ea12c2ccdc004d01eec4))
* **chrome-extension:** client-side redaction, blocked-scheme guards, memory-save, tests ([c82cbcf](https://github.com/jmagar/axon/commit/c82cbcf6276ef54bfbdffe9dba6f01d051d2de42))
* **chrome-extension:** minimal host permissions + drop dead-route fan-out ([#298](https://github.com/jmagar/axon/issues/298) WS-I) ([0c0068b](https://github.com/jmagar/axon/commit/0c0068b219d007370b280bcbbb55fd2962f04a61))


### Fixed

* **chrome-extension:** migrate legacy verb routes onto POST /v1/sources ([5a812a1](https://github.com/jmagar/axon/commit/5a812a179c9f65fb53eb89e11c1d831d81d3f08b))


### Changed

* **chrome-extension:** restructure into contracted src/ module layout ([#298](https://github.com/jmagar/axon/issues/298) WS-I) ([8b1e208](https://github.com/jmagar/axon/commit/8b1e208f8c1c3e433404b7d0ffd4baba8f000453))
* **services:** retire dead-route Rest* DTO forks, document remaining diffs ([#298](https://github.com/jmagar/axon/issues/298) WS-E) ([72f2067](https://github.com/jmagar/axon/commit/72f2067d521e4289652c51c2b5c48fb279208619))

## [0.2.2] - 2026-06-24

### Changed

- Align launcher defaults with server transport request policy.

## [0.2.1] - 2026-06-21

### Added

- Aurora side-panel launcher from design handoff (#191)
- Add per-component changelogs and register them in release manifest

## [0.1.0] - 2026-06-08

### Added

- Add Tauri palette and harden search crawl (#136)
- Add independent GitHub Release workflow for the Chrome extension
