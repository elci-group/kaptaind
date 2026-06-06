# Launch Blockers — Adjusted Implementation Plan
**Date:** 2026-06-05 | **Critique Applied:** Yes

---

## Adjustment Summary

Based on file inspection critique, the following changes were made to the original plan:

1. **P0 Escalation:** PolicyEditor JSON crash + debug route data exposure moved from P1 to P0
2. **next.config.ts:** Must be written from scratch (was empty skeleton)
3. **.dockerignore:** Added to Workstream D (missing, causes huge build context)
4. **Cargo layer caching:** Added to Dockerfile instructions
5. **lru-cache:** Specified as rate limiting dependency
6. **Stripe env validation:** Added fail-fast helper

---

## Workstream A: Monetization (Stripe Billing)
**Items:** #1 (billing routes), #9 (webhook signature verification)
**Agent:** Agent A
**Priority:** P0

### A1. Stripe Client (`web/lib/stripe.ts`)
```typescript
import Stripe from "stripe";
export const stripe = new Stripe(process.env.STRIPE_SECRET_KEY || "", {
  apiVersion: "2024-12-18.acacia",
});
export function assertStripeConfigured() {
  if (!process.env.STRIPE_SECRET_KEY) throw new Error("STRIPE_SECRET_KEY missing");
}
```

### A2. Create Checkout (`web/app/api/billing/create-checkout/route.ts`)
- POST, `requireAuth()` from `lib/api-auth.ts`
- Read `priceId` from body, fallback to env vars by tier
- Create Stripe customer if missing (store in BillingCustomer)
- Create Checkout session, return `{ url }`
- **Errors:** 401 unauthorized, 400 invalid price, 500 Stripe error

### A3. Create Portal (`web/app/api/billing/create-portal/route.ts`)
- POST, `requireAuth()`
- Lookup BillingCustomer by userId
- Create Portal session, return `{ url }`
- **Errors:** 404 if no customer, 401 unauthorized

### A4. Webhook (`web/app/api/billing/webhook/route.ts`)
- POST, NO auth (Stripe calls this)
- `export const config = { api: { bodyParser: false } }`
- Read raw body, verify `stripe-signature` with `STRIPE_WEBHOOK_SECRET`
- Events handled:
  - `checkout.session.completed` → create/update BillingSubscription, set ACTIVE
  - `invoice.payment_succeeded` → renew subscription status
  - `invoice.payment_failed` → set PAST_DUE
  - `customer.subscription.deleted` → set CANCELED
- **Idempotency:** Check existing subscription by stripeSubscriptionId before creating

### A5. Subscription Status (`web/app/api/billing/subscription/route.ts`)
- GET, `requireAuth()`
- Query BillingSubscription where status=ACTIVE for user
- Fallback to legacy Subscription table
- Return `{ subscription: { tier, status, currentPeriodEnd, cancelAtPeriodEnd } | null }`

### A6. Usage (`web/app/api/billing/usage/route.ts`)
- GET, `requireAuth()`
- Query MeteredUsageEvent for current period (start of month)
- Aggregate by feature
- Return `{ events, totalCost, periodStart, periodEnd }`

### A7. Prisma Schema Changes
- Add `stripeCustomerId` to BillingCustomer (already has it? verify)
- Ensure `stripeSubscriptionId` is unique on BillingSubscription
- Add `@index` on BillingSubscription.status, stripeSubscriptionId

### A8. Tests
- Playwright: pricing → click Pro Trial → redirect to checkout URL
- Mock webhook test with valid/invalid signatures

---

## Workstream B: Auth & Security
**Items:** #2 (middleware), #3 (fix API auth), #4 (rate limiting)
**Agent:** Agent B
**Priority:** P0

### B1. Auth Helpers (`web/lib/api-auth.ts`)
```typescript
export async function requireAuth(req: Request): Promise<Session>
export async function requireProjectAccess(req: Request, projectId: string): Promise<Project>
export async function requireEntitlement(req: Request, feature: keyof Entitlements): Promise<void>
```

### B2. Apply Auth to All API Routes
- `/api/kaptaind/analysis/route.ts` — add requireAuth + project access check
- `/api/kaptaind/aoc/route.ts` — add requireAuth + project access check
- `/api/kaptaind/status/route.ts` — add requireAuth + project access check
- `/api/kaptaind/traces/route.ts` — add requireAuth + project access check
- `/api/ai/bump-reasoning/route.ts` — add requireAuth + entitlement check (canUseAi)
- `/api/ai/changelog/route.ts` — add requireAuth + entitlement check
- `/api/ai/commit-message/route.ts` — add requireAuth + entitlement check
- Refactor `/api/policy/route.ts` and `/api/audit/route.ts` to use helpers

### B3. Middleware (`web/middleware.ts`)
- Edge runtime compatible (no Prisma)
- Public routes whitelist: `/`, `/auth/*`, `/pricing`, `/whitepapers/*`, `/case-studies/*`, `/compare/*`, `/platform`, `/security`, `/enterprise`, `/docs`, `/download`, `/api/auth/*`, `/api/billing/webhook`
- Redirect `/dashboard/*` unauthenticated → `/auth/signin`
- Add `x-request-id` header for tracing

### B4. Rate Limiting
- Install `lru-cache` (^11.0.0)
- `web/lib/rate-limit.ts`:
  - In-memory per-IP: 100 req/min general, 10 req/min AI
  - Per-user AI limits: 50/day Pro, 500/day Team, unlimited Enterprise
  - Return 429 with `Retry-After`
- Apply to `/api/ai/*` routes via wrapper

### B5. next.config.ts (rewrite from scratch)
- Security headers: X-Frame-Options, X-Content-Type-Options, Referrer-Policy, CSP
- No `output: "export"` (API routes require server)
- `distDir: "dist"` if needed
- Turbopack config

---

## Workstream C: Database Migration
**Item:** #5 (SQLite → PostgreSQL)
**Agent:** Agent C
**Priority:** P1

### C1. Schema Updates (`web/prisma/schema.prisma`)
- Change `provider = "sqlite"` → `provider = "postgresql"`
- Add `@index [timestamp]` on AuditLog
- Add `@index [orgId]` on AuditLog
- Add `@index [projectId]` on AuditLog
- Add `@index [status]` on BillingSubscription
- Add `@index [userId, timestamp]` on MeteredUsageEvent
- Add `@relation` fields: AuditLog.projectId → Project.id

### C2. docker-compose.yml Update
- Add `postgres:15-alpine` service
- Add health check for postgres
- Daemon service depends_on postgres condition: service_healthy

### C3. CI Update
- `.github/workflows/web.yml`: Add postgres service container
- `.github/workflows/rust.yml`: No change needed

### C4. Seed Protection
- Gate test user creation behind `NODE_ENV === "development"`
- Document in README that production requires manual admin creation

### C5. Migration
- Generate migration: `npx prisma migrate dev --name init_postgres`
- Verify all models compile with PostgreSQL provider

---

## Workstream D: Hardening & Polish
**Items:** #6 (debug route), #7 (PolicyEditor), #8 (Docker)
**Agent:** Agent D
**Priority:** P0 (items 6,7) / P1 (item 8)

### D1. Debug Route Restriction (`web/app/api/debug/session/route.ts`)
- If `process.env.NODE_ENV !== "development"` → return 404
- If development, require admin role (check user.email against env ADMIN_EMAILS)

### D2. PolicyEditor JSON Guards (`web/components/dashboard/PolicyEditor.tsx`)
- Wrap `JSON.parse(value)` in `try/catch` in handleChange
- On parse error: set field-level error state, do NOT crash
- Add Zod schema for policy shape validation before save
- Show inline error per textarea

### D3. Dockerfile Security
- Add non-root user: `RUN useradd -m -u 1000 kaptaind`
- `USER kaptaind` in runtime stage
- Layer caching: copy Cargo.toml/Cargo.lock first, `cargo build --release` dependencies, THEN copy source
- Add `HEALTHCHECK CMD kaptaind-cli status || exit 1`

### D4. .dockerignore
- Create root `.dockerignore`:
  - web/node_modules, web/.next, web/dist
  - target/
  - .git/
  - *.md (except README/INSTALL)
  - tests/
  - deploy/

### D5. nginx.conf Security Headers
- X-Frame-Options DENY
- X-Content-Type-Options nosniff
- Referrer-Policy strict-origin-when-cross-origin
- gzip for static assets

---

## Execution Order

**Phase 1 (Parallel):**
- Workstream A (Stripe billing)
- Workstream B (Auth/security)
- Workstream D items 1-2 (Debug route + PolicyEditor) — quick wins

**Phase 2 (After Phase 1 completes):**
- Workstream C (PostgreSQL migration) — safer to do after auth/billing stable
- Workstream D items 3-5 (Docker + nginx)

**Rationale:** Auth helpers (B) must exist before Stripe routes (A) can use `requireAuth()`. However, Stripe routes can be written first with inline auth checks and refactored after B completes. Phase 2 items have no dependencies on Phase 1.
