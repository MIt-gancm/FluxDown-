/** Public sponsor-wall issue (each comment is one wall entry). */
export const SPONSOR_WALL_URL = "https://github.com/zerx-lab/FluxDown/issues/3";

/** Entry from GET /api/sponsor/list (newest first). */
export interface WallSponsor {
  name: string;
  avatar: string | null;
  amountCents: number;
  date: string;
  message: string | null;
}

/** Fen → "¥66" / "¥6.60". */
export function fmtCents(cents: number): string {
  const yuan = cents / 100;
  return Number.isInteger(yuan) ? `¥${yuan}` : `¥${yuan.toFixed(2)}`;
}

/** "2024-05-01" → "2024.05.01"; other formats pass through. */
export function fmtWallDate(date: string): string {
  return /^\d{4}-\d{2}-\d{2}$/.test(date) ? date.replace(/-/g, ".") : date;
}
