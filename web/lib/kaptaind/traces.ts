
import fs from "fs/promises";
import path from "path";
import { kaptaindDir } from "./reader";
import type { TraceRecord } from "@/types/kaptaind";

export async function readTracesForAoc(
  repoPath: string,
  aocId: string,
  limit = 100
): Promise<TraceRecord[]> {
  const dir = path.join(kaptaindDir(repoPath), "traces");
  let entries: string[];

  try {
    entries = await fs.readdir(dir);
  } catch {
    return [];
  }

  const jsonFiles = entries.filter((f) => f.endsWith(".json"));
  const traces: TraceRecord[] = [];

  for (const file of jsonFiles) {
    try {
      const raw = await fs.readFile(path.join(dir, file), "utf-8");
      const trace = JSON.parse(raw) as TraceRecord;
      if (trace.aoc_id === aocId) {
        traces.push(trace);
      }
    } catch {
      // skip malformed files
    }
  }

  traces.sort((a, b) => (a.started_at < b.started_at ? 1 : -1));
  return traces.slice(0, limit);
}
