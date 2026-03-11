# Base Rust image
FROM rust:1.81 as builder

WORKDIR /usr/src/app
COPY . .

RUN cargo install --path .

FROM debian:bookworm-slim
COPY --from=builder /usr/local/cargo/bin/unique_id_generator /usr/local/bin/unique_id_generator
CMD ["unique_id_generator"]
