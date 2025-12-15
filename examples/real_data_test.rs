use statsig_client::{StatsigClient, StatsigClientConfig, StatsigEvent, StatsigEventValue, User};
use std::collections::HashMap;
use std::time::Duration;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let api_key = std::env::var("STATSIG_API_KEY")
        .unwrap_or_else(|_| "client-RvCu85ZSfYnKnbZgBJ2qffMX1yJObB77TJ7Jb7cKLcc".to_string());

    // Custom configuration
    let config = StatsigClientConfig::builder()
        .api_key(api_key)
        .timeout(Duration::from_secs(10))
        .cache_ttl(Duration::from_secs(300))
        .retry_attempts(3)
        .build();

    let client = StatsigClient::with_config(config).await?;

    // Rich user context
    let user = User::builder()
        .user_id("123")
        .email("user@example.com")
        .country("US")
        .custom([
            ("subscription_plan", serde_json::json!("premium")),
            ("account_age_days", serde_json::json!(45)),
        ])
        .custom_ids([("stable_id", "stable-123")])
        .build()?;

    // Single gate check
    let gate_result = client.check_gate("test_gate", &user).await?;
    println!("test_gate: {}", gate_result);

    // Batch gate checks
    let gates = client
        .check_gates(
            vec!["test_gate".to_string(), "feature_flag".to_string()],
            &user,
        )
        .await?;

    for (name, enabled) in gates {
        println!("{}: {}", name, enabled);
    }

    // Dynamic config
    let config = client.get_config("test_config", &user).await?;
    println!("test_config: {}", serde_json::to_string_pretty(&config)?);

    // Config evaluation details
    let eval = client.get_config_evaluation("test_config", &user).await?;
    println!("Evaluation: {} -> {:?}", eval.name, eval.group);

    // Event logging
    client.log_event("page_view", &user).await?;

    let mut metadata = HashMap::new();
    metadata.insert("product_id".to_string(), "prod_456".to_string());

    client
        .log_events(
            vec![
                StatsigEvent::builder()
                    .event_name("add_to_cart")
                    .value(StatsigEventValue::Number(29.99))
                    .metadata(metadata)
                    .build(),
            ],
            &user,
        )
        .await?;

    println!("Events logged successfully");

    Ok(())
}
