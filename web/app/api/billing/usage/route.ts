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
    });

    if (!billingCustomer) {
      const now = new Date();
      return NextResponse.json({
        events: [],
        totalCost: 0,
        periodStart: new Date(now.getFullYear(), now.getMonth(), 1).toISOString(),
        periodEnd: new Date(
          now.getFullYear(),
          now.getMonth() + 1,
          0,
          23,
          59,
          59,
          999
        ).toISOString(),
      });
    }

    const now = new Date();
    const periodStart = new Date(now.getFullYear(), now.getMonth(), 1);
    const periodEnd = new Date(
      now.getFullYear(),
      now.getMonth() + 1,
      0,
      23,
      59,
      59,
      999
    );

    const events = await prisma.meteredUsageEvent.groupBy({
      by: ["meterName"],
      where: {
        customerId: billingCustomer.id,
        timestamp: { gte: periodStart },
      },
      _sum: { quantity: true },
    });

    const costPerUnit: Record<string, number> = {
      analysis_events: 0.01,
      automated_commits: 0.05,
      release_decisions: 0.1,
      AI_tokens: 0.002,
      trace_retention_gb_month: 0.5,
      active_repositories: 2.0,
    };

    let totalCost = 0;
    const formatted = events.map((e) => {
      const count = e._sum.quantity || 0;
      const cost =
        Math.round(count * (costPerUnit[e.meterName] || 0) * 100) / 100;
      totalCost += cost;
      return {
        feature: e.meterName,
        count,
        cost,
      };
    });

    totalCost = Math.round(totalCost * 100) / 100;

    return NextResponse.json({
      events: formatted,
      totalCost,
      periodStart: periodStart.toISOString(),
      periodEnd: periodEnd.toISOString(),
    });
  } catch (error) {
    const message =
      error instanceof Error ? error.message : "Unknown error occurred";
    return NextResponse.json({ error: message }, { status: 500 });
  }
}
