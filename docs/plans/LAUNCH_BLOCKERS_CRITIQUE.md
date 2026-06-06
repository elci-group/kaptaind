# Launch Blockers Plan — Architecture & Security Critique
**Date:** 2026-06-04  
**Auditor:** Senior Engineering Architect  
**Scope:** `LAUNCH_BLOCKERS_PLAN.md` and all referenced implementation files  
**Verdict:** The plan addresses the right *problems* but the *solutions* are underspecified, naive in several security-critical areas, and leave major architectural debts unaddressed. **This plan will produce a revenue-leaky, horizontally-unscalable, partially-broken SaaS if executed as written.**

---

## Executive Summary

The plan treats 9 P0/P1 blockers as independent feature tickets rather than as an integrated system. It misses:
- **The static-export vs. dynamic-runtime contradiction** that makes the entire web deployment story incompatible with the API routes being built.
- **Revenue protection gaps** in tier enforcement and usage metering.
- **Horizontal scaling assumptions** (in-memory rate limiting, no connection pooling strategy).
- **Data integrity risks** in the dual subscription model and audit log schema changes.

**Estimated additional work not in the plan:** ~1 week of hardening, plus a deployment architecture rethink.

---

## Workstream A: Monetization (Stripe Billing)

### 1. Shortcomings

**A. The dual-subscription model is a ticking bomb.**  
The plan proposes falling back to the legacy `Subscription` table (A4) without a migration path or reconciliation logic. Two sources of truth for tier/status will diverge the moment a webhook updates `BillingSubscription` while the UI or an API route still reads `Subscription`. The plan should:
- Deprecate `Subscription` explicitly.
- Write a one-time migration script.
- Remove fallback logic after cutover.

**B. Missing Stripe customer lifecycle.**  
The plan says "Look up `BillingCustomer` by `userId`; create if missing" at checkout time. But what about existing users who hit the billing portal before checkout? The portal route (A2) returns 404 if no Stripe customer ID exists. There is no "create customer on first portal visit" logic. This breaks the "Manage Billing" button for users who haven't completed checkout yet (e.g., gifted/enterprise accounts).

**C. `CheckoutSession` table has no cleanup mechanism.**  
The plan says "temporary, 24h TTL" but specifies no cron job, no Postgres TTL, no Prisma deleteMany on read. This table will grow indefinitely.

**D. Usage API (A5) is schema-incompatible with the data model.**  
The plan says:
> "Aggregate by feature (AI commits, changelog generation, etc.)" → `{ events: [{ feature, count, cost }] }`

But `MeteredUsageEvent` has `meterName: String`, `quantity: Float`, and `customerId: String` — not `userId`, not `feature`, not `cost`. The plan invents a return shape that doesn't match the schema and omits how `cost` is computed (Stripe lookup? Hardcoded?).

**E. No actual metered billing integration with Stripe.**  
Storing `MeteredUsageEvent` rows locally is not billing. The plan never mentions `stripe.subscriptionItems.createUsageRecord` or Stripe Billing Meter API. Without this, "usage-based billing" is a dashboard fiction.

**F. Missing webhook events.**  
The plan handles 4 events. Missing critical ones:
- `customer.subscription.updated` — plan downgrades/upgrades, quantity changes.
- `customer.subscription.trial_will_end` — required for trial UX.
- `invoice.payment_action_required` — 3D Secure / SCA in EU.
- `charge.dispute.created` — revenue protection.

**G. No price validation.**  
A1 reads `priceId` from the request body and passes it straight to Stripe. An attacker can pass an arbitrary `priceId` (e.g., a $0.01 price they created in their own Stripe account, if they somehow know one, or just fuzz it). The plan should validate `priceId` against an allowlist (`STRIPE_PRICE_ID_PRO`, `STRIPE_PRICE_ID_TEAM`, etc.) and reject unknowns with 400.

**H. Org vs. User billing ambiguity.**  
The schema supports both `userId` and `orgId` on `BillingCustomer`. The plan only mentions `userId` lookups. For Team/Enterprise plans, the checkout session should likely be associated with an `orgId`, not a `userId`. The plan is silent on this, which means the first org admin who upgrades will personally own the subscription.

### 2. Security Risks

**A. Webhook idempotency is underspecified.**  
"Check if subscription already processed before mutating" — how? By `stripeSubscriptionId`? What about `invoice.payment_succeeded` where the same subscription generates multiple invoices? The idempotency key should be event-scoped (`stripeEventId`), not subscription-scoped.

**B. Raw body configuration is App Router-incompatible.**  
The plan says:
> `export const config = { api: { bodyParser: false } }`

This is the **Pages Router** API config. The billing routes are in App Router (`route.ts`). In App Router, you must read the raw body via `req.text()` **before** any `req.json()` call elsewhere in the pipeline. The plan will cause webhook signature verification to fail silently because Next.js may buffer/parse the body differently.

**C. No webhook IP allowlisting or timestamp tolerance.**  
Stripe recommends verifying the `Stripe-Signature` timestamp is within 5 minutes of now to prevent replay attacks. The plan mentions none of this.

**D. No tier enforcement on AI routes.**  
The plan builds billing routes but never connects them to the AI routes (`/api/ai/*`). Currently, any authenticated user can hit `/api/ai/bump-reasoning` and burn inference tokens. The plan should add `requireEntitlement("canUseAi", ...)` to every AI route as part of Workstream A or B. Without this, billing is decoration.

### 3. Scalability Issues

**A. Synchronous webhook handling.**  
Webhook events are processed inline. At scale, a burst of Stripe events (e.g., end-of-month invoicing) will overwhelm the Next.js server. Should queue events and ack immediately.

**B. No usage aggregation strategy.**  
`MeteredUsageEvent` rows accumulate per API call. The A5 route does a `findMany` per request with no time-bucket aggregation table. At 100x scale, this query becomes a table scan.

### 4. Missed Opportunities

**A. Implement Stripe Customer Portal configuration.**  
The plan creates a portal session but doesn't configure `business_profile`, `default_return_url`, or `features` (e.g., allowing coupon updates, plan cancellation). This is 10 minutes of work that saves weeks of support tickets.

**B. Implement trial logic.**  
The schema has a `TRIALING` status. The plan never creates trial subscriptions. A 14-day Pro trial would massively improve activation.

### 5. Dependency Risks

- **A depends on C:** The `CheckoutSession` model (A7) requires the schema migration (C1) to be applied first.
- **A depends on B:** The billing routes need auth. If B delivers `requireAuth` after A, A's routes will use ad-hoc auth and need refactoring.
- **A conflicts with existing seed data:** The seed creates a legacy `Subscription` with `stripeCustomerId: "cus_test_123"`. If A creates a `BillingCustomer` for the same user, you now have two customer records.

---

## Workstream B: Auth & Security Middleware

### 1. Shortcomings

**A. The middleware strategy is half-baked.**  
The plan wants to add `x-user-id` headers in middleware for downstream API use. This header is **trivially spoofable** if any API route trusts it without independently verifying the session. The plan correctly notes Edge runtime lacks Prisma, but the solution ("enforce auth per-route") means the middleware provides zero value for API security. It's just a router guard for `/dashboard/*`.

**B. No Edge-compatible session verification.**  
With JWT strategy, `next-auth/jwt`'s `getToken({ req })` works in Edge runtime and verifies the JWT signature. The plan should use this to set `x-user-id` *securely* in middleware, but it doesn't mention `getToken` at all.

**C. `requireAuth(req)` helper is undefined.**  
The plan says:
> `requireAuth(req)` → returns session or throws 401

But `getServerSession(authOptions)` does not accept a raw `Request`; it needs `(req, res, authOptions)` in Pages Router, or you call it with no args in App Router (it reads cookies from the implicit request context). In App Router `route.ts`, you actually *can* call `getServerSession(authOptions)` with no args. So the helper signature `requireAuth(req)` is misleading. It should probably be `requireAuth()` for App Router.

**D. No project-scoped auth for AI routes.**  
The AI routes (`/api/ai/*`) currently accept a `projectId` body parameter but never verify the user has access to that project. `projectAccessWhere` exists but isn't used in AI routes. The plan's `requireProjectAccess` helper is exactly what's needed here, but the plan doesn't mandate applying it to AI routes.

**E. `requireEntitlement` throws generic `Error`, not HTTP errors.**  
The existing `requireEntitlement` function throws `new Error("Forbidden: ...")`. In a route handler, this becomes a 500 unless wrapped. The plan should standardize on `NextResponse.json({ error: ... }, { status: 403 })` or a custom error class that route handlers catch.

### 2. Security Risks

**A. CSRF on `/api/auth/signup` is unaddressed.**  
The assessment flagged this as a critical issue. The plan doesn't mention it. The custom signup endpoint accepts JSON with no CSRF token, origin check, or double-submit cookie. While less critical for JSON endpoints than form posts, it's still a gap.

**B. Rate limiting uses `lru-cache` — useless for horizontal scale, trivial to bypass.**  
Per-IP limiting with `lru-cache` assumes one server instance. Deploy two containers and the attacker gets 2x the quota. Per-user limits require authentication first, which means the rate-limiter must run *after* auth, not in middleware. The plan says "Apply via wrapper function on AI routes" which is correct, but then says "apply general limit via middleware if possible" — if done in middleware, it's IP-only and doesn't distinguish authenticated vs. anonymous.

**C. No input validation / sanitization framework.**  
Beyond PolicyEditor Zod, the plan doesn't mandate Zod (or similar) for any API route. The signup route accepts arbitrary JSON. The billing routes accept `priceId` strings without schema validation. Every API route should validate its input shape.

**D. Missing security headers.**  
The plan lists 4 headers but misses:
- `Strict-Transport-Security` (HSTS) — required for any site handling payments.
- `Permissions-Policy` — reduces fingerprinting surface.
- `Content-Security-Policy` is mentioned but "restrictive, allow Stripe scripts" is vague. Stripe Checkout requires `frame-src https://js.stripe.com https://hooks.stripe.com`, `script-src https://js.stripe.com`, and `connect-src https://api.stripe.com`. Getting this wrong breaks checkout.

### 3. Scalability Issues

**A. `getServerSession` is expensive.**  
It hits the database (via PrismaAdapter) to verify the session even with JWT strategy because NextAuth v4 still does a session lookup. At 100x scale, this becomes a bottleneck. The plan should consider caching entitlement resolution for the request lifecycle.

**B. No API key support for daemon-to-web communication.**  
The Rust daemon needs to talk to the web API. Currently, this isn't addressed. OAuth sessions don't work for daemon processes. The plan should add API key auth (scoped to project) as a P1, not P2.

### 4. Missed Opportunities

**A. Use `next-auth/jwt` in middleware for secure `x-user-id`.**  
This would allow rate limiting and route guarding to happen in one place, reducing per-route boilerplate.

**B. Standardize error response format.**  
Every API route invents its own error shape. A single `AppError` class with `{ error: string, code: string, status: number }` would improve DX and client error handling.

### 5. Dependency Risks

- **B blocks A:** Billing routes need `requireAuth` helpers. If B is delayed, A will write its own auth checks, creating inconsistency.
- **B blocks D:** The debug route restriction (D1) should use the auth helpers from B.
- **B and C conflict on Prisma client location:** If B creates `web/lib/api-auth.ts` that imports `prisma`, and C changes the Prisma client configuration (connection pooling), B's auth helpers may fail in Edge runtime or tests.

---

## Workstream C: Database Migration

### 1. Shortcomings

**A. Indecisive ID strategy.**  
> "Add `@db.Uuid` types for ID fields (or keep cuid — both work on PG)"

This is not a plan; it's a shrug. cuid() works on PG but is suboptimal for B-tree indexes (prefix similarity causes index page splits). For a billing system, use `uuid()` with `@db.Uuid` or stick with cuid and accept the index bloat. Pick one and document why.

**B. `AuditLog.projectId` foreign key is dangerous.**  
The plan says:
> "Add explicit `@relation` for `AuditLog.projectId -> Project.id` (currently orphaned)"

Audit logs must be **append-only and tamper-evident**. Adding a foreign key with `onDelete: Cascade` (Prisma's default) means deleting a project deletes its audit trail — a compliance disaster. If you add a relation, it must be `onDelete: SetNull` with `projectId String?`. Better yet, keep it denormalized as a string reference and enforce integrity in application logic.

**C. Index list is incomplete.**  
Missing critical indexes:
- `User.email` (frequently queried, though `@unique` implies an index).
- `Session.userId` (for session cleanup).
- `Account.userId` (for user deletion).
- `Project.ownerId` (for "my projects" queries).
- `MeteredUsageEvent.timestamp` (for usage aggregation queries).
- `MeteredUsageEvent.customerId + timestamp` (composite, for A5's billing-period queries).

**D. No migration rollback strategy.**  
"Database migration is the only irreversible change — back up SQLite before switching" is not a rollback strategy. What if the PG migration fails mid-deploy? The plan needs:
- A blue/green or shadow migration approach.
- Validation queries to confirm data integrity post-migration.
- A runbook for reverting to SQLite if PG is unhealthy.

**E. Connection pooling hand-waving.**  
> "Add `connection_limit` to DATABASE_URL or use Prisma Accelerate"

Prisma Accelerate is a paid SaaS add-on. For a self-hostable product, this is a bad default dependency. The plan should specify:
- PgBouncer or similar for transaction pooling.
- Connection string format for pooled mode (`?pgbouncer=true`).
- Recommended pool size (e.g., `connection_limit=5` per instance).

### 2. Security Risks

**A. No mention of database credential rotation.**  
The plan updates `.env.example` but doesn't address secret management. `DATABASE_URL` with embedded credentials in env vars is standard for MVP but should be flagged for hardening (e.g., use a secrets manager before SOC 2).

**B. Seed script still creates hardcoded credentials.**  
The assessment flagged this (P1): `test@example.com` / `password123`. The plan doesn't mention it. The seed should gate test user creation behind `NODE_ENV === "development"` and use a random password.

### 3. Scalability Issues

**A. SQLite → PG changes query semantics.**  
SQLite is case-insensitive for `LIKE` by default; PG is case-sensitive. Any existing search/filter queries will break silently. The plan doesn't mention query auditing.

**B. No partitioning strategy for `AuditLog` or `MeteredUsageEvent`.**  
At enterprise scale, these tables grow unbounded. PG partitioning by month should be architected now, even if not implemented.

### 4. Missed Opportunities

**A. Add `createdAt` indexes on all event tables now.**  
Cheap to add during migration, expensive to add later on a billion-row table.

**B. Use `prisma migrate dev` in CI to generate migrations, then `prisma migrate deploy` in production.**  
The plan only mentions `migrate dev`, which is unsafe for production. Document the deploy-time command.

### 5. Dependency Risks

- **C is on the critical path for everything.** Every workstream touches the database. If C fails or is delayed, A, B, and D are blocked.
- **C changes test infrastructure:** GitHub Actions needs a PG service container. If this isn't ready, all CI fails.

---

## Workstream D: Hardening & Polish

### 1. Shortcomings

**A. Debug route restriction is naive.**  
> "If route exists, gate behind `NODE_ENV === "development"`"

Environment-variable security is brittle. `NODE_ENV` can be overridden at runtime. The route should be **deleted entirely** before launch, or gated by an explicit admin role check + allowlist. The plan's approach is "good enough for MVP" but shouldn't be.

**B. Docker non-root user is specified incorrectly.**  
The plan says:
> "Add `RUN useradd -m -u 1000 kaptaind` in builder"  
> "Add `USER kaptaind` in runtime stage"

The builder stage (`rust:1.82-bookworm`) and runtime stage (`debian:bookworm-slim`) are **different images**. Creating the user in builder does not propagate to runtime. You must create the user in the runtime stage, or use a numeric UID (`USER 1000:1000`) if the user doesn't exist.

**C. Volume permissions are unspecified.**  
The docker-compose uses a named volume `daemon-data:/opt/kaptaind/.kaptaind`. Named volumes are initialized with root ownership. If the daemon runs as UID 1000, it won't be able to write to `/opt/kaptaind/.kaptaind` without an entrypoint script that `chown`s the directory. The plan doesn't mention this.

**D. Dockerfile has no layer caching for Cargo deps.**  
The Dockerfile does:
> `COPY . .`  
> `RUN cargo build --release ...`

This invalidates the build cache on any file change. The plan should add:
```dockerfile
COPY Cargo.toml Cargo.lock ./
RUN mkdir src && echo "fn main() {}" > src/main.rs && cargo build --release
COPY . .
RUN cargo build --release
```

**E. nginx.conf is irrelevant to the actual architecture.**  
The current docker-compose serves the web app as static files via nginx. Once API routes exist (billing, auth, AI), the web app **must** run as a Node.js server (Next.js standalone or similar). nginx can reverse-proxy to it, but the plan treats nginx headers as a sufficient web security layer. This is false if the actual web app is served by Next.js directly (e.g., on Vercel or as a standalone container).

### 2. Security Risks

**A. The Dockerfile still copies everything including `.env` files.**  
No `.dockerignore` hardening is mentioned. If a developer has a `.env.local` with `STRIPE_SECRET_KEY`, it will end up in the image layers.

**B. No secret management strategy.**  
Stripe keys, DB credentials, NextAuth secret — all live in env vars. The plan doesn't mention:
- Secret rotation procedures.
- Different keys for test/live Stripe environments.
- `NEXTAUTH_SECRET` generation and validation at startup.

### 3. Scalability Issues

**A. docker-compose is not production orchestration.**  
The plan updates `docker-compose.yml` with postgres but doesn't address:
- Health checks for Postgres.
- Restart policies and backoff.
- Log rotation (Docker default is JSON-file driver with no rotation).
- Resource limits (memory/CPU).

### 4. Missed Opportunities

**A. Use Next.js `standalone` output for Docker.**  
Instead of nginx serving static files, build Next.js with `output: "standalone"` and run it in a Node.js container. This is the standard pattern for containerized Next.js apps with API routes.

**B. Add a health check endpoint to the daemon Dockerfile.**  
The Rust daemon has a health server (per AGENTS.md). Expose it in Docker and add a `HEALTHCHECK` instruction.

### 5. Dependency Risks

- **D depends on B:** Security headers in nginx (D4) conflict with middleware headers (B4). If both are set, you get duplicate or conflicting headers.
- **D depends on deployment architecture:** If the team decides to use Next.js standalone output (which they must for API routes), nginx.conf changes become irrelevant.

---

## Cross-Cutting Concerns

### Testing Strategy

The testing plan is optimistic and incomplete:
- **No integration test for tier enforcement on AI routes.** This is the highest-value test; without it, free users can consume paid features.
- **No load test for webhook handler.** Stripe will retry webhooks aggressively if your handler is slow. A 5-second timeout on a synchronous DB write will cause event storms.
- **No test for the dual-subscription fallback.** The legacy → new billing migration path is untested.
- **Playwright checkout flow** is good but requires a real Stripe account or Stripe's test environment. The plan doesn't specify test fixture setup.

### Rollback Plan

> "Each workstream is independent; can be rolled back via git revert"

**False.** The workstreams are tightly coupled:
- A (billing) and C (schema) must land together or not at all.
- B (middleware) changes auth patterns that A and D rely on.
- Reverting A after C has migrated production data leaves orphaned `BillingCustomer` records.

The only truly independent workstream is D (hardening), and even then, the Docker changes affect how A's webhook routes are deployed.

### Deployment Architecture — The Elephant in the Room

**The plan completely ignores the static-export vs. dynamic-app contradiction identified in the assessment.**

The current setup:
- `next.config.ts` has no `output: "export"` currently (good).
- `docker-compose.yml` serves `deploy/web/` via nginx as static files (bad — this only works with static export).
- API routes (`/api/billing/*`, `/api/ai/*`) require a running Next.js server.

**This means the current docker-compose cannot host the product after Workstream A is complete.** The plan must address:
1. Build Next.js with `output: "standalone"`.
2. Run it in a Node.js container.
3. Optionally put nginx in front as a reverse proxy.
4. Remove or repurpose the static-export release workflow.

Without this, the team will build 5 billing API routes that have nowhere to run in production.

### Missing Critical Items (Not in Plan At All)

| Item | Assessment Priority | Why It Matters |
|------|---------------------|----------------|
| Remove hardcoded test credentials from seed | P1 | `test@example.com` / `password123` in production seed is a breach waiting to happen. |
| Tier enforcement on `/api/ai/*` routes | P0-level gap | Free users can burn inference credits unlimited. |
| API key auth for daemon | P1 | Daemon can't use OAuth. |
| Input validation (Zod) on all API routes | P1 | Prevents injection, malformed data, and 500s. |
| `customer.subscription.updated` handling | P0 | Users upgrading/downgrading plans will have stale entitlements. |
| Usage reporting to Stripe | P0 | Local usage events don't generate invoices. |
| NextAuth `NEXTAUTH_SECRET` validation at boot | P1 | App crashes mysteriously if secret is missing. |
| Environment variable validation | P1 | Missing `STRIPE_WEBHOOK_SECRET` → all webhooks fail with 500. |
| GDPR / data retention for billing data | P1 | Required for EU customers. |
| Billing email notifications | P1 | `invoice.payment_failed` says "email user" but no email provider is configured. |

---

## Recommendations

### Before executing the plan, add these items:

1. **Deployment architecture decision:** Switch to Next.js standalone output and update docker-compose before building any API routes. This is blocking.
2. **Schema decision:** Pick one ID strategy (recommend `uuid()` with `@db.Uuid` for billing tables, keep `cuid()` for auth to avoid NextAuth adapter issues). Add all missing indexes.
3. **Webhook hardening:** Use `req.text()` in App Router, add timestamp tolerance (±5 min), validate `priceId` allowlist, handle `subscription.updated`.
4. **Revenue protection:** Add `requireEntitlement("canUseAi", ...)` to all AI routes **before** checkout goes live. This is one line of code per route.
5. **Kill the debug route:** Don't gate it. Delete it. It's 14 lines; recovery from git history is trivial.
6. **Docker hardening:** Create user in runtime stage, add `.dockerignore`, add Cargo layer caching, add health check.
7. **Rate limiting reality check:** Acknowledge `lru-cache` is single-instance-only. Document "Redis-backed rate limiting" as a fast-follow.
8. **Seed safety:** Gate test user creation behind `NODE_ENV === "development"`. Use `crypto.randomUUID()` for test password if needed.

### Execution order:
1. **C first** (schema + migration + CI PG setup).
2. **B second** (auth helpers + middleware).
3. **A third** (billing routes, built on top of C and B).
4. **D fourth** (hardening, applied after architecture is stable).

**Parallel execution of A and B will create merge conflicts and inconsistent auth patterns. Serialize them.**

---

## Final Verdict

The plan is a **good first draft** that identifies the right workstreams but treats them as isolated tickets rather than an integrated system. It will get a demo working, but it will **not** produce a production-ready SaaS without the additional hardening identified above. The most dangerous gaps are:

1. **Revenue leakage** (no tier enforcement on AI routes).
2. **Deployment impossibility** (static nginx can't serve dynamic API routes).
3. **Audit integrity destruction** (foreign key cascade on audit logs).
4. **Billing data inconsistency** (dual subscription models with no reconciliation).

**Time to fix gaps:** Add ~5-7 days to the 2-3 week estimate.
