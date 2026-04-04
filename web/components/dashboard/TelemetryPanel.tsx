import Card, { CardHeader, CardTitle } from "@/components/ui/Card";
import type { TokenMetrics } from "@/types/kaptaind";

export default function TelemetryPanel({
  telemetry,
}: {
  telemetry: TokenMetrics;
}) {
  return (
    <Card>
      <CardHeader>
        <CardTitle>Token Telemetry</CardTitle>
      </CardHeader>
      <div className="grid grid-cols-2 gap-4">
        <div>
          <p className="text-xs text-zinc-500 dark:text-zinc-400">
            Input Tokens
          </p>
          <p className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
            {telemetry.input_tokens.toLocaleString()}
          </p>
        </div>
        <div>
          <p className="text-xs text-zinc-500 dark:text-zinc-400">
            Output Tokens
          </p>
          <p className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
            {telemetry.output_tokens.toLocaleString()}
          </p>
        </div>
        <div>
          <p className="text-xs text-zinc-500 dark:text-zinc-400">
            Last Cost
          </p>
          <p className="text-xl font-semibold text-zinc-900 dark:text-zinc-100">
            ${telemetry.marginal_cost.toFixed(4)}
          </p>
        </div>
        <div>
          <p className="text-xs text-zinc-500 dark:text-zinc-400">
            Total Cost
          </p>
          <p className="text-xl font-semibold text-violet-600">
            ${telemetry.aggregate_cost.toFixed(4)}
          </p>
        </div>
      </div>
    </Card>
  );
}
