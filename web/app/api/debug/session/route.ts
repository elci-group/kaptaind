import { NextResponse } from "next/server";
import { getServerSession } from "next-auth";
import { authOptions } from "@/lib/auth";

export async function GET() {
  console.log("[debug] testing getServerSession...");
  const session = await getServerSession(authOptions);
  console.log("[debug] session:", JSON.stringify(session));
  return NextResponse.json({
    session,
    hasUser: !!session?.user,
    hasId: !!session?.user?.id,
  });
}
