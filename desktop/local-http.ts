import { Agent, request as send } from "node:http";
// These requests only target the user's loopback engine. They must not inherit
// company HTTP proxies, even when Node's environment-proxy mode is enabled.
const agent = new Agent({ keepAlive: true, maxSockets: 16, proxyEnv: {} });
export function localFetch(
  address: string,
  options: RequestInit = {},
): Promise<Response> {
  const url = new URL(address);
  if (
    url.protocol !== "http:" ||
    !["127.0.0.1", "localhost", "[::1]"].includes(url.hostname)
  )
    return Promise.reject(new Error("本机服务地址无效"));
  return new Promise((resolve, reject) => {
    const request = send(
      url,
      {
        method: options.method || "GET",
        headers: Object.fromEntries(new Headers(options.headers)),
        agent,
        signal: options.signal || undefined,
      },
      (response) => {
        const chunks: Buffer[] = [];
        let bytes = 0;
        response.on("data", (chunk: Buffer) => {
          bytes += chunk.length;
          if (bytes > 512 * 1024 * 1024) {
            request.destroy(new Error("本机服务响应超过大小上限"));
            return;
          }
          chunks.push(chunk);
        });
        response.on("error", reject);
        response.on("end", () =>
          resolve(
            new Response(
              [204, 205, 304].includes(response.statusCode || 500)
                ? null
                : Buffer.concat(chunks),
              {
                status: response.statusCode || 500,
                headers: Object.fromEntries(
                  Object.entries(response.headers).filter(
                    (entry): entry is [string, string] =>
                      typeof entry[1] === "string",
                  ),
                ),
              },
            ),
          ),
        );
      },
    );
    request.on("error", reject);
    if (options.body !== undefined && options.body !== null)
      request.write(options.body);
    request.end();
  });
}
