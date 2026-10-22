export const json = (body: unknown, status = 200, headers: Record<string, string> = {}) =>
  new Response(JSON.stringify(body), {
    status,
    headers: { "Content-Type": "application/json", "Cache-Control": "no-store", ...headers },
  });

/** The OpenAI error shape, like OpenAI.`type` is what the app acts on. */
export const fail = (
  status: number,
  type: string,
  message = type.replace(/_/g, " "),
  extra: Record<string, unknown> = {},
  headers: Record<string, string> = {},
) => json({ error: { message, code: status, type, ...extra } }, status, headers);
