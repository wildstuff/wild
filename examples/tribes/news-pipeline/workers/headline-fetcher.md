---
worker_name: headline-fetcher
component_type: http-fetcher
config:
  url: "https://hn.algolia.com/api/v1/search?tags=front_page&hitsPerPage=15"
  timeout_ms: 5000
---
