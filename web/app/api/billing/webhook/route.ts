import { NextResponse } from "next/server";
import { Prisma } from "@prisma/client";
import { getStripe } from "@/lib/stripe";
import { prisma } from "@/lib/prisma";
import type Stripe from "stripe";

export const config = {
  api: {
    bodyParser: false,
  },
};

function mapStripeStatus(status: string): string {
  switch (status) {
    case "active":
      return "ACTIVE";
    case "past_due":
      return "PAST_DUE";
    case "canceled":
      return "CANCELED";
    case "trialing":
      return "TRIALING";
    default:
      return status.toUpperCase();
  }
}

function tierFromPriceId(priceId: string | null | undefined): string | null {
  if (!priceId) return null;
  if (priceId === process.env.STRIPE_PRICE_ID_PRO) return "pro";
  if (priceId === process.env.STRIPE_PRICE_ID_TEAM) return "team";
  if (priceId === process.env.STRIPE_PRICE_ID_ENTERPRISE) return "enterprise";
  return null;
}

export async function POST(req: Request) {
  const rawBody = await req.text();
  const sig = req.headers.get("stripe-signature") || "";

  let event: Stripe.Event;

  try {
    // Tolerance of 300s rejects replayed events with stale timestamps.
    event = getStripe().webhooks.constructEvent(
      rawBody,
      sig,
      process.env.STRIPE_WEBHOOK_SECRET || "",
      300
    );
  } catch (err) {
    const message = err instanceof Error ? err.message : "Invalid signature";
    return NextResponse.json({ error: message }, { status: 400 });
  }

  try {
    // Idempotency: record the event id first. A unique-constraint violation
    // (P2002) means we already processed this event, so acknowledge & skip.
    try {
      await prisma.processedWebhookEvent.create({
        data: { eventId: event.id },
      });
    } catch (idempotencyError) {
      if (
        idempotencyError instanceof Prisma.PrismaClientKnownRequestError &&
        idempotencyError.code === "P2002"
      ) {
        return NextResponse.json({ received: true });
      }
      throw idempotencyError;
    }

    switch (event.type) {
      case "checkout.session.completed": {
        const session = event.data.object as Stripe.Checkout.Session;
        const stripeSubscriptionId =
          typeof session.subscription === "string"
            ? session.subscription
            : null;
        const stripeCustomerId =
          typeof session.customer === "string" ? session.customer : null;

        if (!stripeSubscriptionId || !stripeCustomerId) {
          break;
        }

        const existing = await prisma.billingSubscription.findUnique({
          where: { stripeSubscriptionId },
        });

        if (existing) {
          break;
        }

        const billingCustomer = await prisma.billingCustomer.findUnique({
          where: { stripeCustomerId },
        });

        if (!billingCustomer) {
          break;
        }

        const tier = session.metadata?.tier || "pro";
        const plan = await prisma.plan.findUnique({
          where: { code: tier.toLowerCase() },
        });

        if (!plan) {
          break;
        }

        await prisma.billingSubscription.create({
          data: {
            customerId: billingCustomer.id,
            planId: plan.id,
            stripeSubscriptionId,
            status: "ACTIVE",
            currentPeriodStart: new Date(),
          },
        });

        // Update legacy Subscription if it exists
        const legacy = billingCustomer.userId
          ? await prisma.subscription.findUnique({
              where: { userId: billingCustomer.userId },
            })
          : null;

        if (legacy && billingCustomer.userId) {
          await prisma.subscription.update({
            where: { userId: billingCustomer.userId },
            data: {
              tier,
              status: "active",
              stripeSubscriptionId,
              stripeCustomerId,
            },
          });
        }

        break;
      }

      case "invoice.payment_succeeded": {
        const invoice = event.data.object as Stripe.Invoice & {
          subscription?: string;
        };
        const subscriptionId = invoice.subscription || null;

        if (!subscriptionId) break;

        const subscription =
          await getStripe().subscriptions.retrieve(subscriptionId);
        const currentPeriodEnd = new Date(
          (subscription as unknown as { current_period_end: number }).current_period_end * 1000
        );

        await prisma.billingSubscription.updateMany({
          where: { stripeSubscriptionId: subscriptionId },
          data: {
            status: "ACTIVE",
            currentPeriodEnd,
          },
        });

        break;
      }

      case "invoice.payment_failed": {
        const invoice = event.data.object as Stripe.Invoice & {
          subscription?: string;
        };
        const subscriptionId = invoice.subscription || null;

        if (!subscriptionId) break;

        await prisma.billingSubscription.updateMany({
          where: { stripeSubscriptionId: subscriptionId },
          data: { status: "PAST_DUE" },
        });

        break;
      }

      case "customer.subscription.deleted": {
        const subscription = event.data.object as Stripe.Subscription;

        await prisma.billingSubscription.updateMany({
          where: { stripeSubscriptionId: subscription.id },
          data: { status: "CANCELED" },
        });

        break;
      }

      case "customer.subscription.updated": {
        const subscription = event.data.object as Stripe.Subscription;
        const periodEnd = (
          subscription as unknown as { current_period_end?: number }
        ).current_period_end;
        const priceId = subscription.items?.data?.[0]?.price?.id ?? null;
        const tier = tierFromPriceId(priceId);
        const plan = tier
          ? await prisma.plan.findUnique({ where: { code: tier } })
          : null;

        await prisma.billingSubscription.updateMany({
          where: { stripeSubscriptionId: subscription.id },
          data: {
            status: mapStripeStatus(subscription.status),
            ...(periodEnd
              ? { currentPeriodEnd: new Date(periodEnd * 1000) }
              : {}),
            ...(plan ? { planId: plan.id } : {}),
          },
        });

        break;
      }
    }

    return NextResponse.json({ received: true });
  } catch (err) {
    const message =
      err instanceof Error ? err.message : "Webhook handler failed";
    return NextResponse.json({ error: message }, { status: 500 });
  }
}
