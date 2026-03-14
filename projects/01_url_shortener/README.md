# 01 URL Shortener

A high-performance URL shortener service built with Rust, focusing on clean architecture and scalability.

## Architecture

This project follows a layered architecture to ensure separation of concerns and maintainability:

1.  **API Layer (`handler.rs`)**: Built with Axum, this layer handles HTTP requests, input validation (via Serde), and provides OpenAPI documentation (via Utoipa).
2.  **Service Layer (`manager.rs`)**: The `AppManager` contains the core business logic. It uses **Dependency Injection** to interact with the data layer through traits (`UrlRepository`, `CacheRepository`, `AnalyticsRepository`), making the code highly testable and decoupled.
3.  **Data Access Layer (`access.rs`)**: Implements the **Repository Pattern**.
    *   **PostgreSQL**: Handles persistent storage of URL mappings and click analytics.
    *   **Redis**: Provides a high-performance caching layer for URL lookups.
4.  **Analytics Layer**: Records usage data (IP, User-Agent) asynchronously, including automated IP anonymization to protect user privacy.

#### URL Shortening Flow
Handles the creation of a new short link with collision detection and persistence.
```mermaid
sequenceDiagram
    participant U as User
    participant A as API Layer
    participant M as Manager
    participant D as Data Access Layer
    participant DB as Database

    U->>A: POST /shorten {url}
    A->>M: shorten_url(url)
    loop Up to 3 attempts
        M->>M: generate nanoid(8)
        M->>D: save(url, code)
        D->>DB: INSERT (Check Conflict)
        DB-->>D: Result/Conflict
    end
    D-->>M: Record
    M-->>A: short_code
    A-->>U: 201 Created {short_code}
```

#### URL Redirection Flow
Retrieves the original URL using a Cache-Aside strategy and records analytics asynchronously.
```mermaid
sequenceDiagram
    participant U as User
    participant A as API Layer
    participant M as Manager
    participant C as Cache (Redis)
    participant D as Data Access Layer
    participant DB as Database

    U->>A: GET /{code}
    A->>M: get_url_record(code)
    M->>C: get(code)
    alt cache hit
        C-->>M: Record
    else cache miss
        M->>D: get_by_code(code)
        D->>DB: SELECT
        DB-->>D: Record
        D-->>M: Record
        M->>C: set(code, record)
    end
    M-->>A: Record
    par 
        A-->>U: 307 Temporary Redirect
    and
        A->>M: record_analytics(id, ip, ua)
        M->>M: Anonymize IP
        M->>D: record_click(...)
        D->>DB: INSERT
    end
```

## Tech Stack

- **Language**: [Rust](https://www.rust-lang.org/) (Edition 2024)
- **Web Framework**: [Axum](https://github.com/tokio-rs/axum)
- **Database**: [PostgreSQL](https://www.postgresql.org/) (SQLx)
- **Caching**: [Redis](https://redis.io/)
- **Async Runtime**: [Tokio](https://tokio.rs/)
- **ID Generation**: [nanoid](https://github.com/p-nerd/nanoid-rs) (8-character short codes)
- **API Documentation**: [Utoipa](https://github.com/juhakivekas/utoipa) (Swagger UI available at `/swagger-ui`)
- **Tracing**: [tracing](https://github.com/tokio-rs/tracing) for structured logging.

## Getting Started

### Prerequisites
- Docker and Docker Compose
- Rust toolchain

### Running the Project
1. Start the infrastructure:
   ```powershell
   docker-compose up -d
   ```
2. Set the environment variables:
   ```powershell
   $env:DATABASE_URL="postgres://postgres:password@localhost:5432/system_design"
   $env:REDIS_URL="redis://localhost:6379/"
   ```
3. Run the application:
   ```powershell
   cargo run
   ```

## Usage

### Shorten a URL
```powershell
curl -X POST http://localhost:3005/shorten `
     -H "Content-Type: application/json" `
     -d '{"url": "https://www.rust-lang.org"}'
```

### Follow a Redirect
Open `http://localhost:3005/{short_code}` in your browser or use:
```powershell
curl -I http://localhost:3005/{short_code}
```

### API Documentation
- **Swagger UI**: `http://localhost:3005/swagger-ui`
- **OpenAPI Spec**: `http://localhost:3005/api-docs/openapi.json`
