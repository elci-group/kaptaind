import { getServerSession } from "next-auth";
import { authOptions } from "./auth";
import { prisma } from "./prisma";
import { projectAccessWhere } from "./kaptaind/access";
import { getEntitlements, type Entitlements } from "./entitlements";
import type { Session } from "next-auth";

export class ApiAuthError extends Error {
  constructor(message: string, public status: number) {
    super(message);
  }
}

export function isAuthError(
  error: unknown
): { status: number; message: string } | null {
  if (error instanceof ApiAuthError) {
    return { status: error.status, message: error.message };
  }
  if (error instanceof Error) {
    if (error.message === "Unauthorized") {
      return { status: 401, message: error.message };
    }
    if (
      error.message === "Forbidden" ||
      error.message.startsWith("Forbidden:")
    ) {
      return { status: 403, message: error.message };
    }
  }
  return null;
}

export async function requireAuth(req: Request): Promise<Session> {
  void req;
  const session = await getServerSession(authOptions);
  if (!session?.user?.id) {
    throw new ApiAuthError("Unauthorized", 401);
  }
  return session;
}

export async function requireProjectAccess(
  req: Request,
  projectId: string
): Promise<{ id: string; orgId: string | null }> {
  void req;
  const session = await getServerSession(authOptions);
  if (!session?.user?.id) {
    throw new ApiAuthError("Unauthorized", 401);
  }
  const project = await prisma.project.findFirst({
    where: projectAccessWhere(projectId, session.user.id),
    select: { id: true, orgId: true },
  });
  if (!project) {
    throw new ApiAuthError("Forbidden", 403);
  }
  return project;
}

export async function requireEntitlement(
  req: Request,
  feature: keyof Entitlements
): Promise<void> {
  void req;
  const session = await getServerSession(authOptions);
  if (!session?.user?.id) {
    throw new ApiAuthError("Unauthorized", 401);
  }
  const entitlements = await getEntitlements({ userId: session.user.id });
  if (!entitlements[feature]) {
    throw new ApiAuthError(
      `Forbidden: Entitlement '${feature}' is required.`,
      403
    );
  }
}

export async function getUserTier(userId: string): Promise<string> {
  const legacySub = await prisma.subscription.findUnique({
    where: { userId },
    select: { tier: true },
  });
  if (legacySub) {
    return legacySub.tier.toLowerCase();
  }

  const customer = await prisma.billingCustomer.findUnique({
    where: { userId },
    include: {
      subscriptions: {
        where: { status: "ACTIVE" },
        include: { plan: true },
      },
    },
  });
  if (customer && customer.subscriptions.length > 0) {
    return customer.subscriptions[0].plan.code.toLowerCase();
  }

  return "free";
}
