# Base Rust image
FROM rust:1.81 as builder

WORKDIR /usr/src/app
COPY . .

# Note: In a workspace, you might need to copy 'common' as well.
# For simplicity, we assume 'common' is available or this is a simple build.
# A more robust Dockerfile would handle workspace structures properly.

RUN cargo install --path .

FROM debian:bookworm-slim
COPY --from=builder /usr/local/cargo/bin/url_shortener /usr/local/bin/url_shortener
CMD ["url_shortener"]
