# ADR-0006: Auth and Token Handling

**Status:** Accepted

## Context

`rustpi` integrates with providers that use different authentication mechanisms:
- API keys (OpenAI, Anthropic)
- OAuth 2.0 browser flow (Google/Gemini, some enterprise providers)
- OAuth 2.0 device authorization flow (GitHub Copilot, headless/CI environments)

Tokens must be:
- Stored encrypted at rest on the local filesystem
- Refreshed automatically before expiry
- Revocable by the operator
- Scoped to a provider — one provider's token must never be used for another

Auth state changes must be observable through the event system.

## Decision

1. **`auth-core` is the single token authority.** No other crate reads or writes token storage directly.

2. **Three supported flows, selected per-provider in config:**
   - `AuthFlow::OAuthBrowser` — opens the system browser, receives callback on a local loopback server
   - `AuthFlow::DeviceCode` — displays a user code and polls the token endpoint; suitable for headless/SSH environments
   - `AuthFlow::ApiKey` — reads key from environment variable or config file; no refresh needed

3. **`ProviderAuth` trait** (in `auth-core::provider`) is implemented per provider adapter. `auth-core` orchestrates flow and storage; adapters provide the provider-specific URLs and token exchange logic.

4. **Token storage** uses the `TokenStore` trait with an encrypted-at-rest backend (implementation in Phase 3). Encryption key is derived from a machine secret (e.g. OS keychain or `~/.config/rustpi/key`).

5. **Automatic refresh:** `auth-core` tracks `expires_at` for each token. A background task refreshes tokens before expiry (target: refresh when <10% of lifetime remains). Refresh failures emit `AuthStateChanged { state: Expired }` and notify the operator.

6. **Auth state is published as `AgentEvent::AuthStateChanged`** whenever state transitions occur. Consumers (TUI auth pane, RPC clients) subscribe to this event rather than polling.

7. **Secrets are redacted** in all tracing spans, log lines, and serialized `AgentEvent`s. Token values never appear outside `auth-core`.

## Consequences

**Positive:**
- Auth logic is isolated; no provider adapter touches the keychain or disk directly
- Device code flow supports headless deployments (servers, CI, SSH sessions)
- Automatic refresh is transparent to the rest of the runtime

**Negative:**
- OS keychain integration varies by platform (macOS Keychain, Linux Secret Service, Windows DPAPI) — Phase 3 must handle all three or fall back to encrypted file
- The loopback callback server for browser OAuth requires a free port; port conflicts must be handled
- Token refresh failures surface as events, not panics — callers must handle `AuthState::Expired` gracefully
