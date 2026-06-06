import { NextResponse, type NextRequest } from "next/server";
import { getServerSession } from "next-auth";
import { authOptions } from "./auth";
import { requireEntitlement } from "./entitlements";
import { prisma } from "./prisma";
import { projectAccessWhere } from "./kaptaind/access";

export interface PolicyContext {
  userId: string;
  orgId?: string;
  projectId?: string;
}

/**
 * Checks whether the user/org has the canUsePolicyPacks entitlement.
 * Throws if not entitled.
 */
export async function requirePolicyPacks(ctx: PolicyContext): Promise<void> {
  await requireEntitlement("canUsePolicyPacks", {
    userId: ctx.userId,
    orgId: ctx.orgId,
  });
}

/**
 * Route helper for API handlers: validates session, optional project access,
 * and enforces the canUsePolicyPacks entitlement.
 */
export async function withPolicyAccess(
  req: NextRequest,
  handler: (ctx: { userId: string; orgId?: string; projectId?: string }) => Promise<NextResponse>
): Promise<NextResponse> {
  const session = await getServerSession(authOptions);
  if (!session?.user?.id) {
    return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
  }

  const userId = session.user.id;
  const { searchParams } = new URL(req.url);
  const projectId = searchParams.get("projectId") || undefined;
  let orgId: string | undefined;

  if (projectId) {
    const project = await prisma.project.findFirst({
      where: projectAccessWhere(projectId, userId),
      select: { id: true, orgId: true },
    });
    if (!project) {
      return NextResponse.json({ error: "Forbidden" }, { status: 403 });
    }
    orgId = project.orgId || undefined;
  }

  try {
    await requirePolicyPacks({ userId, orgId, projectId });
  } catch {
    return NextResponse.json(
      { error: "Policy packs are not enabled for this plan" },
      { status: 403 }
    );
  }

  return handler({ userId, orgId, projectId });
}
