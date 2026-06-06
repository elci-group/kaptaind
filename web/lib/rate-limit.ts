import { LRUCache } from "lru-cache";

type LimitEntry = {
  count: number;
  resetAt: number;
};

const generalCache = new LRUCache<string, LimitEntry>({ max: 10000 });
const aiIpCache = new LRUCache<string, LimitEntry>({ max: 10000 });
const aiDailyCache = new LRUCache<string, LimitEntry>({ max: 10000 });

function getClientIp(req: Request): string {
  const forwarded = req.headers.get("x-forwarded-for");
  if (forwarded) {
    return forwarded.split(",")[0].trim();
  }
  return "unknown";
}

function checkLimit(
  cache: LRUCache<string, LimitEntry>,
  key: string,
  max: number,
  windowMs: number
): { allowed: boolean; retryAfter?: number } {
  const now = Date.now();
  const entry = cache.get(key);
  if (!entry || now >= entry.resetAt) {
    cache.set(key, { count: 1, resetAt: now + windowMs });
    return { allowed: true };
  }
  if (entry.count >= max) {
    return {
      allowed: false,
      retryAfter: Math.max(1, Math.ceil((entry.resetAt - now) / 1000)),
    };
  }
  entry.count++;
  cache.set(key, entry);
  return { allowed: true };
}

export async function rateLimit(
  req: Request,
  options: { type: "general" | "ai"; userId?: string; tier?: string }
): Promise<{ allowed: boolean; retryAfter?: number }> {
  const ip = getClientIp(req);

  if (options.type === "general") {
    return checkLimit(generalCache, `${ip}:general`, 100, 60000);
  }

  // AI per-IP limit: 10 requests / 60s
  const ipResult = checkLimit(aiIpCache, `${ip}:ai`, 10, 60000);
  if (!ipResult.allowed) {
    return ipResult;
  }

  // AI per-user daily limit
  if (options.userId && options.tier) {
    const dailyLimits: Record<string, number> = {
      pro: 50,
      team: 500,
      enterprise: Infinity,
    };
    const limit = dailyLimits[options.tier.toLowerCase()] ?? 50;
    if (limit !== Infinity) {
      const dailyResult = checkLimit(
        aiDailyCache,
        `${options.userId}:ai-daily`,
        limit,
        86400000
      );
      if (!dailyResult.allowed) {
        return dailyResult;
      }
    }
  }

  return { allowed: true };
}
