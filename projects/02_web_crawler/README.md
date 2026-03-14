# 02 Web Crawler

A high-performance, modular web crawler built with Rust, designed for lead generation and automated discovery with a focus on politeness and intelligent scoring.

## Architecture

This project employs a highly modular architecture using traits to allow for pluggable crawl, extraction, and scoring strategies:

1.  **Engine Layer (`AppManager`)**: The central orchestrator that coordinates the crawl lifecycle. It manages the flow from URL selection to fetching, extraction, scoring, and persistence.
2.  **Strategy Engines**:
    *   **Crawl Engine**: Determines URL selection priority (e.g., `LeadFocusedEngine` vs. `DiscoveryEngine`).
    *   **Extraction Engine**: Parses HTML to find leads and new links (e.g., `RegexExtractor` vs. `SelectorExtractor`).
    *   **Scoring Engine**: Evaluates the quality of extracted data (e.g., `WealthIntentScorer` vs. `ProfessionalReferralScorer`).
3.  **Frontier Management**:
    *   **Frontier**: An in-memory/database hybrid queue that uses a **Bloom Filter** for high-efficiency URL deduplication before hitting the database.
    *   **Politeness**: Integrated checks against domain-specific metrics to ensure crawl delays are respected.
4.  **Data Access Layer (`repository`)**:
    *   **PostgreSQL**: Stores the URL frontier, extracted leads, and domain metrics using SQLx.

## System Architecture

The crawler uses a layered approach with trait-based dependency injection to decouple business logic from infrastructure.

```mermaid
graph TD
    subgraph L1 [CLI Layer]
        C[Command Parser]
    end

    subgraph L2 [Orchestration Layer]
        M[App Manager]
        F[Frontier <br> Bloom Filter]
        M --> F
    end

    subgraph L3 [Engine Layer]
        direction LR
        CE[Crawl<br>Engine] ~~~ EE[Extraction<br>Engine] ~~~ SE[Scoring<br>Engine]
    end

    subgraph L4 [Access Layer]
        direction LR
        R[(Data Repository)]
        H[HTTP Client]
    end

    %% Pyramid Connections
    C --> M
    M --> L3
    M --> L4

    %% Data Flow
    F -- Warm-up --> R
    W((Internet/Websites))
    H <-->|Fetch HTML| W

    %% Styling
    style L1 fill:#f9f9f9,stroke:#333
    style L2 fill:#e1f5fe,stroke:#01579b
    style L3 fill:#fff3e0,stroke:#e65100
    style L4 fill:#f1f8e9,stroke:#33691e
```

#### Crawl Loop Flow
The crawler operates in an iterative loop, processing batches of URLs while respecting domain-specific politeness constraints.
```mermaid
sequenceDiagram
    participant M as AppManager
    participant F as Frontier
    participant H as HttpClient
    participant E as Extraction
    participant S as Scoring
    participant R as Repository

    loop Batch Cycle
        M->>M: select_batch(limit)
        loop for each URL
            M->>R: can_crawl(domain)?
            R-->>M: status
            alt allowed
                M->>H: GET url
                H-->>M: HTML
                M->>E: extract(html)
                E-->>M: raw_leads, links
                loop for each raw_lead
                    M->>S: score(raw_lead)
                    S-->>M: score
                    M->>R: upsert_lead(lead)
                end
                loop for each link
                    M->>F: contains(link_hash)?
                    F-->>M: false
                    M->>F: add(link_hash)
                    M->>R: add_to_frontier(link)
                end
                M->>R: mark_completed(url_hash)
                M->>R: update_metrics(domain)
            end
        end
    end
```

## Tech Stack

- **Language**: [Rust](https://www.rust-lang.org/) (Edition 2024)
- **Database**: [PostgreSQL](https://www.postgresql.org/) (SQLx)
- **Async Runtime**: [Tokio](https://tokio.rs/)
- **HTTP Client**: [Reqwest](https://github.com/seanmonstar/reqwest)
- **HTML Parsing**: [Scraper](https://github.com/causal-agent/scraper)
- **Deduplication**: [Bloom Filter](https://github.com/crepererum/bloomfilter-rs)
- **CLI Framework**: [Argh](https://github.com/google/argh)
- **Logging**: [Tracing](https://github.com/tokio-rs/tracing)

## Getting Started

### Prerequisites
- Docker and Docker Compose
- Rust toolchain (v1.85+)

### Running the Project
1. Start the database:
   ```powershell
   docker-compose up -d
   ```
2. Set the database URL:
   ```powershell
   $env:DATABASE_URL="postgres://postgres:password@localhost:5433/web_crawler"
   ```
3. Seed the crawler with a starting URL:
   ```powershell
   cargo run -- seed "https://news.ycombinator.com"
   ```
4. Run the crawler:
   ```powershell
   cargo run -- crawl --batch 10 --delay 5
   ```

## Usage

The crawler supports several subcommands via the CLI:

### Seed URLs
Add entry points to the crawl frontier:
```powershell
cargo run -- seed "https://example.com" --priority 5
```

### Execute Crawl
Start the worker loop with specific engines:
```powershell
# Options: --engine [lead|discovery], --extractor [regex|selector], --scorer [wealth|referral]
cargo run -- crawl --engine lead --extractor regex --scorer wealth --batch 5
```

### View Discovered Leads
Query the database for highly-scored leads:
```powershell
cargo run -- leads --limit 50
```
