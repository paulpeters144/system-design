# Zero-Panic Refactoring Plan

## Project Overview
This plan defines the transition of `kafka_lite` to a zero-panic architecture. We will eliminate all run-time panics (including those from `unwrap()` and `expect()`) in favor of explicit error handling. This ensures the broker remains resilient even when encountering corrupted data or unexpected system states.

## Technical Stack
- **Language:** Rust
- **Tools:** `cargo clippy`, `cargo test`

## Phases of Development

### Phase 1: Policy & Architecture
- **Zero-Panic Policy:** Use of `unwrap()`, `expect()`, or `panic!()` is strictly forbidden in the core logic.
- **Error Mapping Strategy:**
    - All slice conversions (e.g., `try_into()`) must return `io::Error` with `ErrorKind::InvalidData`.
    - All component constructors (e.g., `AppManager::new`) must return `Result<Self, E>`.
- **Untrusted Data Principle:** Treat all data read from disk (headers, indices, payloads) as potentially corrupted.

### Phase 2: Implementation Steps

#### Task 1: Defensive Slice Conversions
Refactor `src/access/segment.rs` and `src/codec.rs` to remove the `expect()` calls I recently introduced.
- **Strategy:** Replace with `.map_err(|_| io::Error::new(io::ErrorKind::InvalidData, "corrupted header/index slice"))?`.
- **Files:** `src/access/segment.rs`, `src/codec.rs`.

#### Task 2: Fallible Component Initialization
Refactor `AppManager` to handle regex compilation failure gracefully.
- **Strategy:** Change `AppManager::new` to return `Result<Self, AppError>`.
- **Files:** `src/manager/app_manager.rs`.

#### Task 3: Hardening Core Logic
Audit `TopicLog` and `Segment` for any remaining implicit panics (e.g., array indexing that could out-of-bounds).
- **Strategy:** Use `.get()` or checked arithmetic where appropriate.

#### Task 4: Test Suite Cleanup
Update unit tests that currently use `unwrap()` to use `?` in `#[tokio::test] -> io::Result<()>` or `expect()` (only permitted in test code).

### Phase 3: Validation & Quality Assurance
- **Clippy Enforcement:** Run `cargo clippy -- -D clippy::unwrap_used -D clippy::expect_used`.
- **Integration Testing:** Ensure the system handles corrupted files by returning `Err` instead of crashing.
- **Panic Audit:** Scan the final binary or use `no_panic` attribute if necessary for critical paths.

## Potential Challenges
- **API Surface Changes:** Making constructors fallible will require updates to `main.rs` and test setup code.
- **Error Verbosity:** Mapping simple conversions to `io::Error` adds boilerplate, which we will mitigate using helper methods or macros if needed.
