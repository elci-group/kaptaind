import { NextResponse } from "next/server";
import { getServerSession } from "next-auth/next";
import { authOptions } from "@/lib/auth";
import { prisma } from "@/lib/prisma";
import { getStripe } from "@/lib/stripe";

export async function POST(req: Request) {
  try {
    const session = await getServerSession(authOptions);
    if (!session?.user?.id) {
      return NextResponse.json({ error: "Unauthorized" }, { status: 401 });
    }

    const body = await req.json();
    const tier = (body.tier as string) || "pro";
    let priceId = (body.priceId as string) || "";

    if (!priceId) {
      switch (tier.toLowerCase()) {
        case "pro":
          priceId = process.env.STRIPE_PRICE_ID_PRO || "";
          break;
        case "team":
          priceId = process.env.STRIPE_PRICE_ID_TEAM || "";
          break;
        case "enterprise":
          priceId = process.env.STRIPE_PRICE_ID_ENTERPRISE || "";
          break;
      }
    }

    if (!priceId) {
      return NextResponse.json(
        { error: "Price ID is required" },
        { status: 400 }
      );
    }

    let billingCustomer = await prisma.billingCustomer.findUnique({
      where: { userId: session.user.id },
    });

    if (!billingCustomer) {
      const customer = await getStripe().customers.create({
        email: session.user.email,
        metadata: { userId: session.user.id },
      });

      billingCustomer = await prisma.billingCustomer.create({
        data: {
          stripeCustomerId: customer.id,
          userId: session.user.id,
        },
      });
    }

    const checkoutSession = await getStripe().checkout.sessions.create({
      customer: billingCustomer.stripeCustomerId,
      mode: "subscription",
      line_items: [{ price: priceId, quantity: 1 }],
      success_url: `${process.env.NEXT_PUBLIC_APP_URL}/dashboard?checkout=success`,
      cancel_url: `${process.env.NEXT_PUBLIC_APP_URL}/pricing`,
      metadata: {
        userId: session.user.id,
        tier,
      },
    });

    return NextResponse.json({ url: checkoutSession.url });
  } catch (error) {
    const message =
      error instanceof Error ? error.message : "Unknown error occurred";
    return NextResponse.json({ error: message }, { status: 500 });
  }
}
