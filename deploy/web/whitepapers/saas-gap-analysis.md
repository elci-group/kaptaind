# Whitepaper: SaaS & Enterprise Claims — Gap Analysis

## Abstract
The Kaptaind landing page includes several enterprise-focused claims related to SSO, RBAC, cryptographic audit trails, custom retention, self-hosting, and policy engines. This whitepaper documents which claims are fully implemented, partially implemented, or aspirational.

## Claims Evaluated

### 1. SSO / SAML / OIDC
**Claim:** "Secure team access integrated with Okta, Azure AD, and Google Workspace."

**Status: PARTIAL**
- The Prisma schema includes `OrganizationMembership` with roles (`OWNER`, `ADMIN`, `MEMBER`).
- The NextAuth configuration (`web/lib/auth.ts`) currently supports only GitHub OAuth and email/password (CredentialsProvider).
- No SAML, OIDC, or SCIM provider configurations were found in the codebase.
- **Conclusion:** Schema and role model exist; provider integrations do not.

### 2. Role-Based Access Control (RBAC)
**Claim:** "Granular permissions for policy definition, version overrides, and audit log exports."

**Status: PARTIAL**
- The schema defines `OrganizationMembership.role` and `TeamMember.role`.
- `web/lib/entitlements.ts` maps plans to boolean capabilities (`canUsePolicyPacks`, `canExportAuditLogs`).
- No middleware enforcing these permissions on API routes was found.
- **Conclusion:** Role storage and entitlement definitions exist; enforcement middleware is missing.

### 3. Immutable Audit Trails
**Claim:** "Cryptographically signed release traces mapping every line changed to its version decision."

**Status: PARTIAL**
- The `AuditLog` model has `beforeHash` and `afterHash` fields.
- The daemon writes analysis artifacts and trace files locally.
- **No cryptographic chaining** (Merkle tree, sequential hashes, or digital signatures) was found.
- Webhook HMAC signatures exist (`src/angler/webhooks.rs`) but sign outbound HTTP payloads, not audit trails.
- **Conclusion:** Audit records exist; cryptographic immutability is not implemented.

### 4. Custom Retention Policies
**Claim:** "Keep traces and artifact history for 30 days, 180 days, or indefinitely."

**Status: PARTIAL**
- `Entitlements.retentionDays` is defined per plan (Free=0, Pro=30, Team=180, Enterprise=3650).
- **No automated pruning or expiration logic** was found in the daemon or web backend.
- **Conclusion:** Entitlements define retention limits; enforcement is manual or not yet built.

### 5. Enterprise Self-Hosting
**Claim:** "Deploy in your own VPC with external Postgres, object storage, and 100% air-gapped support."

**Status: PARTIAL**
- The Prisma schema targets SQLite by default (`provider = "sqlite"`).
- Postgres can be configured via `DATABASE_URL` (standard Prisma behavior).
- No explicit air-gapped toggles, offline-mode flags, or S3-compatible storage adapters were found.
- **Conclusion:** Self-hosting is possible (Next.js + Prisma); air-gapped and object storage features are not explicitly implemented.

### 6. Advanced Policy Engine
**Claim:** "Apply organization-wide branch protection, test requirements, and restricted file check policies."

**Status: PARTIAL**
- The `Policy` model has JSON columns: `versionBumpRules`, `branchProtections`, `minimumTests`, `disallowedFilePatterns`.
- The daemon has `.kaptaindignore`, test hooks, and staging rules, but these are local per-repo settings.
- **No centralized policy enforcement** linking the `Policy` table to the daemon scheduler was found.
- **Conclusion:** Schema supports policies; centralized enforcement does not yet exist.

## Overall Assessment

| Claim | Implementation | Schema | Enforcement | Result |
|-------|---------------|--------|-------------|--------|
| SSO/SAML/OIDC | ❌ | ✅ | ❌ | PARTIAL |
| RBAC | ❌ | ✅ | ❌ | PARTIAL |
| Immutable Audit Trails | ❌ | ✅ | ❌ | PARTIAL |
| Custom Retention | ❌ | ✅ | ❌ | PARTIAL |
| Enterprise Self-Hosting | ⚠️ | ✅ | ❌ | PARTIAL |
| Advanced Policy Engine | ❌ | ✅ | ❌ | PARTIAL |

## Conclusion
All six enterprise claims have **database schema and entitlement scaffolding** in place, but the **runtime enforcement, provider integrations, and automated workflows are incomplete**. The claims on the landing page should be qualified to reflect that these are roadmap features rather than shipping capabilities.

**Recommendation:** Add "Coming Soon" or "Beta" badges to enterprise features, or restrict the claims to what is demonstrably implemented today.
