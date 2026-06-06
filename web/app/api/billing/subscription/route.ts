import { NextResponse } from "next/server";
import { getServerSession } from "next-auth/next";
import { authOptions } from "@/lib/auth";
import { prisma } from "@/lib/prisma";

export async function GET() {
  try {
    const session = await getServerSession(authOptions);
    if (!session?.user?.id) {
      return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
    }

    const billingCustomer = await prisma.billingCustomer.findUnique({
      where: { userId: session.user.id },
      include: {
        subscriptions: {
          orderBy: { createdAt: "desc" },
          take: 1,
          include: { plan: true },
        },
      },
    });

    if (billingCustomer?.subscriptions[0]) {
      const sub = billingCustomer.subscriptions[0];
      return NextResponse.json({
        subscription: {
          tier: sub.plan.code,
          status: sub.status,
          currentPeriodEnd: sub.currentPeriodEnd?.toISOString() || null,
          cancelAtPeriodEnd: false,
        },
      });
    }

    // Fallback to legacy Subscription table
    const legacy = await prisma.subscription.findUnique({
      where: { userId: session.user.id },
    });

    if (legacy) {
      return NextResponse.json({
        subscription: {
          tier: legacy.tier,
          status: legacy.status,
          currentPeriodEnd: legacy.currentPeriodEnd?.toISOString() || null,
          cancelAtPeriodEnd: false,
        },
      });
    }

    return NextResponse.json({ subscription: null });
  } catch (error) {
    const message =
      error instanceof Error ? error.message : "Unknown error occurred";
    return NextResponse.json({ error: message }, { status: 500 });
  }
}
