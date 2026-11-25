# Contributing / 贡献指南

## English

Bug reports and focused pull requests are welcome. Check existing issues first, describe the expected behavior, and use synthetic sample documents for reproduction.

1. Fork the repository and create a branch from `main`.
2. Follow the [development guide](docs/development.md). Use the lockfiles and pinned Rust toolchain.
3. Keep parsing, storage, model adapters, retrieval, MCP transport, and UI responsibilities in their existing modules.
4. Run `npm run check`, `npm test`, and `npm run test:integration` for relevant changes. Packaging changes should also pass the package audit and packaged-app smoke test on a supported platform.
5. Update both languages when changing documented user behavior. Keep the release status honest about what was actually tested.
6. Open a pull request describing the problem, resulting behavior, and meaningful validation. Screenshots help for visible UI changes.

Do not commit original work documents, organization endpoints, databases, `.pkb` backups, API keys, MCP tokens, signing certificates, or build output. Do not include private source contents in issue logs. Use [private vulnerability reporting](SECURITY.md) for security issues.

By contributing, you agree that your contributions may be distributed under the project's Apache-2.0 license. Preserve third-party licenses when adding dependencies. No CLA is currently required.

## 简体中文

欢迎提交问题和范围清晰的 PR。先查看已有 Issue，说明预期行为，使用虚构示例复现。

1. Fork 后从 `main` 创建分支，按[开发指南](docs/development.zh-CN.md)使用锁文件和固定 Rust 工具链。
2. 遵循现有模块边界，不把厂商协议写进 UI，也不绕过目录、项目或来源版本校验。
3. 运行相关类型检查、单元测试和端到端测试；打包变更需检查包内容及打包后应用。
4. 文档行为变更同步更新中英文，明确实际验证的平台和接口。
5. PR 说明问题、最终行为和验证证据，界面变更附截图。

不要提交真实工作资料、公司端点、数据库、备份、Key、Agent 令牌、签名证书或编译产物。日志中的路径和资料内容先脱敏。漏洞使用[私密报告](SECURITY.md)，不要直接公开细节。

贡献按 Apache-2.0 分发，第三方依赖保留原许可证；目前不要求 CLA。
