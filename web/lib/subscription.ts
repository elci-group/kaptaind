import { prisma } from "@/lib/prisma";

export type SubscriptionTier = "free" | "pro";

export async function getUserTier(userId: string): Promise<SubscriptionTier> {
  const sub = await prisma.subscription.findUnique({
    where: { userId },
  });

  if (!sub || sub.tier !== "pro" || sub.status !== "active") return "free";

  if (sub.currentPeriodEnd && sub.currentPeriodEnd < new Date()) return "free";

  return "pro";
}

export async function requirePro(userId: string): Promise<void> {
  const tier = await getUserTier(userId);
  if (tier !== "pro") {
    throw new Error("Pro subscription required");
  }
}
