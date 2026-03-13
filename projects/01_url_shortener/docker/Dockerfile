# Base Rust image
FROM rustlang/rust:nightly as builder

WORKDIR /usr/src

# Copy the common crate
COPY common /usr/src/common

# Copy the app to the same relative depth as in the host
COPY projects/01_url_shortener /usr/src/projects/01_url_shortener

WORKDIR /usr/src/projects/01_url_shortener
RUN cargo install --path .

FROM debian:bookworm-slim
RUN apt-get update && apt-get install -y libssl-dev ca-certificates && rm -rf /var/lib/apt/lists/*
COPY --from=builder /usr/local/cargo/bin/url_shortener /usr/local/bin/url_shortener
CMD ["url_shortener"]
