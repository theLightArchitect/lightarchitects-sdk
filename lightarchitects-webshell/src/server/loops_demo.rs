//! `GET /api/loops/demo` — live SSE stream of a 3-step `ReAct` loop via llama3.2:3b.
//! `GET /loop-demo`      — HTML viewer page.
//! `GET /loop-demo.js`   — viewer JavaScript (served from `'self'` — CSP-safe).
//!
//! No authentication required. Local demo endpoint only.

use std::{convert::Infallible, time::Instant};

use async_trait::async_trait;
use axum::{
    http::{StatusCode, header},
    response::{
        IntoResponse, Response,
        sse::{Event, KeepAlive, Sse},
    },
};
use futures_util::{StreamExt as _, stream};
use serde_json::json;
use tokio::time::Duration;

use lightarchitects::agent::{
    AgentRequest, ChainContext, LlmAgentProvider, OllamaCliProvider, ProviderEvent,
    loops::{
        Budget, LoopRunner, Outcome,
        error::LoopError,
        react::{ReActExecutor, ReActPhase, ReActPrompt, ReActStep, ReActStrategy},
        runner::StepContext,
    },
};

// ── OllamaReActExecutor ───────────────────────────────────────────────────────

struct OllamaReActExecutor {
    provider: OllamaCliProvider,
}

#[async_trait]
impl ReActExecutor for OllamaReActExecutor {
    async fn step(&self, prompt: &ReActPrompt, _ctx: &StepContext) -> Result<ReActStep, LoopError> {
        let system = "You are a concise reasoning agent. Respond in exactly this format:\n\
                      Thought: <one sentence reasoning>\n\
                      Action: <one concrete next action>\n\
                      Result: <expected outcome or observation>";

        let history = prompt
            .steps
            .iter()
            .enumerate()
            .map(|(i, s)| {
                format!(
                    "Step {}: Thought: {} | Action: {} | Result: {}",
                    i + 1,
                    s.thought,
                    s.action,
                    s.result.as_deref().unwrap_or("—")
                )
            })
            .collect::<Vec<_>>()
            .join("\n");

        let user_msg = format!(
            "Task: {}\nContext: {}{}",
            prompt.query,
            prompt.context,
            if history.is_empty() {
                String::new()
            } else {
                format!("\n\nPrevious steps:\n{history}")
            }
        );

        let step_num = prompt.steps.len() + 1;
        tracing::debug!(step = step_num, "ReAct demo step");

        let req = AgentRequest {
            sibling_identity: system.to_owned(),
            user_prompt: user_msg,
            schema: None,
            allowed_tools: vec![],
            max_turns: 1,
            max_budget_usd: 0.0,
            model_hint: None,
            parent_span_id: None,
            chain_origin: None,
            chain_depth: 0,
            aud: None,
            conversation_history: vec![],
            tool_definitions: vec![],
        };

        let sanitized = req
            .sanitize()
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;

        let mut token_stream = self
            .provider
            .spawn_streaming(sanitized)
            .await
            .map_err(|e| LoopError::StepFailed(e.to_string()))?;

        let mut full_text = String::new();
        while let Some(event) = token_stream.next().await {
            if let ProviderEvent::TextDelta { text, .. } = event {
                full_text.push_str(&text);
            }
        }

        let thought = extract_field(&full_text, "Thought").unwrap_or_else(|| {
            full_text
                .lines()
                .next()
                .unwrap_or("(no thought)")
                .to_owned()
        });
        let action = extract_field(&full_text, "Action").unwrap_or_else(|| "continue".to_owned());
        let result = extract_field(&full_text, "Result");

        let next_phase = if step_num >= prompt.max_steps.saturating_sub(1) {
            ReActPhase::Close
        } else {
            prompt.phase.next()
        };

        Ok(ReActStep {
            observation: result.clone().unwrap_or_default(),
            thought,
            action,
            result,
            phase: next_phase,
        })
    }
}

fn extract_field(text: &str, field: &str) -> Option<String> {
    let prefix = format!("{field}:");
    text.lines()
        .find(|l| l.trim_start().starts_with(&prefix))
        .map(|l| l.trim_start().trim_start_matches(&prefix).trim().to_owned())
        .filter(|s| !s.is_empty())
}

// ── GET /api/loops/demo ───────────────────────────────────────────────────────

/// Live SSE stream. Events: `start`, `step` (×N), `halt` | `error`.
///
/// Each event `data` is a JSON object. No auth required.
pub async fn demo_sse_handler() -> Response {
    let model = std::env::var("LOOP_DEMO_MODEL")
        .ok()
        .filter(|s| !s.is_empty())
        .unwrap_or_else(|| "llama3.2:3b".to_owned());

    let provider = OllamaCliProvider::new_local(&model, None);
    let executor = OllamaReActExecutor { provider };
    let strategy = ReActStrategy::new(executor).with_name("demo-react");
    let task = ReActPrompt::new(
        "What are 3 key benefits of Rust's ownership model for systems programming?",
        3,
    );

    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Event, Infallible>>(16);

    let query = task.query.clone();
    let max_steps = task.max_steps;
    let model_name = model.clone();

    tokio::spawn(async move {
        let start = Instant::now();

        let _ = tx
            .send(Ok(Event::default().event("start").data(
                json!({
                    "type": "start",
                    "query": query,
                    "model": model_name,
                    "max_steps": max_steps,
                })
                .to_string(),
            )))
            .await;

        let mut loop_stream =
            LoopRunner::new(strategy, Budget::unlimited()).run(task, ChainContext::default(), None);

        while let Some(result) = loop_stream.next().await {
            match result {
                Ok(step) => match step.outcome {
                    Outcome::Continue(state) => {
                        if let Some(last) = state.steps.last() {
                            let payload = json!({
                                "type": "step",
                                "step_num": state.steps.len(),
                                "phase": format!("{:?}", last.phase),
                                "thought": last.thought,
                                "action": last.action,
                                "result": last.result,
                                "elapsed_ms": start.elapsed().as_millis(),
                            });
                            let _ = tx
                                .send(Ok(Event::default().event("step").data(payload.to_string())))
                                .await;
                        }
                    }
                    Outcome::Halt(state) => {
                        let payload = json!({
                            "type": "halt",
                            "steps_completed": state.steps.len(),
                            "elapsed_ms": start.elapsed().as_millis(),
                        });
                        let _ = tx
                            .send(Ok(Event::default().event("halt").data(payload.to_string())))
                            .await;
                        break;
                    }
                    Outcome::Pause(_, _) => {
                        let _ = tx
                            .send(Ok(Event::default().event("error").data(
                                r#"{"type":"error","message":"unexpected Pause outcome"}"#,
                            )))
                            .await;
                        break;
                    }
                },
                Err(e) => {
                    let payload = json!({"type": "error", "message": e.to_string()});
                    let _ = tx
                        .send(Ok(Event::default()
                            .event("error")
                            .data(payload.to_string())))
                        .await;
                    break;
                }
            }
        }
    });

    let event_stream = stream::unfold(
        rx,
        |mut rx| async move { rx.recv().await.map(|ev| (ev, rx)) },
    );

    Sse::new(event_stream)
        .keep_alive(
            KeepAlive::new()
                .interval(Duration::from_secs(15))
                .text("keep-alive"),
        )
        .into_response()
}

// ── GET /loop-demo ────────────────────────────────────────────────────────────

/// Serves the HTML viewer page for the loop demo.
pub async fn demo_page_handler() -> Response {
    (
        StatusCode::OK,
        [(header::CONTENT_TYPE, "text/html; charset=utf-8")],
        HTML_PAGE,
    )
        .into_response()
}

/// Serves the viewer JavaScript (CSP-safe: `'self'` origin).
pub async fn demo_js_handler() -> Response {
    (
        StatusCode::OK,
        [(
            header::CONTENT_TYPE,
            "application/javascript; charset=utf-8",
        )],
        VIEWER_JS,
    )
        .into_response()
}

// ── Static assets ─────────────────────────────────────────────────────────────

static HTML_PAGE: &str = r#"<!DOCTYPE html>
<html lang="en">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<title>LA Loop Engine · ReAct Demo</title>
<style>
  :root {
    --bg: #0d0f14; --surface: #13161d; --border: #1e2330;
    --accent: #4a9eff; --green: #3ecf8e; --yellow: #f5a623;
    --dim: #4a5568; --text: #c9d1d9; --bright: #e6edf3; --label: #8b949e;
    --thought: #7dd3fc; --action-c: #86efac; --result-c: #fcd34d;
    --halt: #a78bfa; --error: #ff5f5f;
    --font: 'SF Mono','Fira Code','JetBrains Mono',monospace;
  }
  * { box-sizing: border-box; margin: 0; padding: 0; }
  body { background: var(--bg); color: var(--text); font-family: var(--font);
         font-size: 13px; line-height: 1.6; min-height: 100vh; padding: 24px; }

  .chrome { max-width: 860px; margin: 0 auto; border: 1px solid var(--border);
             border-radius: 10px; overflow: hidden;
             box-shadow: 0 20px 60px rgba(0,0,0,0.6); }
  .title-bar { background: var(--surface); border-bottom: 1px solid var(--border);
               padding: 10px 16px; display: flex; align-items: center; gap: 12px; }
  .dots { display: flex; gap: 6px; }
  .dot { width: 12px; height: 12px; border-radius: 50%; }
  .dot.r { background: #ff5f57; } .dot.y { background: #febc2e; } .dot.g { background: #28c840; }
  .title-text { flex: 1; text-align: center; color: var(--label); font-size: 11px; letter-spacing: .05em; }
  .badge { font-size: 10px; background: rgba(74,158,255,.15); color: var(--accent);
           border: 1px solid rgba(74,158,255,.3); border-radius: 4px; padding: 2px 6px; }
  .badge.live { background: rgba(62,207,142,.15); color: var(--green);
                border-color: rgba(62,207,142,.3); animation: pulse 1.5s infinite; }
  @keyframes pulse { 0%,100%{opacity:1} 50%{opacity:.6} }

  .body { background: var(--bg); padding: 20px 24px; min-height: 480px; }
  .controls { display: flex; gap: 10px; margin-bottom: 18px; align-items: center; }
  button { background: var(--surface); border: 1px solid var(--border); color: var(--text);
           font-family: var(--font); font-size: 11px; border-radius: 5px;
           padding: 6px 14px; cursor: pointer; letter-spacing: .04em;
           transition: border-color .15s, color .15s; }
  button:hover { border-color: var(--accent); color: var(--accent); }
  button:disabled { opacity: .4; cursor: default; border-color: var(--border); color: var(--dim); }
  .status-pill { font-size: 10px; padding: 3px 8px; border-radius: 10px; margin-left: auto;
                 border: 1px solid var(--border); color: var(--label); }
  .status-pill.running { border-color: rgba(62,207,142,.4); color: var(--green); }
  .status-pill.done    { border-color: rgba(167,139,250,.4); color: var(--halt); }
  .status-pill.error   { border-color: rgba(255,95,95,.4);  color: var(--error); }

  .meta { display: flex; gap: 20px; margin-bottom: 18px; padding: 10px 14px;
          background: var(--surface); border: 1px solid var(--border); border-radius: 6px; }
  .meta-item { display: flex; flex-direction: column; gap: 2px; }
  .meta-label { font-size: 9px; color: var(--label); text-transform: uppercase; letter-spacing: .1em; }
  .meta-value { font-size: 12px; color: var(--bright); }
  .meta-value.blue { color: var(--accent); }

  #feed { min-height: 200px; }
  .step-block { margin-bottom: 18px; animation: fadeIn .25s ease; }
  @keyframes fadeIn { from{opacity:0;transform:translateY(4px)} to{opacity:1;transform:translateY(0)} }
  .step-hdr { display: flex; align-items: center; gap: 10px; margin-bottom: 7px;
              padding: 5px 10px; background: var(--surface);
              border-left: 2px solid var(--accent); border-radius: 0 4px 4px 0; }
  .step-num { font-size: 10px; color: var(--accent); font-weight: 700; letter-spacing: .08em; }
  .step-phase { font-size: 9px; color: var(--dim); letter-spacing: .1em;
                text-transform: uppercase; margin-left: auto; }
  .step-ms { font-size: 9px; color: var(--dim); }
  .field { padding: 3px 0 3px 14px; border-left: 1px solid var(--border);
           display: flex; gap: 8px; align-items: baseline; }
  .fk { font-size: 10px; font-weight: 700; min-width: 56px; text-transform: uppercase;
        letter-spacing: .05em; }
  .fk.t { color: var(--thought); } .fk.a { color: var(--action-c); } .fk.r { color: var(--result-c); }
  .fv { flex: 1; font-size: 12px; }
  .fv.t { color: var(--bright); }

  .halt-block { margin-top: 20px; padding: 13px 16px;
                background: rgba(167,139,250,.08); border: 1px solid rgba(167,139,250,.25);
                border-radius: 6px; }
  .halt-title { font-size: 12px; color: var(--halt); font-weight: 700; margin-bottom: 8px; }
  .halt-row { display: flex; gap: 20px; }
  .halt-kv { display: flex; flex-direction: column; gap: 2px; }
  .halt-kv .k { font-size: 9px; color: var(--label); text-transform: uppercase; letter-spacing: .1em; }
  .halt-kv .v { font-size: 12px; color: var(--bright); }

  .error-block { padding: 10px 14px; background: rgba(255,95,95,.08);
                 border: 1px solid rgba(255,95,95,.3); border-radius: 6px;
                 color: var(--error); font-size: 12px; margin-top: 16px; }

  .waiting { color: var(--dim); font-size: 12px; display: flex; align-items: center; gap: 8px; }
  .spinner { display: inline-block; width: 12px; height: 12px; border: 2px solid var(--dim);
             border-top-color: var(--accent); border-radius: 50%; animation: spin .7s linear infinite; }
  @keyframes spin { to{transform:rotate(360deg)} }
</style>
</head>
<body>
<div class="chrome">
  <div class="title-bar">
    <div class="dots"><div class="dot r"></div><div class="dot y"></div><div class="dot g"></div></div>
    <div class="title-text">LA Loop Engine &mdash; LoopRunner&lt;ReActStrategy&lt;OllamaReActExecutor&gt;&gt;</div>
    <div id="live-badge" class="badge">READY</div>
  </div>
  <div class="body">
    <div class="controls">
      <button id="run-btn">&#9654; Run Demo</button>
      <button id="clear-btn" disabled>Clear</button>
      <span id="status-pill" class="status-pill">idle</span>
    </div>
    <div class="meta">
      <div class="meta-item"><span class="meta-label">Model</span><span class="meta-value blue" id="meta-model">llama3.2:3b</span></div>
      <div class="meta-item"><span class="meta-label">Strategy</span><span class="meta-value">ReActStrategy</span></div>
      <div class="meta-item"><span class="meta-label">Provider</span><span class="meta-value">OllamaCliProvider::new_local()</span></div>
      <div class="meta-item"><span class="meta-label">Budget</span><span class="meta-value">Budget::unlimited()</span></div>
      <div class="meta-item"><span class="meta-label">Elapsed</span><span class="meta-value" id="meta-elapsed">—</span></div>
    </div>
    <div id="feed"><div class="waiting"><span class="spinner" id="idle-spinner" style="display:none"></span><span id="feed-msg">Click Run Demo to start.</span></div></div>
  </div>
</div>
<script src="/loop-demo.js"></script>
</body>
</html>"#;

static VIEWER_JS: &str = r#"
/* LA Loop Engine — ReAct demo SSE client */

var currentSource = null;

function el(id) { return document.getElementById(id); }

function startDemo() {
  clearFeed();
  el('run-btn').disabled = true;
  el('clear-btn').disabled = false;
  setStatus('running', 'connecting…');
  el('live-badge').textContent = 'LIVE';
  el('live-badge').className = 'badge live';
  el('idle-spinner').style.display = 'inline-block';
  el('feed-msg').textContent = 'Connecting to Ollama…';

  if (currentSource) { currentSource.close(); }
  currentSource = new EventSource('/api/loops/demo');

  currentSource.addEventListener('start', function(e) {
    var d = JSON.parse(e.data);
    el('meta-model').textContent = d.model;
    el('feed').innerHTML = '';
    setStatus('running', 'running…');
    el('feed-msg') && (el('feed-msg').textContent = '');
  });

  currentSource.addEventListener('step', function(e) {
    var d = JSON.parse(e.data);
    el('meta-elapsed').textContent = (d.elapsed_ms / 1000).toFixed(1) + 's';
    renderStep(d);
  });

  currentSource.addEventListener('halt', function(e) {
    var d = JSON.parse(e.data);
    el('meta-elapsed').textContent = (d.elapsed_ms / 1000).toFixed(1) + 's';
    renderHalt(d);
    setStatus('done', 'done');
    el('live-badge').textContent = 'DONE';
    el('live-badge').className = 'badge';
    el('run-btn').disabled = false;
    currentSource.close();
  });

  currentSource.addEventListener('error', function(e) {
    var msg = 'stream error';
    try { msg = JSON.parse(e.data).message || msg; } catch(_) {}
    renderError(msg);
    setStatus('error', 'error');
    el('live-badge').textContent = 'ERR';
    el('live-badge').className = 'badge';
    el('run-btn').disabled = false;
    currentSource.close();
  });

  currentSource.onerror = function() {
    if (currentSource.readyState === EventSource.CLOSED) return;
    renderError('Connection lost. Is Ollama running at localhost:11434?');
    setStatus('error', 'error');
    el('run-btn').disabled = false;
    el('live-badge').textContent = 'ERR';
    el('live-badge').className = 'badge';
  };
}

function renderStep(d) {
  var wrap = document.createElement('div');
  wrap.className = 'step-block';
  wrap.innerHTML = '<div class="step-hdr">'
    + '<span class="step-num">STEP ' + d.step_num + ' / 3</span>'
    + '<span>ReActPhase::' + esc(d.phase) + '</span>'
    + '<span class="step-ms">' + (d.elapsed_ms/1000).toFixed(1) + 's</span>'
    + '<span class="step-phase">' + (d.step_num < 3 ? '&#8594; next' : '&#8594; Halt') + '</span>'
    + '</div>'
    + field('t', 'Thought', d.thought)
    + field('a', 'Action',  d.action)
    + field('r', 'Result',  d.result || '—');
  el('feed').appendChild(wrap);
}

function field(cls, label, val) {
  return '<div class="field"><span class="fk ' + cls + '">' + label + '</span>'
       + '<span class="fv ' + cls + '">' + esc(val) + '</span></div>';
}

function renderHalt(d) {
  var wrap = document.createElement('div');
  wrap.className = 'halt-block';
  wrap.innerHTML = '<div class="halt-title">&#9632; Outcome::Halt</div>'
    + '<div class="halt-row">'
    + kv('Steps completed', d.steps_completed + ' / 3')
    + kv('Elapsed', (d.elapsed_ms/1000).toFixed(2) + 's')
    + kv('Halt reason', 'max_steps reached (phase → Close)')
    + '</div>';
  el('feed').appendChild(wrap);
}

function kv(k, v) {
  return '<div class="halt-kv"><span class="k">' + k + '</span><span class="v">' + esc(v) + '</span></div>';
}

function renderError(msg) {
  var wrap = document.createElement('div');
  wrap.className = 'error-block';
  wrap.textContent = '⚠ ' + msg;
  el('feed').appendChild(wrap);
}

function setStatus(cls, text) {
  var p = el('status-pill');
  p.className = 'status-pill ' + cls;
  p.textContent = text;
}

function clearFeed() {
  if (currentSource) { currentSource.close(); currentSource = null; }
  el('feed').innerHTML = '<div class="waiting"><span id="idle-spinner" style="display:none"></span><span id="feed-msg">Click Run Demo to start.</span></div>';
  el('run-btn').disabled = false;
  el('clear-btn').disabled = true;
  el('status-pill').className = 'status-pill';
  el('status-pill').textContent = 'idle';
  el('live-badge').textContent = 'READY';
  el('live-badge').className = 'badge';
  el('meta-elapsed').textContent = '—';
}

function esc(s) {
  if (!s) return '';
  return String(s).replace(/&/g,'&amp;').replace(/</g,'&lt;').replace(/>/g,'&gt;');
}

document.addEventListener('DOMContentLoaded', function() {
  document.getElementById('run-btn').addEventListener('click', startDemo);
  document.getElementById('clear-btn').addEventListener('click', clearFeed);
});
"#;
