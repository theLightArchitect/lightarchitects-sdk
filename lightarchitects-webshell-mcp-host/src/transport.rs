//! MCP transport wiring: wraps a spawned `Command` as an rmcp client.

use rmcp::{
    service::{RoleClient, RunningService, serve_client_with_ct},
    transport::child_process::TokioChildProcess,
};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use crate::McpHostError;

/// Spawn `cmd` as an MCP subprocess and perform the MCP initialize handshake.
///
/// Returns a live `RunningService` whose `peer()` can be used for `tools/list`
/// and `tools/call`. The service is tied to `ct` — cancelling the token tears
/// down the connection.
pub async fn connect(
    cmd: Command,
    server_name: &str,
    ct: CancellationToken,
) -> Result<RunningService<RoleClient, ()>, McpHostError> {
    let transport = TokioChildProcess::new(cmd).map_err(|e| McpHostError::Initialize {
        name: server_name.to_owned(),
        reason: e.to_string(),
    })?;

    serve_client_with_ct((), transport, ct)
        .await
        .map_err(|e| McpHostError::Initialize {
            name: server_name.to_owned(),
            reason: e.to_string(),
        })
}
