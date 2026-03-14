# Robots.txt Compliance Implementation Plan

## Project Overview
Implement full `robots.txt` compliance for the Prospect Web Crawler. This will ensure the crawler respects site-specific rules (`Allow`, `Disallow`) and dynamic crawl delays, fulfilling the claims made in the project's documentation and making it a truly polite web citizen.

## Technical Stack
*   **Language:** Rust
*   **Database:** PostgreSQL (SQLx)
*   **HTTP:** Reqwest
*   **New Dependency:** A reliable `robots.txt` parsing crate (e.g., `robotstxt` which binds to Google's standard C++ parser, or a pure Rust crate like `text-robots`).

## Phases of Development

### 1. Research & Discovery
*   Evaluate and select the most appropriate Rust crate for parsing `robots.txt`. The ideal crate should accurately support wildcards, `Crawl-delay`, and specific User-Agent matching.
*   Finalize the caching strategy for `robots.txt` content. Storing it alongside `domain_metrics` in PostgreSQL is the logical approach to avoid redundant HTTP requests, but we must account for in-memory deduplication of concurrent fetches.

### 2. Architecture & Design
*   **Data Model:** Expand the `DomainMetrics` model to include:
    *   `robots_txt_content`: The raw text of the file.
    *   `robots_txt_fetched_at`: Timestamp of the last fetch.
    *   `robots_txt_status`: The HTTP status code returned during the fetch (crucial for handling 403s/404s).
*   **Control Flow:** The `AppManager` will intercept the crawl flow before fetching a page. It must check the cache for the domain's `robots.txt`, fetch it if missing or stale (e.g., > 24 hours old), parse the rules against the target URL and our `LeadBot` User-Agent, and make a go/no-go decision.

### 3. Implementation Steps
1.  **Add Dependency:** Update `Cargo.toml` with the chosen `robots.txt` parser crate.
2.  **Database Migration:**
    *   Create a new SQLx migration file to add `robots_txt_content` (TEXT, nullable), `robots_txt_fetched_at` (TIMESTAMPTZ, nullable), and `robots_txt_status` (INT, nullable) to the `domain_metrics` table.
    *   Add a `blocked` variant to the `crawl_status` ENUM to clearly differentiate policy rejections from network failures.
    *   Update the `DomainMetrics` struct in `src/repository/models.rs` to reflect the new columns.
3.  **Repository Updates:**
    *   Update the SQL queries in `src/repository/traits.rs` (`get_domain_metrics`, `upsert_domain_metrics`) to handle the new fields.
4.  **AppManager Integration (The Core Logic):**
    *   Modify `AppManager::can_crawl` to accept the full `url` instead of just the `domain`.
    *   Implement caching logic with explicit HTTP status handling:
        *   `404/410`: Treat as Allow All.
        *   `401/403`: Treat as Disallow All.
        *   `5xx`: Treat as Disallow temporarily or apply a strict default crawl delay.
    *   If a fetch is needed, download it using the `http_client`, and update the `domain_metrics`.
    *   Parse the cached or newly fetched `robots.txt` content.
    *   Check if the target URL's path is allowed for our specific User-Agent (`LeadBot/0.1.0`).
    *   If a `Crawl-delay` directive exists for our agent (or `*`), dynamically update the domain's `crawl_delay_ms` in the database.
5.  **Frontier Rejection Handling:**
    *   If `AppManager::can_crawl` determines a URL is disallowed, mark the URL hash as `blocked` in the frontier repository, preventing future attempts and preserving clean metrics.

### 4. Testing & Quality Assurance
*   **Unit Tests:** Add unit tests to `AppManager` specifically testing the `robots.txt` evaluation logic (e.g., simulating `Allow`, `Disallow`, and `Crawl-delay` rules).
*   **Integration Tests:** Update existing tests in `tests/logic_tests.rs` to mock the HTTP responses for `/robots.txt` (including 404s and 500s) and ensure the crawler respects the mocked rules.

## Potential Challenges
*   **The Thundering Herd Problem:** If multiple workers encounter a new domain simultaneously, they might all try to fetch the `robots.txt` at the same time. We need an in-memory lock (e.g., `moka` cache or `tokio::sync::OnceCell`) or a robust `SELECT FOR UPDATE` mechanism to coalesce these requests.
*   **Caching Failures:** We must briefly cache 5xx server errors so we don't spam a failing server requesting the file on every single URL check.
*   **Malformed Files:** The parser must be robust against syntax errors in wild `robots.txt` files on the internet. If unparseable, default to standard politeness.
