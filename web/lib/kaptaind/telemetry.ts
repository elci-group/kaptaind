
import fs from "fs/promises";
import path from "path";
import { kaptaindDir } from "./reader";
import type { TokenMetrics } from "@/types/kaptaind";

export async function readTelemetry(repoPath: string): Promise<TokenMetrics> {
  const filePath = path.join(kaptaindDir(repoPath), "telemetry.json");

  try {
    const raw = await fs.readFile(filePath, "utf-8");
    return JSON.parse(raw) as TokenMetrics;
  } catch {
    return {
      input_tokens: 0,
      output_tokens: 0,
      marginal_cost: 0,
      aggregate_cost: 0,
    };
  }
}
