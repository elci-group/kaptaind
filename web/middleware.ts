import { getToken } from "next-auth/jwt";
import { NextResponse, type NextRequest } from "next/server";

const PUBLIC_PATHS = new Set([
  "/",
  "/pricing",
  "/platform",
  "/security",
  "/enterprise",
  "/docs",
  "/download",
]);

const PUBLIC_PATTERNS = [
  /^\/auth\//,
  /^\/whitepapers\//,
  /^\/case-studies\//,
  /^\/compare\//,
  /^\/api\/auth\//,
  /^\/api\/billing\/webhook/,
];

function isPublic(pathname: string): boolean {
  if (PUBLIC_PATHS.has(pathname)) return true;
  return PUBLIC_PATTERNS.some((pattern) => pattern.test(pathname));
}

export async function middleware(req: NextRequest) {
  const requestId = crypto.randomUUID();
  const { pathname } = req.nextUrl;

  if (isPublic(pathname)) {
    const res = NextResponse.next();
    res.headers.set("x-request-id", requestId);
    return res;
  }

  const token = await getToken({ req, secret: process.env.NEXTAUTH_SECRET });
  if (!token) {
    const signInUrl = new URL("/auth/signin", req.url);
    signInUrl.searchParams.set("callbackUrl", pathname);
    const res = NextResponse.redirect(signInUrl);
    res.headers.set("x-request-id", requestId);
    return res;
  }

  const res = NextResponse.next();
  res.headers.set("x-request-id", requestId);
  return res;
}

export const config = {
  matcher: ["/((?!_next/static|_next/image|favicon.ico|.*\\.).*)"],
};
