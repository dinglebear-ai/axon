# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [1.0.0](https://github.com/dinglebear-ai/axon/compare/chrome-ext-v0.3.2...chrome-ext-v1.0.0) (2026-08-08)


### ⚠ BREAKING CHANGES

* artifact-bearing responses return opaque artifact IDs instead of server filesystem paths, and web source options use the canonical `headers` object shape.

### Added

* **#298:** post-smoke followups — scope=page, watch create, mutates_if, presentation tokens ([d05eab3](https://github.com/dinglebear-ai/axon/commit/d05eab3302a0427d931fe7a96170f07bf9d7ea02))
* **apps:** web token hardening, palette unified job polling, android memory/session client ([238ead7](https://github.com/dinglebear-ai/axon/commit/238ead7d924e79d49cf2f46e946b2fe1fa1934a8))
* **chrome-extension:** client-side redaction, blocked-scheme guards, memory-save, tests ([d062cba](https://github.com/dinglebear-ai/axon/commit/d062cba21e322f3514ade79296a5d69fa0b39c6f))
* **chrome-extension:** minimal host permissions + drop dead-route fan-out ([#298](https://github.com/dinglebear-ai/axon/issues/298) WS-I) ([8a001e9](https://github.com/dinglebear-ai/axon/commit/8a001e9374c3a7f253f1ea243364d654b61f549b))
* complete [#298](https://github.com/dinglebear-ai/axon/issues/298) closeout wave with structured source progress ([5bb4395](https://github.com/dinglebear-ai/axon/commit/5bb4395ede88b0f9ae6ae186d835bb3fd6787daa))


### Fixed

* **chrome-extension:** migrate legacy verb routes onto POST /v1/sources ([045d649](https://github.com/dinglebear-ai/axon/commit/045d6496269424cf4754c59221a23fffd077d6de))
* **release:** sync component versions after release PRs ([f7d0cfc](https://github.com/dinglebear-ai/axon/commit/f7d0cfc79572e4bd8ec1ce5d3a3e9501005c2133))
* **review:** close source watch review gaps ([dec600e](https://github.com/dinglebear-ai/axon/commit/dec600e3a94d00c0fa6a5e341f654300ca30c26c))


### Changed

* **chrome-extension:** restructure into contracted src/ module layout ([#298](https://github.com/dinglebear-ai/axon/issues/298) WS-I) ([0bf0da4](https://github.com/dinglebear-ai/axon/commit/0bf0da4231818886710d8fc6709637b083045ffe))
* **services:** retire dead-route Rest* DTO forks, document remaining diffs ([#298](https://github.com/dinglebear-ai/axon/issues/298) WS-E) ([73997c5](https://github.com/dinglebear-ai/axon/commit/73997c5f54b4fe488f1ed6a9e39b058cd91f0a33))

## [0.3.2](https://github.com/jmagar/axon/compare/chrome-ext-v0.3.1...chrome-ext-v0.3.2) (2026-07-17)

### Changed

* align with the unified source pipeline contract (#298 closeout) ([#442](https://github.com/jmagar/axon/pull/442))

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
