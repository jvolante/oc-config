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

## System Utilities

The environment requires specialized command-line tools:
- `ast-grep` - AST-based code search and manipulation
- `shellcheck` - Shell script linting and formatting
- `jq` - JSON query engine
- `taplo` - TOML utilities (linting, formatting, querying)
- `jaq` - YAML/TOML/XML/CBOR query engine
- `sage` - SageMath symbolic mathematics toolkit

## Quick Start

- Source `bash_env.sh` in your bashrc
- Clone this repo into ~/.config/opencode
