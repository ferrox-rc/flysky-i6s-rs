# Contributing to flysky-i6x-rs

Thank you for your interest in contributing to `flysky-i6x-rs`! This document details our Git workflow, branching conventions, and development standards.

---

## 1. Branching Strategy

Our repository uses a structured Git branching workflow based on folder-style namespacing:

- **`main`**: Production and official stable release tags only.
- **`dev`**: Primary integration branch for active development.
- **Dedicated Task Branches**: All work (features, bug fixes, refactoring, docs) must be developed on a dedicated branch created from `dev`.

### Branch Naming Convention

Branch names must follow the format: `<prefix>/<kebab-case-description>`

| Prefix | Purpose | Example |
| :--- | :--- | :--- |
| `feat/` | New user-facing feature or protocol support | `feat/crsf-v3-telemetry` |
| `fix/` | Bug fixes or hardware quirk corrections | `fix/spi-dma-transfer-stall` |
| `refactor/` | Internal restructuring, concurrency, or code cleanup | `refactor/rtic-concurrency-model` |
| `docs/` | Documentation additions or updates | `docs/architecture-diagrams` |
| `perf/` | Flash size, SRAM reduction, or timing optimizations | `perf/lut-spline-interpolation` |
| `test/` | Adding test cases, harnesses, or CI pipelines | `test/channel-mixer-harness` |
| `chore/` | Toolchain updates, dependency bumps, linker scripts | `chore/update-cortex-m-hal` |

### Starting New Work

Always ensure you are branched from an up-to-date `dev`:

```bash
git checkout dev
git pull origin dev
git checkout -b <prefix>/<short-description>
```

---

## 2. Commit Standards

We adhere to [Conventional Commits](https://www.conventionalcommits.org/):

- `feat: add Catmull-Rom spline throttle curve interpolation`
- `fix: correct EXTI2 flag clear sequence in RF ISR`
- `refactor: adopt lock-free atomic double-buffering for channels`
- `docs: update concurrency model in README`

---

## 3. Firmware Design Principles

- **`no_std` Bare-Metal:** No heap allocations, no dynamic sizing at runtime.
- **Deterministic Latency:** RF packet transmission (`TIM16`) and critical interrupts must not be blocked by non-essential tasks.
- **Resource Footprint:** Maintain awareness of Flash (<128 KB) and SRAM (<16 KB) utilization.

---

## 4. Testing & Verification Standards

To guarantee mathematical precision and prevent regressions across protocol state machines, all pull requests and contributions must satisfy the **Dual Verification Requirement**:

### 1. Run Host Unit Tests
Execute the automated test suite locally on your host development environment:
```bash
cargo test-host
```
All unit tests must pass with 0 failures before opening a pull request. When introducing new mathematical formulas, mixing modes, or communication protocols, include corresponding unit tests in the appropriate module.

### 2. Verify Bare-Metal Build
Verify that the firmware compiles cleanly for the physical Cortex-M0 target without errors or compiler warnings:
```bash
cargo build --release --target thumbv6m-none-eabi
```

For comprehensive details on our dual-target harness architecture, peripheral mocking, and adversarial verification methodology, refer to the **[Testing Methodology & Verification Guide](docs/TESTING.md)**.

