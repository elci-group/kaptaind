import { prisma } from "@/lib/prisma";

export interface Entitlements {
  canUseAi: boolean;
  maxRepos: number;
  maxUsers: number;
  retentionDays: number;
  canUseSso: boolean;
  canUsePolicyPacks: boolean;
  canExportAuditLogs: boolean;
}

export const DEFAULT_ENTITLEMENTS: Record<string, Entitlements> = {
  free: {
    canUseAi: false,
    maxRepos: 1,
    maxUsers: 1,
    retentionDays: 0,
    canUseSso: false,
    canUsePolicyPacks: false,
    canExportAuditLogs: false,
  },
  pro: {
    canUseAi: true,
    maxRepos: 10,
    maxUsers: 1,
    retentionDays: 30,
    canUseSso: false,
    canUsePolicyPacks: false,
    canExportAuditLogs: false,
  },
  team: {
    canUseAi: true,
    maxRepos: 50,
    maxUsers: 25,
    retentionDays: 180,
    canUseSso: false,
    canUsePolicyPacks: true,
    canExportAuditLogs: false,
  },
  enterprise: {
    canUseAi: true,
    maxRepos: 100000,
    maxUsers: 100000,
    retentionDays: 3650, // 10 years
    canUseSso: true,
    canUsePolicyPacks: true,
    canExportAuditLogs: true,
  },
};

/**
 * Resolves the active entitlements for a given user or organization.
 * Looks up the BillingSubscription, maps through Plan and its associated Entitlements.
 * Falls back to defaults based on the tier if not customized in the database.
 */
export async function getEntitlements(params: { userId?: string; orgId?: string }): Promise<Entitlements> {
  const { userId, orgId } = params;

  // 1. Try to find the billing customer and subscription
  let subscription = null;

  if (orgId) {
    const customer = await prisma.billingCustomer.findUnique({
      where: { orgId },
      include: {
        subscriptions: {
          where: { status: "ACTIVE" },
          include: {
            plan: {
              include: { entitlements: true },
            },
          },
        },
      },
    });
    if (customer && customer.subscriptions.length > 0) {
      subscription = customer.subscriptions[0];
    }
  }

  if (!subscription && userId) {
    const customer = await prisma.billingCustomer.findUnique({
      where: { userId },
      include: {
        subscriptions: {
          where: { status: "ACTIVE" },
          include: {
            plan: {
              include: { entitlements: true },
            },
          },
        },
      },
    });
    if (customer && customer.subscriptions.length > 0) {
      subscription = customer.subscriptions[0];
    }
  }

  // 2. Fallback to backward-compatible legacy Subscription model if new billing models aren't set up yet
  if (!subscription && userId) {
    const legacySub = await prisma.subscription.findUnique({
      where: { userId },
    });
    if (legacySub && legacySub.status === "active") {
      const tier = legacySub.tier.toLowerCase();
      if (DEFAULT_ENTITLEMENTS[tier]) {
        return DEFAULT_ENTITLEMENTS[tier];
      }
    }
  }

  // 3. If a subscription with a structured plan is found, compile its entitlements
  if (subscription && subscription.plan) {
    const planCode = subscription.plan.code.toLowerCase();
    const defaults = DEFAULT_ENTITLEMENTS[planCode] || DEFAULT_ENTITLEMENTS.free;
    const entitlements = { ...defaults };

    // Apply database-overridden entitlements if any
    for (const ent of subscription.plan.entitlements) {
      const key = ent.featureKey as keyof Entitlements;
      if (key in entitlements) {
        if (typeof entitlements[key] === "boolean") {
          (entitlements as Record<string, unknown>)[key] = ent.featureValue === "true";
        } else if (typeof entitlements[key] === "number") {
          (entitlements as Record<string, unknown>)[key] = parseInt(ent.featureValue, 10) || 0;
        }
      }
    }

    return entitlements;
  }

  // Default fallback is Free
  return DEFAULT_ENTITLEMENTS.free;
}

/**
 * Asserts that a user/org has a specific boolean entitlement.
 */
export async function requireEntitlement(
  key: keyof Omit<Entitlements, "maxRepos" | "maxUsers" | "retentionDays">,
  params: { userId?: string; orgId?: string }
): Promise<void> {
  const entitlements = await getEntitlements(params);
  if (!entitlements[key]) {
    throw new Error(`Forbidden: Entitlement '${key}' is required.`);
  }
}
