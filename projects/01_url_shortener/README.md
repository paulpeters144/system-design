# 01 URL Shortener

## Requirements
- Shorten a long URL to a short link.
- Redirect from short link to original long URL.
- Support high availability and scalability.

## Data Model
- URL Mapping (short_id -> long_url)

## System Design
```mermaid
graph TD
    Start((start)) --> Input[input: longURL]
    Input --> Hash[hash function]
    Hash --> Short[shortURL]
    Short --> Exist{exist in DB?}
    
    Exist -- "yes (has collision)" --> Collision[longURL + predefined string]
    Collision --> Input
    
    Exist -- "no" --> Save[save to DB]
    Save --> End((end))
```

## Implementation Details
- Using Redis for caching/storing hot links.
- Postgres or similar for persistence.
