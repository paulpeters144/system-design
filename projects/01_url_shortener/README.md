# 01 URL Shortener

A high-performance URL shortener service built with Rust, focusing on clean architecture and scalability.

## Architecture

This project follows a layered architecture to ensure separation of concerns and maintainability:

1.  **API Layer (`handler.rs`)**: Built with Axum, this layer handles HTTP requests, input validation (via Serde), and provides OpenAPI documentation (via Utoipa).
2.  **Service Layer (`manager.rs`)**: The `AppManager` contains the core business logic. It uses **Dependency Injection** to interact with the data layer through the `UrlRepository` trait, making the code highly testable and decoupled from the specific database implementation.
3.  **Data Access Layer (`access.rs`)**: Implements the **Repository Pattern** using `sqlx`. It provides a concrete PostgreSQL implementation of the `UrlRepository` trait, handling SQL queries and data mapping.
4.  **Infrastructure**: PostgreSQL is used for persistent storage of URL mappings.

### System Design

#### High-Level Architecture
![High-Level Architecture](docs/images/high_level.png)

#### URL Shortening Flow
![URL Shortening Flow](docs/images/shorten_flow.png)

#### URL Redirection Flow
![URL Redirection Flow](docs/images/redirect_flow.png)

## Tech Stack

- **Language**: [Rust](https://www.rust-lang.org/) (Edition 2024)
- **Web Framework**: [Axum](https://github.com/tokio-rs/axum)
- **Database**: [PostgreSQL](https://www.postgresql.org/) with [SQLx](https://github.com/launchbadge/sqlx) for type-safe async queries.
- **Async Runtime**: [Tokio](https://tokio.rs/)
- **ID Generation**: [nanoid](https://github.com/p-nerd/nanoid-rs) (8-character short codes)
- **API Documentation**: [Utoipa](https://github.com/juhakivekas/utoipa) (Swagger UI available at `/swagger-ui`)
- **Tracing**: [tracing](https://github.com/tokio-rs/tracing) for structured logging.

## Getting Started

### Prerequisites
- Docker and Docker Compose
- Rust toolchain

### Running the Project
1. Start the database:
   ```powershell
   docker-compose up -d
   ```
2. Set the environment variable:
   ```powershell
   $env:DATABASE_URL="postgres://user:password@localhost:5432/url_shortener"
   ```
3. Run the application:
   ```powershell
   cargo run
   ```

## Requirements (Original)
- Shorten a long URL to a short link.
- Redirect from short link to original long URL.
- Support high availability and scalability.
