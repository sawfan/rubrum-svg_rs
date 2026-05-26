# Avante project instructions

## Dependencies

Do NOT change the Cargo.toml dependencies to use the relative paths in ./librubrum 
You CAN access those projects through ./librubrum to gather info or to make feature requests that satisfy my requests. This folder is only here because of AVANTE's file access limitations though. ./librubum will not be commit and will not always be available. 

## Model
Use model: gpt-5.3

## General behavior
- Be concise and technical.
- Do not explain basics unless explicitly asked.
- Prefer direct answers over conversational tone.

## Rust LSP requirement (MANDATORY)

When working on Rust code:

- You MUST use Rust Analyzer / LSP capabilities to locate symbols (go-to-definition, find-references, type info) whenever those tools are available.
- Do NOT use manual text search (e.g. grep/ripgrep) as a substitute for symbol navigation.
- If the Rust LSP is unavailable (no client / rust-analyzer not running), STOP immediately and notify the user. Do not proceed by grepping.
- Starting/restarting the LSP is acceptable; once it is available, resume using it.

## Code
- Match existing style exactly.
- Do not reformat unless requested.
- Show full files when modifying code.
- Prefer correctness and clarity over cleverness.

## Reasoning
- State assumptions explicitly.
- If uncertain, say so.
- Do not hallucinate APIs, crates, or flags.
- Ask before making architectural changes.

## Scope
- Focus only on the current project.
- Do not introduce new dependencies unless requested.

