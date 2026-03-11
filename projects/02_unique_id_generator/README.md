# 02 Unique ID Generator

## Requirements
- Generate unique 64-bit IDs.
- High availability.
- Low latency.

## Implementation Details
- Snowflake-like ID generation.
- Distributed counter using Zookeeper or similar for coordination (optional).
