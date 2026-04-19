# Memory & Vault Sync Test Checklist

Checklist for validating memory and vault sync flows in `crates/memory-sync`. Items marked **Automated** reference tests in `crates/memory-sync/tests/integration.rs`. Items marked **Manual** require a running Qdrant instance or live filesystem inspection.

---

## Parsing & Frontmatter

- [ ] **Malformed frontmatter (no colon)**: A note with malformed YAML frontmatter (missing `:`) returns a `MemorySyncError::ParseError`, not a panic. **Automated** → `malformed_frontmatter_no_colon_via_vault_accessor`
- [ ] **Unclosed frontmatter delimiter**: A note with an unclosed `---` block returns a parse error cleanly. **Automated** → `unclosed_frontmatter_returns_error`
- [ ] **Empty document is valid**: A completely empty `.md` file parses without error and produces a valid (empty) `VaultDoc`. **Automated** → `empty_document_is_valid`
- [ ] **Headings-only produces sections**: A document with headings but no body content produces sections with empty content strings (no panic or data loss). **Automated** → `headings_only_produces_sections_with_empty_content`
- [ ] **Frontmatter round-trip preserves key-values**: Write a `VaultDoc` with frontmatter fields and read it back; all key-value pairs are preserved exactly. **Automated** → `frontmatter_round_trip_preserves_key_values`
- [ ] **Malformed UTF-8 handling**: A file containing invalid UTF-8 bytes returns a `MemorySyncError` (IO or parse), not a panic. **Manual** (create file with `echo -e '\xFF\xFE' > bad.md` and call `VaultAccessor::read`)

---

## Vault Read / Write

- [ ] **Open nonexistent path returns error**: `VaultAccessor::open` on a nonexistent directory returns an error. **Automated** → `open_nonexistent_path_returns_error`
- [ ] **Open file path instead of directory returns error**: `VaultAccessor::open` on a file path (not a directory) returns an error. **Automated** → `open_file_path_instead_of_directory_returns_error`
- [ ] **Open valid directory succeeds**: `VaultAccessor::open` on a valid temp directory succeeds. **Automated** → `open_valid_directory_succeeds`
- [ ] **Read-only canonical doc (SOUL.md)**: Write to `SOUL.md` via `VaultAccessor`; verify a `ReadOnly` error is returned. **Automated** → `write_to_readonly_soul_returns_readonly_error`
- [ ] **Read-only canonical doc (IDENTITY.md)**: Attempt write to `IDENTITY.md`; verify `ReadOnly` error. **Automated** → `write_to_readonly_identity_returns_readonly_error`
- [ ] **Approval-required doc rejected without approval**: Write to an approval-required doc without providing approval; verify `ApprovalRequired` error. **Automated** → `write_to_approval_required_user_without_approval_errors`
- [ ] **Runtime-writable HEARTBEAT.md**: Write to `HEARTBEAT.md` (runtime-writable); verify write succeeds and content is persisted. **Automated** → `write_to_runtime_writable_heartbeat_succeeds`
- [ ] **Runtime-writable TOOLS.md**: Write to `TOOLS.md`; verify write succeeds. **Automated** → `write_to_runtime_writable_tools_succeeds`
- [ ] **Missing canonical doc returns None**: `VaultAccessor::read` on a canonical doc slug that does not exist returns `None`. **Automated** → `vault_read_nonexistent_canonical_doc_returns_none`

---

## Duplicate / Idempotent Writes

- [ ] **Idempotent write produces stable content**: Writing the same `VaultDoc` twice produces identical file content (no duplicate sections, no stale data). **Automated** → `idempotent_write_produces_stable_content`
- [ ] **Duplicate note upsert is idempotent**: Writing the same note twice via `SyncEngine` produces exactly one entry; no duplicates in vault or retrieval index. **Automated** → `duplicate_note_upsert_is_idempotent`

---

## Sync Engine

- [ ] **Structured store → vault sync**: `SyncEngine::sync_to_vault` writes status and tool docs to vault files correctly. **Automated** → `sync_to_vault_writes_status_and_tools`
- [ ] **Sync on empty vault**: Running `sync_to_vault` on a fresh vault directory with no `.md` files creates the required docs cleanly. **Automated** → `sync_to_vault_on_empty_vault_creates_docs`
- [ ] **Vault index is consistent across two calls**: Calling `SyncEngine::index_vault` twice returns consistent results with no phantom entries. **Automated** → `index_vault_is_consistent_across_two_calls`
- [ ] **Machine sections preserve human prose**: When updating machine-controlled sections in a doc, human-written prose sections are not modified. **Automated** → `update_machine_sections_preserves_human_prose`

---

## Sync Conflict Resolution

- [ ] **Conflict: manual edit then sync preserves human prose**: Manually edit a vault file, then call `sync_to_vault`; verify human-written prose is preserved and machine sections are updated. **Automated** → `detect_conflict_after_manual_edit_then_sync_preserves_human_prose`
- [ ] **Sync conflict resolution (documented behavior)**: A manually edited vault file wins on prose sections; machine-generated sections are overwritten by sync. This is the expected behavior: the vault is the source of truth for human content, the structured store is the source of truth for machine content. **Automated** → `sync_conflict_resolution`
- [ ] **Conflict detection triggers appropriate event**: Verify that a detected conflict emits a `MemorySyncConflict` event (or equivalent) that can be observed by the event bus. **Manual**

---

## Canonical Docs

- [ ] **`init_defaults` creates all canonical docs**: On a fresh vault, `VaultAccessor::init_defaults` creates `SOUL.md`, `IDENTITY.md`, `HEARTBEAT.md`, `TOOLS.md`, and other canonical slugs. **Automated** → `init_defaults_creates_all_canonical_docs`
- [ ] **`init_defaults` is idempotent**: Calling `init_defaults` twice does not overwrite existing content. **Automated** → `init_defaults_is_idempotent_and_does_not_overwrite`
- [ ] **SOUL.md template round-trip**: Write SOUL.md via the canonical template and read it back; structure is preserved. **Automated** → `soul_template_round_trips`

---

## Personality Loading & Injection

- [ ] **Personality load includes SOUL and IDENTITY**: `load_personality` reads `SOUL.md` and `IDENTITY.md` into the `PersonalityConfig`. **Automated** → `load_personality_includes_soul_and_identity`
- [ ] **SOUL comes before IDENTITY in output**: In the assembled personality sections, SOUL content precedes IDENTITY content. **Automated** → `personality_soul_comes_before_identity`
- [ ] **`inject_personality` produces correct markdown**: `inject_personality` appends a `## System` section containing personality content to the assembled prompt. **Automated** → `inject_personality_adds_system_section_with_content`
- [ ] **Personality tokens field matches section sum**: The `tokens` field in `PersonalityConfig` equals the sum of token counts across all loaded sections. **Automated** → `personality_tokens_field_matches_sum_of_sections`

---

## Token Budget / Truncation

- [ ] **Large note truncated within budget**: A `VaultDoc` exceeding the configured token budget is truncated to fit; the truncated result is not silently omitted but included in a shortened form. **Automated** → `large_soul_doc_is_truncated_within_budget`
- [ ] **Truncation is not silent**: When truncation occurs, the returned doc indicates it was truncated (field set or error annotated). Verify no data is silently dropped without any signal. **Manual** (inspect returned `VaultDoc` fields)
- [ ] **Context assembly is token-budget-aware**: `ContextEngine` picks up vault docs when relevant but respects the configured token budget; docs are not included if budget is exhausted. **Manual** (integration with `crates/context-engine`)

---

## Retrieval Index (Qdrant)

- [ ] **Vault → retrieval index sync**: After writing a note to the vault via `SyncEngine`, querying the retrieval index returns the written note. **Manual** (requires live Qdrant)
- [ ] **Semantic search returns relevant results**: Query the retrieval index with a prompt related to vault content; verify top results are semantically relevant. **Manual** (requires live Qdrant)
- [ ] **Upsert does not create duplicates in Qdrant**: Upserting the same note twice produces a single point in Qdrant, not two. **Manual** (requires live Qdrant, verify with Qdrant dashboard or `count` API)
