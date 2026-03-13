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
```mermaid
graph TD
    User([User]) <--> API[Axum API Layer]
    API <--> Logic[App Manager - Service Layer]
    Logic <--> Repo[Url Repository - Data Access Layer]
    Repo <--> DB[(PostgreSQL)]
```

#### URL Shortening Flow
```mermaid
sequenceDiagram
    participant U as User
    participant H as Handler (Axum)
    participant M as Manager (AppManager)
    participant R as Repository (PostgresUrlRepository)
    participant D as Database (Postgres)

    U->>H: POST /shorten {url}
    H->>M: shorten_url(url)
    M->>M: generate nanoid(8)
    M->>R: save(url, code)
    R->>D: INSERT INTO urls...
    D-->>R: UrlRecord
    R-->>M: UrlRecord
    M-->>H: short_code
    H-->>U: 201 Created {short_code}
```

#### URL Redirection Flow
```mermaid
sequenceDiagram
    participant U as User
    participant H as Handler (Axum)
    participant M as Manager (AppManager)
    participant R as Repository (PostgresUrlRepository)
    participant D as Database (Postgres)

    U->>H: GET /{code}
    H->>M: get_long_url(code)
    M->>R: get_by_code(code)
    R->>D: SELECT ... FROM urls WHERE code = ?
    D-->>R: Result
    R-->>M: Option<UrlRecord>
    M-->>H: Option<String>
    alt code found
        H-->>U: 307 Temporary Redirect {url}
    else code not found
        H-->>U: 404 Not Found
    end
```

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
