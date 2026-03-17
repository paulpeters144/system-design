# Log Access Layer Development Plan

## Project Overview
This task focuses on implementing the **Log Access Layer**, the storage engine's "physical" interface. Its sole responsibility is to provide efficient, reliable, and thread-safe operations for appending data to and reading data from the disk. This layer abstracts away file handles, file pointers, and byte-level offsets, providing the `AppManager` with a logical stream of messages.

## Technical Stack
- **Language:** Rust
- **Runtime:** `tokio` (Asynchronous File I/O)
- **Crate:** `tokio::fs` (for non-blocking disk operations)
- **Safety:** `std::sync::Arc` and `tokio::sync::Mutex` (for synchronized write access)

## Phases of Development

### Phase 1: Research & Discovery
- **Storage Strategy:** 
    - Evaluate **Segmented Logs**: Instead of one massive file, use multiple files (segments) of fixed size (e.g., 100MB). This simplifies deletion (retention) and recovery.
    - **Indexing**: Research sparse vs. dense indices. A sparse index (mapping every N-th message to a byte offset) reduces memory usage while maintaining fast lookups.
- **System Constraints:**
    - **O_APPEND:** Investigate filesystem behavior regarding atomic appends in a multi-threaded context.
    - **File Syncing:** Research `fdatasync` vs. `fsync` for balancing performance with data durability guarantees.
- **Error States:** Define disk-specific errors: `DiskFull`, `SegmentCorrupted`, `OffsetOutOfRange`.

### Phase 2: Architecture & Design
- **Data Structures:**
    - `LogAccess`: The primary entry point. Holds a list of `Segments`.
    - `Segment`: Represents a single pair of `.log` (data) and `.index` (offset mapping) files.
- **The Interface (API):**
    - `append(data: &[u8]) -> Result<u64>`: Appends bytes and returns the assigned logical offset.
    - `read(offset: u64) -> Result<Vec<u8>>`: Locates the correct segment, uses the index to find the byte position, and returns the message.
- **Persistence Format:**
    - **Log Entry:** `[4-byte CRC] [4-byte Length] [N-byte Payload]`.
    - **Index Entry:** `[8-byte Logical Offset] [8-byte Physical Position]`.

### Phase 3: Implementation Steps
- **Task 1: Segment Abstraction:** Implement a `Segment` struct that handles the low-level opening and closing of a single data file.
- **Task 2: Atomic Appender:** Implement the `append` logic in `LogAccess`. Ensure that only one task can write to the "active" segment at a time using a Mutex.
- **Task 3: Indexing Engine:** Implement the index file logic. Every write to the `.log` must create a corresponding entry in the `.index`.
- **Task 4: Logical-to-Physical Translation:** Implement the `read` logic. This requires binary searching the active segments to find which one contains the requested logical offset.
- **Task 5: Boot Recovery:** Implement a "bootstrap" process that scans the data directory on startup and reconstructs the `LogAccess` state (identifying the latest offset).

### Phase 4: Testing & Quality Assurance
- **Concurrency Stress Test:** Multiple tasks appending simultaneously to verify the Mutex prevents interleaving/corruption.
- **Integrity Test:** Simulate a crash (kill process), restart, and verify that the `LogAccess` can still read previously written data.
- **Boundary Test:** Attempt to read an offset that doesn't exist or a message that spans a segment boundary.
- **Performance Benchmarking:** Measure the latency of a `write + flush` operation compared to a `write` (buffered).

## Potential Challenges
- **File Handle Exhaustion:** Managing many open segment files simultaneously.
- **Index Corruption:** Ensuring the index stays in sync with the log if the system crashes mid-write.
- **Performance Bottlenecks:** The `Mutex` on the active segment will be the primary contention point for producers.
