# Verification record

[English](verification.md) · [简体中文](verification.zh-CN.md) · [Home](../README.md)

Initial verification date: **2026-11-25**. Version **1.0.0**. Local build host: Intel macOS 14.6.1. Targets: Intel Mac and Windows x64. All fixtures are synthetic and isolated from real personal work folders.

## 1.0.1 local regression results (2026-11-26)

Added cases exercise stdio initialize/tool discovery with an unresponsive backend; HTTP/stdio and full-text fallback with one async worker and deliberately slow embeddings; HTTP handshakes while the database writer is held; old MCP configurations after migration; cancellation of long parsing; and Windows worker cleanup after forced termination. Desktop GUI tests cover limits, migration/restart, removal of the old database, preservation of originals and resumed filesystem watching, with wide/narrow screenshots.

Additional cases cover local auto budgets, continued department-API/explicit-CPU support, capacity thresholds, incremental counters, long-line overlap, paragraph grouping and project detection. The packaged local-model flow includes longer Chinese chunks. Consult Actions and the execution record for the exact source commit. Synthetic tests are not a million-vector performance benchmark; no private organization corpus is uploaded for validation.

### Completed locally for 1.0.1

| Check | Result |
| --- | --- |
| TypeScript / Rust | Type checks and production builds pass; unused frontend declarations checked. |
| Engine | 16 unit cases pass, including appearance validation/persistence, counters, disk limits, migrations and busy-database MCP handshakes. |
| Parsers | 14 cases pass, including local OCR, short-paragraph grouping and bounded long-line overlap. |
| Integration | Real watching, compatible model APIs, standard SDK stdio/HTTP MCP, scopes, memory review and backup/restore pass. The sample generator is removed. |
| Resilience | 3 cases pass on Mac: hung-backend stdio handshake, slow-embedding responsiveness/FTS/migration, and parser cancellation. The forced-termination Job Object case is Windows-only. |
| Appearance and storage GUI | All appearance options, custom color input, system theme, reset, unsaved provider fields, actual migration/restart, source preservation and watching pass. |
| Design / accessibility | All main pages and settings groups checked at 1536/1024/800/390 px, including 18 px base text, dialogs, active navigation and visible save-failure/retry recovery. No related console/page errors. See [design QA](../design-qa.md). |
| Full Mac package | Five synthetic documents, two projects, six real local vectors, PDF, image OCR, XLSX, citations, MCP and memory review pass without an external model. |
| Windows package | x64 engine cross-build and assisted NSIS package complete. Both platform audits find only target resources, the pinned model digest, no personal data and no bundled credentials. Cross-building does not replace native Windows acceptance. |

Hosted matrix tests include the Windows-only worker-cleanup case and native Electron appearance/migration flow. Check [Actions](https://github.com/JshengZ/personal-ai-knowledge-base/actions) for the exact commit/run; a configured check is not a passing run. No real company API credentials or documents were used.

## Historical 1.0.0 checks

| Check | Evidence |
| --- | --- |
| Static/build checks | TypeScript check, Rust check, frontend build, desktop build, and both target engine builds passed. |
| Engine unit tests | 7 passing cases covering stale reads, deletion/invalidation, live revocation, local-only project isolation, model/dimension separation, backup handling, and preserving ZCode settings. |
| Parser tests | 10 passing cases: code locations, DOCX, safe XML character decoding, PPTX order/notes, XLSX cached formulas/cells, multiline CSV, PDF pages, notebooks, persistent worker recovery, and actual scanned-PDF OCR. |
| Integration | Real file creation/modification/rename/deletion, compatible model API protocols, official SDK stdio/HTTP MCP, permission changes, memory review/staleness, backup/restore, canonical sample paths. |
| Real local retrieval | Bundled Qwen3-Embedding-0.6B, 1024 dimensions, real vector indexing and hybrid retrieval without an external model API. |
| Packaged Mac workflow | Native folder wizard, two projects, five real fixtures, PDF/PNG OCR/XLSX/text, local embeddings, workspace filtering, actual MCP client calls, memory proposals/confirmation/deletion, and asynchronous backend exit. |
| UI | Project editing, source reading, search, synchronization, memories, connections, and model settings; 1488/1024/390 px layout checks; zero relevant console errors after fixing favicon. See [design QA](../design-qa.md). |
| Windows binaries | Engine cross-built for x64 GNU; llama.cpp statically built from pinned source; imports do not require VCRUNTIME/MSVCP or MinGW dynamic runtimes. |
| Package audits | Only target platform engines/native modules; pinned model digest; no knowledge database, backup, owner token, agent token, or API key; blank generic organization template. |

There are 19 local automated test cases/workflows across the engine, parser, integration, and packaged smoke suites. Hosted CI results are independently visible in [Actions](https://github.com/JshengZ/personal-ai-knowledge-base/actions); adding a workflow alone is not a passing run.

Provider integration uses a synthetic local OpenAI-compatible API, not a real organization's model deployment. ZCode config tests use temporary paths and do not prove behavior in an actual installed ZCode client. Native API-key storage with organization credentials is not claimed as verified here.

## Pending environment acceptance

1. Install and exercise the Windows GUI, tray, startup, directory picker, watchers, PDF/OCR, and local retrieval on a target Windows workstation. ARM Windows is not included.
2. Test the real organization API URL, deployed models, credentials, custom paths, network, and enterprise certificate trust in the app.
3. Run an actual ZCode session after configuration and check `get_context/search/read_source` calls and citations for that client version.
4. Configure a real signing identity and, for macOS, notarization before making claims about signed distribution. The initial installers are unsigned.

Windows CI backend tests, when successful, are useful additional evidence but do not replace target-workstation or real-client acceptance.

## Current boundaries

- Explicitly selected local folders only; no full-disk crawl, online document connectors, audio/video transcription, or model training.
- Inferred experience/roles are reviewable suggestions. An agent is not guaranteed to call MCP for every message.
- Exact cosine search is used for personal collections; small-fixture timing is not a large-scale benchmark.
- Maximum file setting: 32 MB; backup upload: 512 MB; restored database: 1 GiB. Large OCR pages have resource limits.
- No automatic online updates. Quit the old tray application before replacing it; per-user knowledge remains in its data directory.

Maintainers can run `node --test tests/packaged.test.mjs`; `PKB_PACKAGED_APP` selects a particular executable. Package audits use `node scripts/verify-package.mjs mac` or `win`. Shutdown checks allow the background process to finish asynchronous cleanup.

Release assets include `SHA256SUMS.txt` and `BUILD-INFO.json`. Publishing source and generic installers does not publish a user's database, work files, credentials, or backups.
