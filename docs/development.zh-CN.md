# 开发与发布

[English](development.md) · [简体中文](development.zh-CN.md) · [项目首页](../README.zh-CN.md)

## 环境

维护者需要 Node.js 24+、由 rustup 管理的 Rust（项目固定 1.100.0）和本机编译器。首发使用 Node 26.3.0。Mac 使用 Xcode Command Line Tools；Windows 使用 Visual Studio C++ 工具与 Windows SDK。

重建 Windows 检索运行时需要 CMake。Mac 交叉构建另需 Zig、cargo-zigbuild 和 Windows GNU Rust target。普通安装包用户不需要这些开发工具。

## 检查与启动

```sh
npm ci
npm run assets -- --small-only
npm run check
npm run build:web
npm run build:desktop
cargo build --locked --manifest-path engine/Cargo.toml
npm test
npm run test:integration
npm run test:resilience
npm run test:desktop
npm run dev
```

`--small-only` 只准备 OCR 和图标，不下载大模型。集成测试用本机模拟接口验证公司模型协议和 MCP；真实本地向量需要完整资源。

开发数据在忽略的 `.local/dev`，桌面开发启动使用 `.local/desktop`，与安装版用户数据库分开。Vite 仅绑定 `127.0.0.1:5173`，不把知识数据目录作为静态文件开放。

## Mac Intel 安装包

```sh
npm ci
npm run assets
npm run dist:mac
node scripts/verify-package.mjs mac
node --test tests/packaged.test.mjs
```

输出在 `release/mac-x64/`。当前首发准备了 Intel Mac 运行时；ARM64 需补充并验证对应运行文件后再分发。

## Windows 本机构建

在具备 CMake 和 Windows SDK 的 Visual Studio 开发环境运行：

```sh
npm ci
npm run assets
npm run assets:win-runtime
npm run dist:win
node scripts/verify-package.mjs win
node --test tests/packaged.test.mjs
```

脚本从固定校验和的 llama.cpp 源码构建静态 C++ 运行时，关闭 OpenSSL/OpenMP，使用基础 x64 CPU 设置。NSIS 安装包位于 `release/win-x64/`。

## Mac 交叉构建 Windows

先安装 Windows GNU target 与 cargo-zigbuild，将 `PKB_ZIG` 指向 Zig 可执行文件；CMake 不在 PATH 时设置 `PKB_CMAKE`：

```sh
rustup target add x86_64-pc-windows-gnu
cargo install cargo-zigbuild --locked
npm run assets
npm run assets:win-runtime
export CARGO_ZIGBUILD_ZIG_PATH="$PKB_ZIG"
npm run dist:win
node scripts/verify-package.mjs win
```

首发使用 Zig 0.17.1。交叉编译不等于 Windows 实际窗口验收，仍应在 Windows 运行打包后测试。

## 资源与组织模板

资源脚本校验上游归档和模型；目标 canvas 包按 npm 锁文件 integrity 校验。Windows 运行时有单独的构建清单。公开构建重映射本机编译路径。

包内容审计验证目标平台、模型校验和、原生模块、空白组织模板，检查没有知识数据库、备份和令牌。可选 Cargo 镜像示例在 `.cargo/config.example.toml`；Electron 镜像通过 `ELECTRON_MIRROR` 显式启用，均只影响构建下载。

公司专用包可用 `PKB_COMPANY_CONFIG` 指定组织 JSON 模板。仅允许组织名称、地址、模型、协议等字段，不打包 Key。不要把公司专用配置包作为通用公开版本上传。

## 版本发布

1. 同步更新 Node/Rust 版本与锁文件、中英文文档和更新日志。
2. 运行 CI 和目标平台打包后测试，按实际证据说明验证状态。
3. 重建安装包并通过两套包审计；需要签名时通过构建环境配置 `CSC_LINK` 等凭据，禁止提交证书和密码，Mac 公证另行验证。
4. 运行 `node scripts/release-info.mjs` 生成校验和与公开构建信息。
5. 创建 Git tag 和草稿 Release，仅上传安装包、`SHA256SUMS.txt`、`BUILD-INFO.json`；核对上传后的摘要再公开。
6. 检查仓库公开性和无需登录的下载。不要上传开发数据、备份、日志、签名材料或用户配置。

在 GitHub Actions 中先对 `main` 运行 **Build installers**。两端安装包、对应源码 CI 和密钥扫描全部通过后，运行 **Publish verified release**，将构建记录的数字 ID 填入 `installer_run`。流程在云端下载产物，核对源码、版本、大小与 SHA-256，创建草稿，上传并验证四份附件后公开。tag 指向实际受测的安装包源码；`BUILD-INFO.json` 另外记录发布工具的提交。发布前在 `docs/releases/v<版本>.md` 准备中英文说明。构建附件保留七天，过期后需重新构建。

发布流程会拒绝失败构建、被修改的安装包、两端版本不一致及已公开的同名版本；上方手工步骤仍可使用。依赖更新需同步第三方许可；JavaScript 清单可用 `collect-licenses.mjs` 更新，Rust 清单随 Cargo 锁文件维护。

离线打包 Electron 时，可将 `PKB_ELECTRON_DIST` 指向已解压的 Electron，或包含固定版本 `electron-v<version>-<platform>-<arch>.zip` 的本地目录。此选项只使用缓存，不更改固定的 Electron 版本。
