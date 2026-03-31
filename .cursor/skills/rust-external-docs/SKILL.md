---
name: rust-external-docs
description: Documents Rust items using the no-incode-comments `#[external_doc]` macro and keyed markdown sections in `docs/`. Use when the user asks to “write docs”, “add external_doc docs”, “move docs out of code”, or maintain `docs/*.md` keys for structs/functions/methods.
---

# Rust external docs (no-incode-comments)

## Core rules (must follow)

### Keyed markdown format

- Documentation is stored in a markdown file (usually under `docs/`).
- The macro extracts text by **key**, where keys are **top-level headings** that start with `# `.
  - ✅ Key example: `# ExpectedTypedBytes`
  - ✅ Nested keys are still top-level headings: `# ExpectedTypedBytes::deref`
  - ❌ `##` headings do **not** create keys (they are just normal markdown inside the current key).
- Everything after the `# <KEY>` line, up until the next `# ` heading, becomes the rustdoc body.

### Applying `#[external_doc]`

- Add `use no_incode_comments::external_doc;` in the module if needed.
- Apply `#[external_doc(path = "...", key = "...")]` directly to the item you are documenting.
- In this codebase, it is valid to apply `#[external_doc]` to:
  - Structs: `pub struct ...`
  - Free functions: `pub fn ...`
  - Methods inside `impl` blocks (each method is its own item for the macro)

### Writing doc keys for methods

Use a predictable naming scheme:

- Struct/type: `# TypeName`
- Method: `# TypeName::method_name`
- Free function: `# function_name` (or `# module::function_name` if you want to avoid collisions)

Then attach the macro:

```rust
#[external_doc(path = "docs/util/low_level.md", key = "ExpectedTypedBytes")]
pub struct ExpectedTypedBytes<T> { /* ... */ }

impl<T> ExpectedTypedBytes<T> {
  #[external_doc(path = "docs/util/low_level.md", key = "ExpectedTypedBytes::from_bytes")]
  pub fn from_bytes(...) -> Self { /* ... */ }
}
```

## Workflow

1. Find the Rust item(s) that need docs (struct/function/method).
2. Find (or create) the markdown file under `docs/...` that should hold the docs.
3. Add a **top-level** `# <KEY>` section for each item you will document.
4. Add/adjust the `#[external_doc(path=..., key=...)]` attribute on the Rust item to point at the key.
5. Validate quickly by running `cargo doc --no-deps` (preferred) or `cargo build` to ensure the macro resolves the path and key.

## Source-file symlinks (docs discoverability)

When docs live in `docs/<area>/foo.md`, add a sibling symlink to the implementation file so readers can jump from docs → code in the same folder view.

Example pattern used in this repo:

- `docs/util/low_level.md` (external doc text)
- `docs/util/low_level.rs` → `../../src/util/low_level.rs` (symlink)

Create/refresh symlink:

```bash
ln -sf ../../src/util/low_level.rs docs/util/low_level.rs
```

Also add a short “Source” line near the top of the relevant key:

```markdown
# ExpectedTypedBytes

**Source:** [`low_level.rs`](low_level.rs) (symlink to [`src/util/low_level.rs`](../../src/util/low_level.rs)).
```

## Common pitfalls

- **Wrong heading level**: using `## Key` won’t be found. Keys must be `# Key`.
- **Key mismatches**: the `key = "..."` string must match the heading text exactly.
- **Path mistakes**: `path = "..."` is resolved at compile time from the crate root; keep it repo-relative (as done elsewhere in this project).
- **Accidental duplication**: if you move docs into markdown, delete redundant `///` blocks unless the user wants them kept.

## Output expectations

- Keep doc text concise and action-oriented: purpose, invariants/semantics, how it’s used in this codebase.
- Prefer linking to crate items with rustdoc links where stable (e.g. `crate::server::ServerMessageHandler`).
- Put per-method details under per-method keys so each method can import its own docs.

