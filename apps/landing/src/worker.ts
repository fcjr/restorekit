import { drizzle } from "drizzle-orm/d1";
import { reservations } from "./db/schema";

// Serves the built landing page, 301s the apex domain to www, and collects
// dongle-lite preorder interest into D1.
export default {
  async fetch(request, env): Promise<Response> {
    const url = new URL(request.url);
    if (url.hostname === "restorekit.org") {
      url.hostname = "www.restorekit.org";
      return Response.redirect(url.toString(), 301);
    }
    if (url.pathname === "/api/reserve" && request.method === "POST") {
      return reserve(request, env);
    }
    return env.ASSETS.fetch(request);
  },
} satisfies ExportedHandler<Env>;

// Deliberately loose: just enough to catch typos, the unique index handles dupes.
const EMAIL = /^[^\s@]+@[^\s@]+\.[^\s@]{2,}$/;

async function reserve(request: Request, env: Env): Promise<Response> {
  let email = "";
  try {
    email = String(((await request.json()) as { email?: unknown }).email ?? "");
  } catch {
    // fall through to the validation error
  }
  email = email.trim().toLowerCase();
  if (email.length > 254 || !EMAIL.test(email)) {
    return Response.json({ error: "that doesn't look like an email address" }, { status: 400 });
  }
  await drizzle(env.DB).insert(reservations).values({ email }).onConflictDoNothing();
  return Response.json({ ok: true });
}
