# lightarchitects-eva

Typed Rust client for [EVA](https://github.com/TheLightArchitects/EVA)'s `evaTools` MCP orchestrator.

EVA exposes a single MCP tool (`evaTools`) with 8 actions: `visualize`, `ideate`, `memory`, `build`, `research`, `bible`, `secure`, and `teach`. This crate provides both typed methods per action and a generic adapter for dynamic dispatch.

## Quick Start

```rust
use lightarchitects_eva::{EvaClient, TeachMode, SkillLevel, BuildMode};

#[tokio::main]
async fn main() -> Result<(), lightarchitects_core::SdkError> {
    let client = EvaClient::builder().build().await?;

    // Typed method: teach a concept
    let lesson = client
        .teach(TeachMode::Explain, "lifetimes in Rust", SkillLevel::Intermediate)
        .await?;
    println!("{}", lesson.output);

    // Typed method: code review
    let review = client
        .build(BuildMode::Review, Some("fn foo() { panic!() }"), Some("rust"))
        .await?;
    println!("{}", review.output);

    // Generic adapter: call any action by name
    let params = serde_json::json!({ "goal": "design a plugin system" });
    let out = client.action("ideate", params).await?;
    println!("{}", out.output);

    Ok(())
}
```

## Two Call Paths

| Path | Use When |
|------|----------|
| **Typed methods** — `client.teach()`, `client.build()`, etc. | Action is known at compile time; prefer type safety |
| **Generic adapter** — `client.action(name, params)` | Action determined at runtime; building higher-level orchestration |

## Actions

`visualize` · `ideate` · `memory` · `build` · `research` · `bible` · `secure` · `teach`
