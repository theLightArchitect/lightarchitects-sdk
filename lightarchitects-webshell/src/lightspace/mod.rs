//! Lightspace integration layer for the webshell.
//!
//! Each module is a single-responsibility unit; compose them in route handlers.

pub mod empty_state;
pub mod hmac_seed;
pub mod injection_shield;
pub mod path_safety;
pub mod persist;
pub mod retention;
pub mod session_registry;
pub mod uri_scheme_acl;
