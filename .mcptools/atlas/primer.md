### Project Purpose
A **Swiss Army Knife for Agentic Coding**, providing a comprehensive suite of tools (web browsing, Git, issue tracking, documentation, code generation, local indexing) specifically designed for **LLM-powered AI Agents**.

### Architecture & Patterns
- **Language**: Built entirely in **Rust**.
- **Core Pattern**: **Functional Core - Imperative Shell**.
  - **`crates/core`**: Houses pure logic, structs, and interfaces.
  - **`mcptool`**: Centralizes I/O and side effects, importing pure code to glue components together.
- **Invocation**:
  - **CLI**: Uses `clap` for parsing; serves as the interface for both humans and agents.
  - **MCP (Model Context Protocol)**: Exposes tools as MCP server endpoints, sharing the same core mechanics as CLI commands.
- **Model Integration**: Uses **Ollama** for local LLM inference.

### Key Modules
- **`crates/core`**: Pure logic and type definitions.
- **`mcptool`**: I/O and side-effect handling.
- **CLI/MCP Interface**: Unified tooling where almost all CLI commands have direct MCP equivalents.

### Key Conventions
- **Functional Core - Imperative Shell**: Always split `pure` code, types, and interfaces, from procedural, I/O, or side-effect producing code.
- **Type Safety**: Relies on **Parse Don't Validate** and **Algebraic Data Types (Sum/Product types)** to enforce valid states and eliminate runtime errors.
- **Unified Tooling**: CLI and MCP share the same underlying core mechanics.
- **Testing**:
  - **Unit Tests**: Comprehensive coverage required for all `pure` code.
  - **Integration/E2E**: Not enforced as acceptance criteria for new features.
  - **Documentation**: New features must include documentation on functionality and testing methods.
- **Build & DevOps**: Managed via **`cargo xtask`** for build, format, lint, and test operations.
