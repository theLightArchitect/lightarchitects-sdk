//! Setup / backend-selection API handlers.
//!
//! Four routes manage the first-run setup flow and live backend switching:
//! - `GET /api/setup/info` — auth detection + setup state (unauthenticated)
//! - `GET /api/setup/models` — available models for a given backend
//! - `POST /api/setup/save` — persist config + hot-reload active agent (authenticated)
//! - `DELETE /api/setup/reset` — wipe config, trigger re-setup (authenticated)

pub mod handlers;

pub use handlers::{
    DeepSeekAuthStatus, GoogleVertexAuthStatus, ModelOption, OllamaCloudAuthStatus, setup_info,
    setup_models, setup_reset, setup_save,
};
