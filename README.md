# statsig-client

A Rust client for Statsig's feature flag and experimentation platform. Built for production use with proper error handling, caching, and batch processing.

## Why this client?

A reliable, type-safe Rust client for Statsig designed for client-side applications. Perfect for:

- Desktop and mobile apps
- Edge functions and middleware
- Anywhere you need client-side feature flags

- Works with client keys (like the JS SDK)
- Handles network failures gracefully with retries
- Caches responses to reduce API calls and latency
- Validates inputs before they hit the wire
- Supports batch operations for better performance
- Uses builder patterns for clean, readable code

## Installation

```toml
[dependencies]
statsig-client = "0.1.0"
tokio = { version = "1", features = ["rt-multi-thread", "macros"] }
```

## Getting Started

```rust
use statsig_client::{StatsigClient, User};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create the client with your Statsig client key
    let client = StatsigClient::new("your-client-key").await?;
    
    // Define your user
    let user = User::builder()
        .user_id("user-123")
        .email("user@example.com")
        .country("US")
        .build()?;
    
    // Check if a feature is enabled for this user
    if client.check_gate("new-dashboard", &user).await? {
        println!("User gets the new dashboard!");
    }
    
    Ok(())
}
```

## Configuration

Tweak the client behavior to fit your needs:

```rust
use statsig_client::{StatsigClient, StatsigClientConfig};
use std::time::Duration;

let config = StatsigClientConfig::builder()
    .api_key("your-client-key")
    .timeout(Duration::from_secs(10))           // Request timeout
    .cache_ttl(Duration::from_secs(300))        // Cache for 5 minutes
    .cache_max_capacity(1000)                   // Max cached items
    .retry_attempts(3)                          // Retry failed requests
    .retry_delay(Duration::from_millis(500))    // Delay between retries
    .build();

let client = StatsigClient::with_config(config).await?;
```

## Rich User Context

Add custom data to target your features better:

```rust
let user = User::builder()
    .user_id("user-123")
    .email("user@example.com")
    .country("US")
    .custom([
        ("subscription_plan", serde_json::json!("premium")),
        ("account_age_days", serde_json::json!(45)),
        ("last_login", serde_json::json!("2024-01-15")),
    ])
    .private_attributes([("internal_id", serde_json::json!("internal-123"))])
    .build()?;
```

## Dynamic Configs

Fetch configuration values:

```rust
let config = client.get_config("ui-settings", &user).await?;
let theme = config.get("theme").and_then(|v| v.as_str()).unwrap_or("light");
let max_items = config.get("max_items").and_then(|v| v.as_u64()).unwrap_or(10);

println!("Theme: {}, Max items: {}", theme, max_items);
```

## Event Tracking

Log user actions for analytics:

```rust
// Simple event
client.log_event("button_click", &user).await?;

// Event with metadata
use std::collections::HashMap;
let mut metadata = HashMap::new();
metadata.insert("button_id".to_string(), "submit_form".to_string());
metadata.insert("page".to_string(), "checkout".to_string());

client.log_event_with_metadata("form_submit", &user, metadata).await?;
```

## Batch Operations

Check multiple flags at once to reduce API calls:

```rust
let gates = client.check_gates(vec![
    "new-dashboard".to_string(),
    "beta-features".to_string(),
    "advanced-analytics".to_string(),
], &user).await?;

for (gate_name, enabled) in gates {
    if enabled {
        println!("{} is enabled for this user", gate_name);
    }
}
```

## Error Handling

Things go wrong. Here's how to handle it:

```rust
match client.check_gate("new-feature", &user).await {
    Ok(enabled) => println!("Feature enabled: {}", enabled),
    Err(StatsigError::Network(msg)) => {
        eprintln!("Network issue: {}. Using fallback.", msg);
        // Fall back to default behavior
    },
    Err(StatsigError::Api { status, message }) => {
        eprintln!("Statsig API error {}: {}", status, message);
        // Log the error and continue
    },
    Err(StatsigError::Validation(msg)) => {
        eprintln!("Invalid input: {}", msg);
        // Fix the input and retry
    },
    Err(e) => eprintln!("Unexpected error: {}", e),
}
```

## What Gets Cached?

The client caches responses to reduce latency and API costs:

- Feature gate results (default: 5 minutes)
- Dynamic config values (default: 5 minutes)
- Cache keys include user hash + entity name
- Automatic cache invalidation on errors

## Performance Tips

1. **Batch your checks** - Use `check_gates()` instead of multiple `check_gate()` calls
2. **Reuse the client** - Create one client and share it across your app
3. **Adjust cache TTL** - Longer TTLs reduce API calls but may delay updates
4. **Set appropriate timeouts** - Balance between reliability and responsiveness

## License

MIT OR Apache-2.0 - pick whichever works for you.

## Contributing

Found a bug or want to add something? Open an issue or send a PR.
