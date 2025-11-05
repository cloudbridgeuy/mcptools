//! Core library for mcptools
//!
//! This crate implements the **Functional Core** of the mcptools application,
//! following the Functional Core - Imperative Shell architectural pattern.
//!
//! # Architecture Overview
//!
//! The mcptools project uses a two-crate architecture to enforce separation of concerns:
//!
//! - **`mcptools_core`** (this crate): Pure transformation functions with zero I/O
//! - **`mcptools`**: I/O operations and orchestration (the Imperative Shell)
//!
//! ## Functional Core Principles
//!
//! All functions in this crate adhere to these principles:
//!
//! - **Pure functions**: Same input always produces the same output
//! - **No side effects**: No I/O operations, no external state mutations
//! - **Deterministic**: Behavior is predictable and reproducible
//! - **Testable**: Can be tested with simple fixture data, no mocking required
//!
//! ## Benefits
//!
//! This architectural separation provides:
//!
//! - **Enhanced testability**: Pure functions are trivial to test comprehensively
//! - **Better maintainability**: Business logic is isolated from I/O concerns
//! - **Improved reusability**: Transformations can be used across CLI, MCP, and future contexts
//! - **Clearer reasoning**: Functions can be understood in isolation
//!
//! # Module Organization
//!
//! The core crate is organized by domain:
//!
//! - [`atlassian`]: Transformations for Atlassian services (Jira, Confluence)
//! - [`hn`]: Transformations for HackerNews API data
//! - [`md`]: Transformations for web page to Markdown conversion
//! - [`upgrade`]: Transformations for version comparison and upgrade logic
//!
//! Each module contains:
//!
//! - **Domain models**: Structured types representing API responses and outputs
//! - **Transformation functions**: Pure functions that convert API data to domain models
//! - **Comprehensive tests**: Unit tests using fixture data (no mocking)
//!
//! # Example Usage
//!
//! ```rust,ignore
//! use mcptools_core::hn::{transform_hn_items, HnItem, ListOutput};
//!
//! // Create fixture data (no HTTP required)
//! let items = vec![
//!     HnItem {
//!         id: 12345,
//!         title: Some("Example Story".to_string()),
//!         // ... other fields
//!     }
//! ];
//!
//! // Transform using pure function
//! let output = transform_hn_items(items, "top".to_string(), 1, 10, 1);
//!
//! // Assert on results (no mocking needed)
//! assert_eq!(output.items.len(), 1);
//! assert_eq!(output.pagination.current_page, 1);
//! ```
//!
//! # Pattern Reference
//!
//! This architecture is based on Gary Bernhardt's Functional Core, Imperative Shell pattern.
//! The key insight: **data transformation logic should be pure and ignorant of where data
//! comes from or where it goes**.
//!
//! For implementation details, see the `REFACTORING-PLAN.md` in the project root.

pub mod atlassian;
pub mod hn;
pub mod md;
pub mod pagination;
pub mod queries;
pub mod upgrade;
