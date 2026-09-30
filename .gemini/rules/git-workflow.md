# Git Workflow & Branching Conventions

Always follow this branching and development workflow for `flysky-i6x-rs`:

## 1. Branching Strategy
- **Base Branch:** All ongoing development and feature branches branch off `dev`. The `main` branch is reserved strictly for production/stable releases.
- **Dedicated Branching:** Before making non-trivial code modifications, always ensure work is performed on a dedicated branch created from `dev`.
- **Namespacing with Slashes:** Branch names must use conventional folder-style namespacing: `<prefix>/<kebab-case-description>`.

### Standard Branch Prefixes
| Prefix | Purpose | Example |
| :--- | :--- | :--- |
| `feat/` | New user-facing feature or protocol support | `feat/crsf-v3-telemetry` |
| `fix/` | Bug fixes or hardware quirk corrections | `fix/spi-dma-transfer-stall` |
| `refactor/` | Internal restructuring, concurrency, or code cleanup | `refactor/rtic-concurrency-model` |
| `docs/` | Documentation additions or updates | `docs/architecture-diagrams` |
| `perf/` | Flash size, SRAM reduction, or timing optimizations | `perf/lut-spline-interpolation` |
| `test/` | Adding test cases, harnesses, or CI pipelines | `test/channel-mixer-harness` |
| `chore/` | Toolchain updates, dependency bumps, linker scripts | `chore/update-cortex-m-hal` |

## 2. Commit Standards
- Use Conventional Commits (`feat: ...`, `fix: ...`, `refactor: ...`, `docs: ...`, `perf: ...`, `chore: ...`).
- Keep commits focused and atomic.
