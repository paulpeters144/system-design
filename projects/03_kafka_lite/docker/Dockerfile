# Base Rust image
FROM rustlang/rust:nightly as builder

WORKDIR /usr/src

# Copy the app to the same relative depth as in the host
COPY projects/03_kafka_lite /usr/src/projects/03_kafka_lite

WORKDIR /usr/src/projects/03_kafka_lite
RUN cargo install --path .

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/kafka_lite /usr/local/bin/kafka_lite
CMD ["kafka_lite"]
