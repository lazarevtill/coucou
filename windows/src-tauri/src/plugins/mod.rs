// Plugins: status sources next to the built-in integrations. A plugin is a
// folder in %APPDATA%\Coucou\plugins\<id>\ holding a plugin.json manifest; it
// either polls an HTTP endpoint (declarative, no code) or talks to an MCP
// server. See windows/docs/plugins.md.

pub mod sha256;
pub mod manifest;
pub mod guard;
pub mod http;
pub mod mcp;
pub mod registry;
pub mod runner;
