import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { defineConfig } from "vite";
import react from "@vitejs/plugin-react";

export default defineConfig({
  plugins: [
    react(),
    {
      name: "knowledge-local-service",
      configureServer(server) {
        server.middlewares.use("/api", async (request, response) => {
          const origin = request.headers.origin;
          if (
            origin &&
            !/^http:\/\/(127\.0\.0\.1|localhost):\d+$/.test(origin)
          ) {
            response.writeHead(403).end();
            return;
          }
          try {
            const directory = process.env.PKB_HOME || resolve(".local/dev");
            const service = JSON.parse(
              readFileSync(resolve(directory, "service.json"), "utf8"),
            );
            const chunks: Buffer[] = [];
            for await (const chunk of request) chunks.push(Buffer.from(chunk));
            const body = chunks.length ? Buffer.concat(chunks) : undefined;
            const upstream = await fetch(
              `http://127.0.0.1:${service.port}/api${request.url}`,
              {
                method: request.method,
                headers: {
                  Authorization: `Bearer ${service.token}`,
                  "Content-Type":
                    request.headers["content-type"] || "application/json",
                },
                body,
              },
            );
            response.writeHead(upstream.status, {
              "Content-Type":
                upstream.headers.get("content-type") || "application/json",
              "Cache-Control": "no-store",
            });
            response.end(Buffer.from(await upstream.arrayBuffer()));
          } catch {
            response.writeHead(503, { "Content-Type": "application/json" });
            response.end(
              JSON.stringify({ error: "知识服务尚未就绪，请稍后重试。" }),
            );
          }
        });
      },
    },
  ],
  server: {
    host: "127.0.0.1",
    port: 5173,
    strictPort: true,
    fs: {
      deny: [
        "**/.local/**",
        "**/*.sqlite*",
        "**/service.json",
        "**/runtime.json",
        "**/.env*",
        "**/.git/**",
      ],
    },
  },
  build: { outDir: "dist", sourcemap: false },
});
