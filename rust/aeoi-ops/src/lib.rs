//! aeoi-ops — every AEOI operator, across domains. Front-ends serve this registry; each domain crate
//! (aeoi-control, aeoi-numeric, …) keeps its own `registry()` for its own tests.
use aeoi_core::DynOperator;

/// All operators: control first (their order is unchanged), then the other domains.
pub fn registry() -> Vec<Box<dyn DynOperator>> {
    let mut r = aeoi_control::registry();
    r.extend(aeoi_numeric::registry());
    r
}

/// Look up by operator id or MCP tool name.
pub fn find<'a>(reg: &'a [Box<dyn DynOperator>], id: &str) -> Option<&'a dyn DynOperator> {
    reg.iter().find(|o| o.meta().id == id || o.meta().manifest["mcp"]["tool_name"] == id).map(|b| b.as_ref())
}
