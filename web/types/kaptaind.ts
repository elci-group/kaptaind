export interface DiffAnalysis {
  structural: number;
  api: number;
  deps: number;
  runtime: number;
  api_breaking: boolean;
  api_added: boolean;
  touched_paths: number;
  api_touches: number;
  api_signatures: number;
  dependency_manifests: number;
  dependency_nodes: number;
  dependency_edges: number;
  runtime_paths: number;
}

export interface WeightResult {
  score: number;
  api_breaking: boolean;
  api_added: boolean;
}

export interface AnalysisArtifact {
  cluster_id: string;
  version: string;
  bump: "Major" | "Minor" | "Patch" | "None";
  event_count: number;
  started_at: string;
  ended_at: string;
  diff: DiffAnalysis;
  weight: WeightResult;
}

export type DaemonState = "Idle" | "Clustering" | "Testing" | "Committing" | "Failed";

export interface StatusReport {
  status: DaemonState;
  last_version: string | null;
  last_action_time: string;
  last_error: string | null;
}

export interface TokenMetrics {
  input_tokens: number;
  output_tokens: number;
  marginal_cost: number;
  aggregate_cost: number;
}

export interface AocSession {
  id: string;
  label: string;
  created_at: string;
  initial_version: string;
}

export interface AocManifest {
  id: string;
  label: string;
  created_at: string;
  shipped_at: string;
  initial_version: string;
  final_version: string;
  cluster_count: number;
  commit_count: number;
  test_failures: number;
  trace_ids: string[];
}

export interface TraceEvent {
  paths: string[];
  kind: "create" | "modify" | "remove" | "other";
  t: string;
}

export interface TraceTest {
  outcome: "passed" | "failed" | "skipped" | "none";
  stderr?: string;
}

export type TraceResult =
  | { type: "committed"; bump: string; version: string }
  | { type: "skipped"; reason: string };

export interface AgentEvent {
  id: string;
  timestamp: string;
  model?: string;
  input?: unknown;
  output?: unknown;
  tools: string[];
  latency_ms: number;
}

export interface TraceRecord {
  cluster_id: string;
  aoc_id: string;
  started_at: string;
  ended_at: string;
  duration_ms: number;
  events: TraceEvent[];
  test: TraceTest;
  result: TraceResult;
  analysis_ref?: string;
  agent_event?: AgentEvent;
}
