import { sql } from "drizzle-orm";
import { integer, sqliteTable, text } from "drizzle-orm/sqlite-core";

/** Preorder-interest signups for dongle-lite (and whatever hardware comes next). */
export const reservations = sqliteTable("reservations", {
  id: integer("id").primaryKey({ autoIncrement: true }),
  email: text("email").notNull().unique(),
  product: text("product").notNull().default("dongle-lite"),
  createdAt: integer("created_at")
    .notNull()
    .default(sql`(unixepoch())`),
});
