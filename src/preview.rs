//! Markdown preview rendering for .md files.
//!
//! Phase 7: Parses markdown via pulldown-cmark and renders to ratatui Text.
//! Phase 8: Extracts mermaid code blocks, renders via mmdc, displays with ratatui-image.
//! Phase 9: Graceful degradation for missing tools and terminal capabilities.

pub mod markdown;
pub mod mermaid;
