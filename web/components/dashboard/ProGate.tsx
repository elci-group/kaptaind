import Link from "next/link";
import Card from "@/components/ui/Card";

export default function ProGate({ feature }: { feature: string }) {
  return (
    <div className="flex flex-1 items-center justify-center p-8">
      <Card className="max-w-md text-center">
        <div className="mb-4 text-4xl">&#x1f512;</div>
        <h2 className="mb-2 text-xl font-semibold text-zinc-900 dark:text-zinc-100">
          {feature}
        </h2>
        <p className="mb-6 text-sm text-zinc-500 dark:text-zinc-400">
          This feature requires a Kaptaind Pro subscription. Upgrade to unlock
          AI-powered insights for your versioning workflow.
        </p>
        <Link
          href="/pricing"
          className="inline-flex items-center justify-center rounded-lg bg-violet-600 px-6 py-2.5 text-sm font-medium text-white hover:bg-violet-700"
        >
          Upgrade to Pro
        </Link>
      </Card>
    </div>
  );
}
