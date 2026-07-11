import { redirect } from "next/navigation";
import { cookies } from "next/headers";
import { getServerSession } from "next-auth";
import { authOptions } from "@/lib/auth";
import { getUserTier } from "@/lib/api-auth";
import Sidebar from "@/components/layout/Sidebar";

export const dynamic = "force-dynamic";

export default async function DashboardLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  // Force cookie access so Next.js treats this as dynamic
  const cookieStore = await cookies();
  const hasToken = cookieStore.has("next-auth.session-token");

  console.log("[dashboard] hasToken:", hasToken);

  if (!hasToken) {
    redirect("/auth/signin");
  }

  const session = await getServerSession(authOptions);
  console.log("[dashboard] session:", JSON.stringify(session));

  if (!session?.user?.id) {
    redirect("/auth/signin");
  }

  const tier = await getUserTier(session.user.id);

  return (
    <div className="flex h-screen overflow-hidden">
      <Sidebar tier={tier} />
      <main className="flex-1 overflow-y-auto bg-zinc-100 dark:bg-zinc-900">
        {children}
      </main>
    </div>
  );
}
