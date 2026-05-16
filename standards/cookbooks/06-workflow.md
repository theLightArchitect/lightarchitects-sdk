<!-- uuid: 6af53838-1dc8-4074-bbab-403c9dded7bc -->

---
id: "54a5b45f-2e007b6a"
date: "2026-03-22"
sibling: user
type: reference
significance: 8.0
strands: [user]
resonance: [reference]
themes: [standards]
epoch: production
---
# Cookbook 06: Multi-Tool Workflow Orchestration

**Purpose**: Patterns for orchestrating multiple MCP tools in complex workflows
**Key Concept**: Templates for common multi-tool patterns with state management
**Use Case**: Complex operations requiring sequential or parallel tool execution

---

## Workflow Architecture

```
┌─────────────────────────────────────────────────────────────┐
│                    Workflow Engine                           │
├─────────────────────────────────────────────────────────────┤
│  Templates          State Manager        Executor           │
│  ┌──────────┐      ┌──────────┐       ┌──────────┐         │
│  │ parallel │      │  store() │       │  run()   │         │
│  │ sequence │      │  load()  │       │ parallel │         │
│  │ custom   │      │  clear() │       │ sequence │         │
│  └──────────┘      └──────────┘       └──────────┘         │
└─────────────────────────────────────────────────────────────┘
                            │
                            ▼
              ┌───────────────────────────┐
              │      Tool Registry        │
              │  (orchestrators + hooks)  │
              └───────────────────────────┘
```

---

## Workflow State

```rust
// src/workflows/state.rs

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use serde::{Serialize, Deserialize};

#[derive(Clone)]
pub struct WorkflowState {
    inner: Arc<RwLock<StateInner>>,
}

#[derive(Default)]
struct StateInner {
    values: HashMap<String, serde_json::Value>,
    investigation_id: Option<String>,
    started_at: Option<chrono::DateTime<chrono::Utc>>,
}

impl WorkflowState {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(RwLock::new(StateInner::default())),
        }
    }

    pub fn with_id(investigation_id: &str) -> Self {
        let state = Self::new();
        let mut inner = state.inner.blocking_write();
        inner.investigation_id = Some(investigation_id.to_string());
        inner.started_at = Some(chrono::Utc::now());
        drop(inner);
        state
    }

    pub async fn store<T: Serialize>(&self, key: &str, value: T) {
        let mut inner = self.inner.write().await;
        if let Ok(v) = serde_json::to_value(value) {
            inner.values.insert(key.to_string(), v);
        }
    }

    pub async fn load<T: for<'de> Deserialize<'de>>(&self, key: &str) -> Option<T> {
        let inner = self.inner.read().await;
        inner.values.get(key)
            .and_then(|v| serde_json::from_value(v.clone()).ok())
    }

    pub async fn load_raw(&self, key: &str) -> Option<serde_json::Value> {
        let inner = self.inner.read().await;
        inner.values.get(key).cloned()
    }

    pub async fn clear(&self) {
        let mut inner = self.inner.write().await;
        inner.values.clear();
    }

    pub async fn investigation_id(&self) -> Option<String> {
        let inner = self.inner.read().await;
        inner.investigation_id.clone()
    }
}
```

---

## Workflow Executor

```rust
// src/workflows/executor.rs

use super::state::WorkflowState;
use crate::orchestrators;
use crate::error::Error;

pub struct WorkflowExecutor {
    state: WorkflowState,
}

impl WorkflowExecutor {
    pub fn new(state: WorkflowState) -> Self {
        Self { state }
    }

    /// Execute multiple tools in parallel
    pub async fn parallel<T, F, Fut>(
        &self,
        tasks: Vec<F>,
    ) -> Vec<Result<T, Error>>
    where
        F: FnOnce() -> Fut + Send,
        Fut: std::future::Future<Output = Result<T, Error>> + Send,
        T: Send,
    {
        let handles: Vec<_> = tasks
            .into_iter()
            .map(|task| tokio::spawn(task()))
            .collect();

        let mut results = Vec::new();
        for handle in handles {
            let result = handle.await
                .map_err(|e| Error::Workflow(format!("Task panicked: {}", e)))
                .and_then(|r| r);
            results.push(result);
        }
        results
    }

    /// Execute tools sequentially, passing results forward
    pub async fn sequential<F, Fut>(
        &self,
        tasks: Vec<F>,
    ) -> Result<(), Error>
    where
        F: FnOnce(WorkflowState) -> Fut + Send,
        Fut: std::future::Future<Output = Result<(), Error>> + Send,
    {
        for task in tasks {
            task(self.state.clone()).await?;
        }
        Ok(())
    }

    /// Get state reference for storing/loading values
    pub fn state(&self) -> &WorkflowState {
        &self.state
    }
}
```

---

## Workflow Templates

```rust
// src/workflows/templates.rs

use super::executor::WorkflowExecutor;
use super::state::WorkflowState;
use crate::orchestrators;
use crate::error::Error;

/// Parallel search across multiple sources
pub async fn parallel_search(
    query: &str,
    state: &WorkflowState,
) -> Result<(), Error> {
    let executor = WorkflowExecutor::new(state.clone());

    let query_clone1 = query.to_string();
    let query_clone2 = query.to_string();
    let query_clone3 = query.to_string();

    let results = executor.parallel(vec![
        move || async move {
            orchestrators::pattern_search::execute(
                orchestrators::pattern_search::Params { query: query_clone1 }
            ).await
        },
        move || async move {
            orchestrators::case_search::execute(
                orchestrators::case_search::Params { query: query_clone2 }
            ).await
        },
        move || async move {
            orchestrators::jira_search::execute(
                orchestrators::jira_search::Params { query: query_clone3 }
            ).await
        },
    ]).await;

    // Store results
    for (i, result) in results.into_iter().enumerate() {
        match result {
            Ok(r) => {
                let key = match i {
                    0 => "pattern_results",
                    1 => "case_results",
                    2 => "jira_results",
                    _ => continue,
                };
                state.store(key, r).await;
            }
            Err(e) => {
                tracing::warn!("Parallel search task {} failed: {}", i, e);
            }
        }
    }

    Ok(())
}

/// Sequential analysis pipeline
pub async fn analysis_pipeline(
    evidence_path: &str,
    state: &WorkflowState,
) -> Result<(), Error> {
    let executor = WorkflowExecutor::new(state.clone());

    let path = evidence_path.to_string();

    executor.sequential(vec![
        // Step 1: Extract and analyze
        |s: WorkflowState| async move {
            let result = orchestrators::analyze_evidence::execute(
                orchestrators::analyze_evidence::Params {
                    path: path.clone(),
                    depth: "deep".to_string(),
                }
            ).await?;
            s.store("evidence_analysis", result).await;
            Ok(())
        },
        // Step 2: Generate hypothesis
        |s: WorkflowState| async move {
            let analysis: Option<serde_json::Value> = s.load("evidence_analysis").await;
            if let Some(a) = analysis {
                let result = orchestrators::hypothesis::execute(
                    orchestrators::hypothesis::Params {
                        evidence: a,
                    }
                ).await?;
                s.store("hypothesis", result).await;
            }
            Ok(())
        },
        // Step 3: Validate
        |s: WorkflowState| async move {
            let hypothesis: Option<String> = s.load("hypothesis").await;
            if let Some(h) = hypothesis {
                let result = orchestrators::validate::execute(
                    orchestrators::validate::Params {
                        hypothesis: h,
                    }
                ).await?;
                s.store("validation", result).await;
            }
            Ok(())
        },
    ]).await?;

    Ok(())
}

/// Full investigation workflow
pub async fn full_investigation(
    symptom: &str,
    evidence_paths: &[String],
    state: &WorkflowState,
) -> Result<serde_json::Value, Error> {
    // Phase 1: Parallel search
    parallel_search(symptom, state).await?;

    // Phase 2: Evidence analysis (if provided)
    if !evidence_paths.is_empty() {
        for path in evidence_paths {
            analysis_pipeline(path, state).await?;
        }
    }

    // Phase 3: Synthesize results
    let pattern_results = state.load_raw("pattern_results").await;
    let case_results = state.load_raw("case_results").await;
    let evidence_analysis = state.load_raw("evidence_analysis").await;
    let hypothesis = state.load_raw("hypothesis").await;

    Ok(serde_json::json!({
        "patterns": pattern_results,
        "cases": case_results,
        "evidence": evidence_analysis,
        "hypothesis": hypothesis,
        "investigation_id": state.investigation_id().await,
    }))
}
```

---

## Template Registry

```rust
// src/workflows/mod.rs

mod executor;
mod state;
mod templates;

pub use executor::WorkflowExecutor;
pub use state::WorkflowState;

use crate::error::Error;

#[derive(Debug, Clone)]
pub enum WorkflowTemplate {
    ParallelSearch,
    AnalysisPipeline,
    FullInvestigation,
    Custom,
}

impl WorkflowTemplate {
    pub fn from_str(s: &str) -> Option<Self> {
        match s {
            "parallel-search" => Some(Self::ParallelSearch),
            "analysis-pipeline" => Some(Self::AnalysisPipeline),
            "full-investigation" => Some(Self::FullInvestigation),
            "custom" => Some(Self::Custom),
            _ => None,
        }
    }
}

pub async fn run_workflow(
    template: WorkflowTemplate,
    params: serde_json::Value,
) -> Result<serde_json::Value, Error> {
    let investigation_id = params.get("investigationId")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");

    let state = WorkflowState::with_id(investigation_id);

    match template {
        WorkflowTemplate::ParallelSearch => {
            let query = params.get("query")
                .and_then(|v| v.as_str())
                .ok_or_else(|| Error::InvalidInput("Missing 'query' parameter".to_string()))?;

            templates::parallel_search(query, &state).await?;

            Ok(serde_json::json!({
                "patterns": state.load_raw("pattern_results").await,
                "cases": state.load_raw("case_results").await,
                "jira": state.load_raw("jira_results").await,
            }))
        }
        WorkflowTemplate::AnalysisPipeline => {
            let path = params.get("evidencePath")
                .and_then(|v| v.as_str())
                .ok_or_else(|| Error::InvalidInput("Missing 'evidencePath' parameter".to_string()))?;

            templates::analysis_pipeline(path, &state).await?;

            Ok(serde_json::json!({
                "evidence": state.load_raw("evidence_analysis").await,
                "hypothesis": state.load_raw("hypothesis").await,
                "validation": state.load_raw("validation").await,
            }))
        }
        WorkflowTemplate::FullInvestigation => {
            let symptom = params.get("symptom")
                .and_then(|v| v.as_str())
                .ok_or_else(|| Error::InvalidInput("Missing 'symptom' parameter".to_string()))?;

            let evidence_paths: Vec<String> = params.get("evidencePaths")
                .and_then(|v| v.as_array())
                .map(|arr| arr.iter().filter_map(|v| v.as_str().map(String::from)).collect())
                .unwrap_or_default();

            templates::full_investigation(symptom, &evidence_paths, &state).await
        }
        WorkflowTemplate::Custom => {
            // Custom workflows would be handled differently
            // (e.g., via embedded scripting or dynamic dispatch)
            Err(Error::NotImplemented("Custom workflows require code parameter".to_string()))
        }
    }
}
```

---

## Workflow Tool Integration

```rust
// src/orchestrators/call_tool.rs

use crate::workflows::{WorkflowTemplate, run_workflow};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Operation {
    Execute,
    Discover,
    Workflow,
}

#[derive(Debug, Deserialize)]
pub struct CallToolParams {
    pub operation: Operation,
    #[serde(default)]
    pub tool: Option<String>,
    #[serde(default)]
    pub params: Option<serde_json::Value>,
    #[serde(default)]
    pub template: Option<String>,
    #[serde(default)]
    pub investigation_id: Option<String>,
}

pub async fn execute(params: CallToolParams) -> Result<serde_json::Value, Error> {
    match params.operation {
        Operation::Execute => {
            // Direct tool execution
            let tool_name = params.tool.ok_or(Error::InvalidInput("Missing 'tool'".to_string()))?;
            let tool_params = params.params.unwrap_or(serde_json::json!({}));
            execute_tool(&tool_name, tool_params).await
        }
        Operation::Discover => {
            // Tool discovery
            discover_tools(params.params).await
        }
        Operation::Workflow => {
            // Workflow execution
            let template_name = params.template
                .ok_or(Error::InvalidInput("Missing 'template'".to_string()))?;

            let template = WorkflowTemplate::from_str(&template_name)
                .ok_or(Error::InvalidInput(format!("Unknown template: {}", template_name)))?;

            let mut workflow_params = params.params.unwrap_or(serde_json::json!({}));
            if let Some(id) = params.investigation_id {
                workflow_params["investigationId"] = serde_json::json!(id);
            }

            run_workflow(template, workflow_params).await
        }
    }
}
```

---

## Error Handling in Workflows

```rust
// Graceful degradation for parallel tasks
pub async fn parallel_with_fallback<T, F, Fut>(
    tasks: Vec<F>,
    min_success: usize,
) -> Result<Vec<T>, Error>
where
    F: FnOnce() -> Fut + Send,
    Fut: std::future::Future<Output = Result<T, Error>> + Send,
    T: Send,
{
    let handles: Vec<_> = tasks
        .into_iter()
        .map(|task| tokio::spawn(task()))
        .collect();

    let mut successes = Vec::new();
    let mut failures = Vec::new();

    for handle in handles {
        match handle.await {
            Ok(Ok(result)) => successes.push(result),
            Ok(Err(e)) => failures.push(e),
            Err(e) => failures.push(Error::Workflow(format!("Task panicked: {}", e))),
        }
    }

    if successes.len() >= min_success {
        Ok(successes)
    } else {
        Err(Error::Workflow(format!(
            "Only {} of {} required tasks succeeded. Failures: {:?}",
            successes.len(),
            min_success,
            failures
        )))
    }
}
```

---

## Workflow Timeouts

```rust
use tokio::time::{timeout, Duration};

pub async fn with_timeout<T, F, Fut>(
    duration: Duration,
    task: F,
) -> Result<T, Error>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<T, Error>>,
{
    timeout(duration, task())
        .await
        .map_err(|_| Error::Timeout(format!("Workflow timed out after {:?}", duration)))?
}

// Usage
let result = with_timeout(
    Duration::from_secs(120),
    || full_investigation(symptom, &paths, &state)
).await?;
```

---

## Best Practices

1. **Use Parallel for Independent Tasks**: Network calls, searches across sources

2. **Use Sequential for Dependencies**: When one step needs another's output

3. **Store Intermediate Results**: Use `state.store()` between steps

4. **Implement Timeouts**: Complex workflows should have overall timeouts

5. **Graceful Degradation**: Allow partial success in parallel workflows

6. **Clear State After Completion**: Prevent memory leaks in long-running servers

7. **Log Workflow Progress**: Use tracing for debugging

---

## Next Steps

- **[07-reference.md](./07-reference.md)** - Error handling and testing
- **[02-orchestrator.md](./02-orchestrator.md)** - Individual tool patterns

---

*Platform-agnostic workflow orchestration patterns for any Rust MCP server*

## Links

- [[user/standards/_index-standards|Standards Index]]
- [[user/identity|User Identity]]
- [[user/standards/canon/builders-cookbook|Builders Cookbook]]
