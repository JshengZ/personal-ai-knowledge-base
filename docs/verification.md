# Verification record

[English](verification.md) · [简体中文](verification.zh-CN.md) · [Home](../README.md)

Initial verification date: **2026-11-25**. Version **1.0.0**. Local build host: Intel macOS 14.6.1. Targets: Intel Mac and Windows x64. All fixtures are synthetic and isolated from real personal work folders.

## Completed local checks

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
- Maximum file setting: 100 MB; backup upload: 512 MB; restored database: 1 GiB. Large OCR pages have resource limits.
- No automatic online updates. Quit the old tray application before replacing it; per-user knowledge remains in its data directory.

Maintainers can run `node --test tests/packaged.test.mjs`; `PKB_PACKAGED_APP` selects a particular executable. Package audits use `node scripts/verify-package.mjs mac` or `win`. Shutdown checks allow the background process to finish asynchronous cleanup.

Release assets include `SHA256SUMS.txt` and `BUILD-INFO.json`. Publishing source and generic installers does not publish a user's database, work files, credentials, or backups.
