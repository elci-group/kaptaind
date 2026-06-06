import Stripe from "stripe";

let _stripe: Stripe | null = null;

export function getStripe(): Stripe {
  if (!_stripe) {
    _stripe = new Stripe(process.env.STRIPE_SECRET_KEY || "", {
      apiVersion: "2024-04-10",
    });
  }
  return _stripe;
}

export function validateStripeEnv(): void {
  if (!process.env.STRIPE_SECRET_KEY) {
    throw new Error("STRIPE_SECRET_KEY is not set");
  }
  if (!process.env.STRIPE_WEBHOOK_SECRET) {
    throw new Error("STRIPE_WEBHOOK_SECRET is not set");
  }
}

export function getStripePriceId(tier?: string): string | undefined {
  switch (tier?.toLowerCase()) {
    case "pro":
      return process.env.STRIPE_PRICE_ID_PRO;
    case "team":
      return process.env.STRIPE_PRICE_ID_TEAM;
    case "enterprise":
      return process.env.STRIPE_PRICE_ID_ENTERPRISE;
    default:
      return undefined;
  }
}
