# OpenCode Configuration Repository

This repository contains the custom configuration, commands, and skills for OpenCode. It defines how OpenCode behaves, what tools are available, and what specialized workflows can be invoked.

## Repository Structure

```
.
├── opencode.json          # Main OpenCode configuration file
├── AGENTS.md             # Guidelines for agent behavior and development best practices
├── commands/             # Custom slash commands available in OpenCode
├── skills/               # Specialized skill packs for domain-specific tasks
├── agents/               # Custom agent definitions
├── package.json          # Node.js dependencies
└── bash_env.sh          # Bash environment setup
```

## Configuration Overview

### `opencode.json`

The main configuration file that controls:
- **Model settings**: Large, medium, and small model assignments
- **Language Servers (LSP)**: Configured for Python (ruff), Lua, Rust, Bash, YAML, C/C++, Markdown, JSON, GLSL, Protocol Buffers, Nix, CMake, TOML, Typst, and more
- **Permissions**: External directory access and file edit permissions
- **MCP Servers**: Model Context Protocol integrations (lib-info for library analysis)
- **Agent Model Assignments**: Specialized models for different agent types

### `AGENTS.md`

Development guidelines and best practices including:
- Build and deployment practices
- Code quality standards (defensive programming, constants over magic numbers)
- Unit testing requirements
- Git workflow conventions (conventional commits, branch naming)
- Language-specific guidance (Python, Shell, Nix, C++)
- Available system utilities (ast-grep, jq, taplo, jaq, sage)

## Available Commands

Custom slash commands available via `ctrl+p` in OpenCode:

### `/fix-ci-failures`
Debug and resolve CI/CD pipeline failures. Analyzes build logs, identifies failure reasons, and helps fix broken workflows and pipelines.

### `/execute-plan-parallel`
Execute multiple independent tasks in parallel for improved efficiency. Useful for running independent subtasks concurrently without waiting for sequential completion.

### `/make-presentation`
Generate presentation materials from code, documentation, or analysis. Helps create slides and visual summaries of your work.

### `/improve-skills-commands`
Enhance and extend the available skills and commands in your OpenCode configuration. Refine workflows and add new capabilities.

### `/execute`
Execute a complex task or workflow. General-purpose command for running multi-step processes with planning and orchestration.

### `/resolve-pr-comments`
Address feedback and comments on pull requests. Helps manage and respond to PR review comments systematically.

## Available Skills

Skills are specialized workflow packs that provide domain-specific tools and instructions. They are automatically loaded when relevant.

| Skill | Purpose |
|-------|---------|
| **CircleCI** (`circleci`) | Query, trigger, and rerun CircleCI pipelines and workflows via REST API |
| **Confluence** (`confluence`) | Search, read, and create Confluence pages and comments |
| **Jira** (`jira`) | Query, update, and create Jira tickets with JQL support |
| **CUDA Texture Reference** (`cuda_texture_reference`) | Empirically verified CUDA texture object constraints and best practices |
| **Graphify** (`graphify`) | Transform code/docs into navigable knowledge graphs with visualization |
| **Open PRs** (`open-prs`) | Query and filter open pull requests with CI correlation |
| **Replace in Files** (`replace-in-files`) | Bulk regex find-and-replace across directory trees |

## Language Server Support

The configuration includes LSP support for:
- **Python**: ruff (linting, formatting)
- **JavaScript/TypeScript**: jsonls
- **C/C++**: clangd (disabled by default)
- **Rust**: rust-analyzer
- **Lua**: lua-ls
- **Bash**: bash-language-server
- **YAML**: yamlls
- **Markdown**: marksman
- **JSON/JSONC**: jsonls
- **GLSL**: glsl_analyzer
- **Protocol Buffers**: buf_ls
- **Nix**: nixd, nil
- **CMake**: neocmake
- **TOML**: taplo
- **Typst**: tinymist

## System Utilities

The environment includes specialized command-line tools:
- `ast-grep` - AST-based code search and manipulation
- `shellcheck` - Shell script linting and formatting
- `jq` - JSON query engine
- `taplo` - TOML utilities (linting, formatting, querying)
- `jaq` - YAML/TOML/XML/CBOR query engine
- `sage` - SageMath symbolic mathematics toolkit

## Development Guidelines

### Best Practices
- Use conventional commits (feat, fix, docs, etc.)
- Create feature branches: `feature/ticket-number-description`
- Practice defensive programming with assertions
- Avoid magic numbers; use named constants
- Thoroughly test all code paths, especially error handling

### Testing
- Prefer many small tests over few large ones
- Always verify status codes and CUDA return values
- Set random seeds in tests that use random values
- Use the build-test-summarizer agent to analyze failures

### Language-Specific Notes
- **Python**: Prefer PySide (not PyQt), use `Path` over `os.path`, prefer ruff for linting
- **Shell**: Use `printf` over `echo`, prefer `/usr/bin/env` shebang
- **Nix**: Use `inputsFrom` for devShell dependencies
- **C++**: Use `const` and `constexpr` wherever possible

## Getting Help

- Press `ctrl+p` in OpenCode to list all available actions
- Report issues: https://github.com/anomalyco/opencode
- Check OpenCode documentation: https://opencode.ai/docs

## Quick Start

1. **Use an existing command**: Press `ctrl+p` and select from available commands
2. **Load a skill**: Skills are automatically loaded when your task matches their description
3. **Create a new command**: Add a new markdown file to `commands/` with OpenCode-compatible syntax
4. **Extend a skill**: Modify or add content to relevant skill directories in `skills/`
