# Launch Blockers — Implementation Plan
**Date:** 2026-06-05
**Objective:** Address all 9 P0/P1 blockers identified in PROJECT_ASSESSMENT.md
**Approach:** 4 parallel workstreams with dependency-aware sequencing

---

## Workstream A: Monetization (Stripe Billing)
**Items:** #1 (billing routes), #9 (webhook signature verification)
**Owner:** Agent A + Agent A2
**Priority:** P0 — Without this, the SaaS cannot generate revenue

### A1. Create Checkout Session (`web/app/api/billing/create-checkout/route.ts`)
- POST handler, requires `getServerSession`
- Read `priceId` from body (or use env `STRIPE_PRICE_ID_PRO`/`TEAM`/`ENTERPRISE` as fallback)
- Look up `BillingCustomer` by `userId`; create if missing
- Create Stripe Checkout session with `mode: "subscription"`, `success_url: "/dashboard?checkout=success"`, `cancel_url: "/pricing"`
- Store `stripeSessionId` in a temporary `CheckoutSession` table or return directly
- Return `{ url: session.url }` (client redirects)
- **Error handling:** 401 if unauthenticated, 400 if no price ID, 500 if Stripe error

### A2. Create Portal Session (`web/app/api/billing/create-portal/route.ts`)
- POST handler, requires auth
- Look up `BillingCustomer` by `userId`; 404 if no Stripe customer ID
- Create Stripe Billing Portal session
- Return `{ url: portalSession.url }`

### A3. Webhook Handler (`web/app/api/billing/webhook/route.ts`)
- POST handler, **no auth required** (Stripe calls this)
- Verify `stripe-signature` header using `STRIPE_WEBHOOK_SECRET`
- Use `stripe.webhooks.constructEvent(rawBody, sig, secret)`
- Handle events:
  - `checkout.session.completed` → create `BillingSubscription`, update `User` tier
  - `invoice.payment_succeeded` → update subscription status to ACTIVE
  - `invoice.payment_failed` → mark subscription PAST_DUE, email user
  - `customer.subscription.deleted` → mark subscription CANCELED
- **Idempotency:** Check if subscription already processed before mutating
- **Raw body:** Must configure Next.js to not parse body (`export const config = { api: { bodyParser: false } }`)

### A4. Subscription Status (`web/app/api/billing/subscription/route.ts`)
- GET handler, requires auth
- Return current user's active subscription: tier, status, currentPeriodEnd, cancelAtPeriodEnd
- Fall back to legacy `Subscription` table for backward compatibility
- Return `{ subscription: { tier, status, currentPeriodEnd, cancelAtPeriodEnd } | null }`

### A5. Usage (`web/app/api/billing/usage/route.ts`)
- GET handler, requires auth
- Query `MeteredUsageEvent` for current billing period
- Aggregate by feature (AI commits, changelog generation, etc.)
- Return `{ events: [{ feature, count, cost }], totalCost, periodStart, periodEnd }`
- If no metered events exist, return mock/placeholder data for UI rendering

### A6. Stripe Client Setup (`web/lib/stripe.ts`)
- Initialize `stripe` with `STRIPE_SECRET_KEY`
- Export both server-side and client-side (publishable key) configs

### A7. Prisma Schema Additions
- `CheckoutSession` model (temporary, 24h TTL): id, stripeSessionId, userId, priceId, status, createdAt
- Add `stripeCustomerId` index on `BillingCustomer`
- Add `stripeSubscriptionId` unique on `BillingSubscription`

### A8. Tests
- Playwright: upgrade flow (Free -> Pro)
- Unit: webhook signature verification with mocked Stripe
- Unit: subscription status resolution (new + legacy fallback)

---

## Workstream B: Auth & Security Middleware
**Items:** #2 (middleware.ts), #3 (fix API auth), #4 (rate limiting)
**Owner:** Agent B
**Priority:** P0 — Data protection and cost control

### B1. Centralized Auth Middleware (`web/middleware.ts`)
- Use `NextResponse.next()` for public routes: `/`, `/auth/*`, `/pricing`, `/whitepapers/*`, `/case-studies/*`, `/compare/*`, `/platform`, `/security`, `/enterprise`, `/docs`, `/download`, `/api/auth/*`, `/api/billing/webhook`
- Redirect unauthenticated users from `/dashboard/*` to `/auth/signin`
- Add `x-user-id` header to all `/api/*` requests (for downstream use)
- Do NOT block API routes in middleware (Next.js edge runtime lacks Prisma); instead enforce auth per-route

### B2. Standardize API Route Auth
- Create `web/lib/api-auth.ts` with helpers:
  - `requireAuth(req)` → returns session or throws 401
  - `requireProjectAccess(req, projectId)` → returns project or throws 403
  - `requireEntitlement(req, feature)` → throws 403 if not entitled
- Apply to ALL `/api/kaptaind/*` and `/api/ai/*` routes
- Refactor `/api/policy/route.ts` and `/api/audit/route.ts` to use the same helpers

### B3. Rate Limiting (`web/lib/rate-limit.ts`)
- Use `lru-cache` for in-memory rate limiting (sufficient for MVP; upgrade to Redis later)
- Per-IP limits: 100 req/min for general APIs, 10 req/min for AI APIs
- Per-user limits: 50 AI requests/day on Pro, 500 on Team, unlimited on Enterprise
- Return `429 Too Many Requests` with `Retry-After` header
- Apply via wrapper function on AI routes; apply general limit via middleware if possible

### B4. CORS & Security Headers
- Add `next.config.ts` headers for:
  - `X-Frame-Options: DENY`
  - `X-Content-Type-Options: nosniff`
  - `Referrer-Policy: strict-origin-when-cross-origin`
  - `Content-Security-Policy` (restrictive, allow Stripe scripts)

---

## Workstream C: Database Migration
**Item:** #5 (SQLite -> PostgreSQL)
**Owner:** Agent C
**Priority:** P1 — Required for production SaaS

### C1. Schema Changes (`web/prisma/schema.prisma`)
- Change `provider = "sqlite"` to `provider = "postgresql"`
- Add `@db.Uuid` types for ID fields (or keep cuid — both work on PG)
- Add `@index` on frequently queried fields:
  - `AuditLog.timestamp`, `AuditLog.orgId`, `AuditLog.projectId`
  - `BillingSubscription.status`, `BillingSubscription.stripeSubscriptionId`
  - `MeteredUsageEvent.userId`, `MeteredUsageEvent.timestamp`
- Add explicit `@relation` for `AuditLog.projectId -> Project.id` (currently orphaned)

### C2. Environment Configuration
- Update `.env.example` with `DATABASE_URL="postgresql://..."`
- Update `docker-compose.yml` to include `postgres:15-alpine` service
- Update GitHub Actions to spin up PostgreSQL service container for tests

### C3. Migration Strategy
- Generate Prisma migration: `npx prisma migrate dev --name init_postgres`
- Seed script updated to check `NODE_ENV === "development"` before creating test user
- Document migration path for existing SQLite users (export/import or start fresh)

### C4. Connection Pooling
- Add `connection_limit` to DATABASE_URL or use Prisma Accelerate for serverless

---

## Workstream D: Hardening & Polish
**Items:** #6 (restrict debug route), #7 (PolicyEditor guards), #8 (Docker non-root)
**Owner:** Agent D
**Priority:** P1 — Security hardening

### D1. Restrict Debug Route (`web/app/api/debug/session/route.ts`)
- If route exists, gate behind `NODE_ENV === "development"`
- If production, return 404
- If development, require admin role

### D2. PolicyEditor JSON Guards (`web/components/dashboard/PolicyEditor.tsx`)
- Wrap all `JSON.parse()` calls in `try/catch`
- Display validation errors in UI instead of crashing
- Add Zod schema validation for policy shape before submission

### D3. Docker Non-Root (`Dockerfile`)
- Add `RUN useradd -m -u 1000 kaptaind` in builder
- Add `USER kaptaind` in runtime stage
- Ensure `.kaptaind` data directory is writable by UID 1000
- Update `docker-compose.yml` volume permissions

### D4. nginx Security Headers (`nginx.conf`)
- Add `add_header X-Frame-Options "DENY" always;`
- Add `add_header X-Content-Type-Options "nosniff" always;`
- Add `add_header Referrer-Policy "strict-origin-when-cross-origin" always;`
- Add gzip compression for static assets

---

## Cross-Cutting Concerns

### Testing Strategy
- Each workstream must add tests before marking complete
- Stripe: mocked webhook tests + Playwright checkout flow
- Auth: unit tests for middleware behavior + API auth helpers
- DB: verify Prisma client works with PostgreSQL in CI
- Docker: build verification in CI

### Rollback Plan
- Each workstream is independent; can be rolled back via git revert
- Database migration is the only irreversible change — back up SQLite before switching

### Documentation Updates
- Update `AGENTS.md` with new API patterns
- Update `README.md` with PostgreSQL setup instructions
- Update `SECURITY.md` with vulnerability reporting process
