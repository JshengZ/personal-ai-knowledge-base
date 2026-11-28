# Third-party notices

本应用采用 Apache-2.0 许可证。以下开源组件随安装包分发或编译进应用，实际平台按需选用。许可证原文保存在 licenses 目录。

| 组件 | 许可证 | 用途 |
| --- | --- | --- |
| Electron / Chromium / Node.js | MIT / BSD 等，见 Electron 随包 notices | 跨平台桌面运行时 |
| Qwen3-Embedding-0.6B-GGUF | Apache-2.0 | 随包离线向量模型 |
| llama.cpp b12711 | MIT | CPU 向量推理 |
| Tesseract eng / chi_sim traineddata | Apache-2.0 | 中英文 OCR |
| Noto Serif SC | OFL-1.1 | 本地字体 |
| Phosphor Icons | MIT | 界面图标 |
| LLVM libc++ / libc++abi / libunwind | Apache-2.0 WITH LLVM-exception | Windows 静态 C++ 运行库 |
| MinGW-w64 CRT | 见随包 COPYING | Windows 系统接口适配 |

Windows 随包 llama.cpp 从 b12711 源码静态构建，禁用 OpenSSL、OpenMP、Web UI 和 GUI manifest，使用通用 x64 CPU 指令；无需另装 Visual C++ Runtime。源码地址与二进制校验和见运行目录 build-manifest.json。

## JavaScript components

| Component | Version | License |
| --- | --- | --- |
| @fast-csv/format | 4.3.5 | MIT |
| @fast-csv/parse | 4.3.6 | MIT |
| @fontsource-variable/noto-serif-sc | 5.3.1 | OFL-1.1 |
| @napi-rs/canvas | 1.2.0 | MIT |
| @napi-rs/canvas-darwin-x64 | 1.2.0 | MIT |
| @napi-rs/canvas-win32-x64-msvc | 1.2.0 | MIT |
| @nodable/entities | 3.1.0 | MIT |
| @phosphor-icons/react | 2.1.10 | MIT |
| @types/debug | 4.1.13 | MIT |
| @types/estree | 1.0.10 | MIT |
| @types/estree-jsx | 1.0.5 | MIT |
| @types/hast | 3.0.5 | MIT |
| @types/mdast | 4.0.4 | MIT |
| @types/ms | 2.1.0 | MIT |
| @types/node | 14.18.63 | MIT |
| @types/node | 14.18.63 | MIT |
| @types/unist | 3.0.3 | MIT |
| @types/unist | 2.0.11 | MIT |
| @ungap/structured-clone | 1.4.0 | ISC |
| anynum | 1.0.1 | MIT |
| archiver | 5.3.2 | MIT |
| archiver-utils | 2.1.0 | MIT |
| archiver-utils | 3.0.4 | MIT |
| async | 3.2.6 | MIT |
| bail | 2.0.2 | MIT |
| balanced-match | 1.0.2 | MIT |
| balanced-match | 1.0.2 | MIT |
| base64-js | 1.5.1 | MIT |
| bl | 4.1.0 | MIT |
| bluebird | 3.7.2 | MIT |
| bmp-js | 0.1.0 | MIT |
| brace-expansion | 2.1.8 | MIT |
| brace-expansion | 1.1.22 | MIT |
| buffer | 5.7.1 | MIT |
| buffer-crc32 | 0.2.13 | MIT |
| ccount | 2.0.1 | MIT |
| character-entities | 2.0.2 | MIT |
| character-entities-html4 | 2.1.0 | MIT |
| character-entities-legacy | 3.0.0 | MIT |
| character-reference-invalid | 2.0.1 | MIT |
| comma-separated-tokens | 2.0.3 | MIT |
| compress-commons | 4.1.2 | MIT |
| concat-map | 0.0.1 | MIT |
| core-util-is | 1.0.3 | MIT |
| crc-32 | 1.2.2 | Apache-2.0 |
| crc32-stream | 4.0.3 | MIT |
| csv-parse | 6.2.1 | MIT |
| dayjs | 1.11.23 | MIT |
| debug | 4.4.3 | MIT |
| decode-named-character-reference | 1.3.0 | MIT |
| dequal | 2.0.3 | MIT |
| devlop | 1.1.0 | MIT |
| duplexer2 | 0.1.4 | BSD-3-Clause |
| end-of-stream | 1.4.5 | MIT |
| estree-util-is-identifier-name | 3.0.0 | MIT |
| exceljs | 4.4.0 | MIT |
| extend | 3.0.2 | MIT |
| fast-csv | 4.3.6 | MIT |
| fast-xml-builder | 1.3.2 | MIT |
| fast-xml-parser | 5.12.1 | MIT |
| fflate | 0.8.3 | MIT |
| fs-constants | 1.0.0 | MIT |
| fs-extra | 11.3.1 | MIT |
| fs.realpath | 1.0.0 | ISC |
| glob | 7.2.3 | ISC |
| graceful-fs | 4.2.12 | BlueOak-1.0.0 |
| hast-util-to-jsx-runtime | 2.3.7 | MIT |
| hast-util-whitespace | 3.0.0 | MIT |
| html-url-attributes | 3.0.1 | MIT |
| idb-keyval | 6.3.0 | Apache-2.0 |
| ieee754 | 1.2.1 | BSD-3-Clause |
| immediate | 3.0.6 | MIT |
| inflight | 1.0.6 | ISC |
| inherits | 2.0.4 | ISC |
| inline-style-parser | 0.2.9 | MIT |
| is-alphabetical | 2.0.1 | MIT |
| is-alphanumerical | 2.0.1 | MIT |
| is-decimal | 2.0.1 | MIT |
| is-hexadecimal | 2.0.1 | MIT |
| is-plain-obj | 4.1.0 | MIT |
| is-unsafe | 2.0.3 | MIT |
| is-url | 1.2.4 | MIT |
| isarray | 1.0.0 | MIT |
| jsonfile | 6.2.1 | MIT |
| jszip | 3.10.2 | (MIT OR GPL-3.0-or-later) |
| lazystream | 1.0.1 | MIT |
| lie | 3.3.0 | MIT |
| lodash.defaults | 4.2.0 | MIT |
| lodash.difference | 4.5.0 | MIT |
| lodash.escaperegexp | 4.1.2 | MIT |
| lodash.flatten | 4.4.0 | MIT |
| lodash.groupby | 4.6.0 | MIT |
| lodash.isboolean | 3.0.3 | MIT |
| lodash.isequal | 4.5.0 | MIT |
| lodash.isfunction | 3.0.9 | MIT |
| lodash.isnil | 4.0.0 | MIT |
| lodash.isplainobject | 4.0.6 | MIT |
| lodash.isundefined | 3.0.1 | MIT |
| lodash.union | 4.6.0 | MIT |
| lodash.uniq | 4.5.0 | MIT |
| longest-streak | 3.1.0 | MIT |
| mdast-util-from-markdown | 2.1.0 | MIT |
| mdast-util-mdx-expression | 2.0.1 | MIT |
| mdast-util-mdx-jsx | 3.2.2 | MIT |
| mdast-util-mdxjs-esm | 2.0.1 | MIT |
| mdast-util-phrasing | 4.1.0 | MIT |
| mdast-util-to-hast | 13.2.2 | MIT |
| mdast-util-to-markdown | 2.2.4 | MIT |
| mdast-util-to-string | 4.0.0 | MIT |
| micromark | 4.0.3 | MIT |
| micromark-core-commonmark | 2.0.4 | MIT |
| micromark-factory-destination | 2.0.1 | MIT |
| micromark-factory-label | 2.0.1 | MIT |
| micromark-factory-space | 2.1.0 | MIT |
| micromark-factory-title | 2.0.1 | MIT |
| micromark-factory-whitespace | 2.0.1 | MIT |
| micromark-util-character | 2.1.1 | MIT |
| micromark-util-chunked | 2.0.1 | MIT |
| micromark-util-classify-character | 2.0.1 | MIT |
| micromark-util-combine-extensions | 2.0.1 | MIT |
| micromark-util-decode-numeric-character-reference | 2.0.2 | MIT |
| micromark-util-decode-string | 2.0.1 | MIT |
| micromark-util-edit-map | 1.0.0 | MIT |
| micromark-util-encode | 2.0.1 | MIT |
| micromark-util-html-tag-name | 2.0.1 | MIT |
| micromark-util-normalize-identifier | 2.0.1 | MIT |
| micromark-util-resolve-all | 2.0.1 | MIT |
| micromark-util-sanitize-uri | 2.0.1 | MIT |
| micromark-util-subtokenize | 2.1.0 | MIT |
| micromark-util-symbol | 2.0.1 | MIT |
| micromark-util-types | 2.0.3 | MIT |
| minimatch | 5.1.9 | ISC |
| minimatch | 3.1.5 | ISC |
| ms | 2.1.3 | MIT |
| node-fetch | 2.7.0 | MIT |
| node-int64 | 0.4.0 | MIT |
| normalize-path | 3.0.0 | MIT |
| once | 1.4.0 | ISC |
| opencollective-postinstall | 2.0.3 | MIT |
| pako | 1.0.11 | (MIT AND Zlib) |
| parse-entities | 4.0.2 | MIT |
| path-expression-matcher | 1.6.2 | MIT |
| path-is-absolute | 1.0.1 | MIT |
| pdfjs-dist | 6.5.438 | Apache-2.0 |
| process-nextick-args | 2.0.1 | MIT |
| property-information | 7.2.0 | MIT |
| react | 19.3.1 | MIT |
| react-dom | 19.3.1 | MIT |
| react-markdown | 10.1.0 | MIT |
| readable-stream | 3.6.2 | MIT |
| readable-stream | 2.3.8 | MIT |
| readable-stream | 2.3.8 | MIT |
| readable-stream | 2.3.8 | MIT |
| readable-stream | 2.3.8 | MIT |
| readdir-glob | 1.1.3 | Apache-2.0 |
| regenerator-runtime | 0.13.11 | MIT |
| remark-parse | 11.0.0 | MIT |
| remark-rehype | 11.1.2 | MIT |
| safe-buffer | 5.1.2 | MIT |
| saxes | 5.0.1 | ISC |
| scheduler | 0.28.0 | MIT |
| setimmediate | 1.0.5 | MIT |
| space-separated-tokens | 2.0.2 | MIT |
| string_decoder | 1.1.1 | MIT |
| stringify-entities | 4.0.5 | MIT |
| strnum | 2.5.1 | MIT |
| style-to-object | 2.0.2 | MIT |
| tar-stream | 2.2.0 | MIT |
| tesseract.js | 7.0.0 | Apache-2.0 |
| tesseract.js-core | 7.0.0 | Apache-2.0 |
| tmp | 0.2.7 | MIT |
| tr46 | 0.0.3 | MIT |
| trim-lines | 3.0.1 | MIT |
| trough | 2.2.0 | MIT |
| unified | 11.0.5 | MIT |
| unist-util-is | 6.0.1 | MIT |
| unist-util-position | 5.0.0 | MIT |
| unist-util-stringify-position | 4.0.0 | MIT |
| unist-util-visit | 5.2.0 | MIT |
| unist-util-visit-parents | 6.1.0 | MIT |
| universalify | 2.0.1 | MIT |
| unzipper | 0.12.5 | MIT |
| util-deprecate | 1.0.2 | MIT |
| uuid | 8.3.2 | MIT |
| vfile | 6.0.3 | MIT |
| vfile-message | 4.0.4 | MIT |
| wasm-feature-detect | 1.10.1 | Apache-2.0 |
| webidl-conversions | 3.0.1 | BSD-2-Clause |
| whatwg-url | 5.0.0 | MIT |
| wrappy | 1.0.2 | ISC |
| xml-naming | 0.3.0 | MIT |
| xmlchars | 2.2.0 | MIT |
| zip-stream | 4.1.1 | MIT |
| zlibjs | 0.3.1 | MIT |
| zwitch | 2.0.4 | MIT |

## Rust components

包含构建工具与特定平台组件的记录。未参与当前平台链接的依赖不包含在对应可执行文件中。

| Component | Version | License |
| --- | --- | --- |
| adler2 | 2.0.1 | 0BSD OR MIT OR Apache-2.0 |
| adler32 | 1.2.0 | Zlib |
| aho-corasick | 1.1.5 | Unlicense OR MIT |
| allocator-api2 | 0.2.21 | MIT OR Apache-2.0 |
| anstream | 1.0.0 | MIT OR Apache-2.0 |
| anstyle | 1.0.14 | MIT OR Apache-2.0 |
| anstyle-parse | 1.0.0 | MIT OR Apache-2.0 |
| anstyle-query | 1.1.5 | MIT OR Apache-2.0 |
| anstyle-wincon | 3.0.11 | MIT OR Apache-2.0 |
| anyhow | 1.0.105 | MIT OR Apache-2.0 |
| arrayvec | 0.7.8 | MIT OR Apache-2.0 |
| async-trait | 0.1.92 | MIT OR Apache-2.0 |
| atomic-waker | 1.1.2 | Apache-2.0 OR MIT |
| autocfg | 1.5.1 | Apache-2.0 OR MIT |
| axum | 0.8.9 | MIT |
| axum-core | 0.5.6 | MIT |
| base64 | 0.22.1 | MIT OR Apache-2.0 |
| base64 | 0.23.1 | MIT OR Apache-2.0 |
| bitflags | 2.13.2 | MIT OR Apache-2.0 |
| blake3 | 1.8.7 | CC0-1.0 OR Apache-2.0 OR Apache-2.0 WITH LLVM-exception |
| block-buffer | 0.12.1 | MIT OR Apache-2.0 |
| bstr | 1.14.0 | MIT OR Apache-2.0 |
| bumpalo | 3.20.4 | MIT OR Apache-2.0 |
| byteorder | 1.5.0 | Unlicense OR MIT |
| bytes | 1.12.2 | MIT |
| cc | 1.11.3 | MIT OR Apache-2.0 |
| cedarwood | 0.4.6 | BSD-2-Clause |
| cfg_aliases | 0.2.2 | MIT |
| cfg-if | 1.0.5 | MIT OR Apache-2.0 |
| chacha20 | 0.10.2 | MIT OR Apache-2.0 |
| chrono | 0.4.45 | MIT OR Apache-2.0 |
| clap | 4.6.10 | MIT OR Apache-2.0 |
| clap_builder | 4.6.10 | MIT OR Apache-2.0 |
| clap_derive | 4.6.9 | MIT OR Apache-2.0 |
| clap_lex | 1.1.2 | MIT OR Apache-2.0 |
| colorchoice | 1.0.5 | MIT OR Apache-2.0 |
| const-oid | 0.10.3 | Apache-2.0 OR MIT |
| constant_time_eq | 0.4.2 | CC0-1.0 OR MIT-0 OR Apache-2.0 |
| core-foundation | 0.10.1 | MIT OR Apache-2.0 |
| core-foundation-sys | 0.8.7 | MIT OR Apache-2.0 |
| cpufeatures | 0.3.1 | MIT OR Apache-2.0 |
| crc32fast | 1.5.6 | MIT OR Apache-2.0 |
| crypto-common | 0.2.2 | MIT OR Apache-2.0 |
| darling | 0.24.1 | MIT |
| darling_core | 0.24.1 | MIT |
| darling_macro | 0.24.1 | MIT |
| dary_heap | 0.3.9 | MIT OR Apache-2.0 |
| digest | 0.11.3 | MIT OR Apache-2.0 |
| directories | 6.0.0 | MIT OR Apache-2.0 |
| dirs-sys | 0.5.0 | MIT OR Apache-2.0 |
| displaydoc | 0.2.7 | MIT OR Apache-2.0 |
| dyn-clone | 1.0.20 | MIT OR Apache-2.0 |
| equivalent | 1.0.3 | Apache-2.0 OR MIT |
| errno | 0.3.15 | MIT OR Apache-2.0 |
| fallible-iterator | 0.3.0 | MIT/Apache-2.0 |
| fallible-streaming-iterator | 0.1.9 | MIT/Apache-2.0 |
| fastrand | 2.5.0 | Apache-2.0 OR MIT |
| find-msvc-tools | 0.1.15 | MIT OR Apache-2.0 |
| flate2 | 1.1.11 | MIT OR Apache-2.0 |
| foldhash | 0.1.5 | Zlib |
| foldhash | 0.2.0 | Zlib |
| form_urlencoded | 1.2.3 | MIT OR Apache-2.0 |
| fs2 | 0.4.3 | MIT/Apache-2.0 |
| futures | 0.3.34 | MIT OR Apache-2.0 |
| futures-channel | 0.3.34 | MIT OR Apache-2.0 |
| futures-core | 0.3.34 | MIT OR Apache-2.0 |
| futures-executor | 0.3.34 | MIT OR Apache-2.0 |
| futures-io | 0.3.34 | MIT OR Apache-2.0 |
| futures-macro | 0.3.34 | MIT OR Apache-2.0 |
| futures-sink | 0.3.34 | MIT OR Apache-2.0 |
| futures-task | 0.3.34 | MIT OR Apache-2.0 |
| futures-util | 0.3.34 | MIT OR Apache-2.0 |
| getrandom | 0.2.17 | MIT OR Apache-2.0 |
| getrandom | 0.4.3 | MIT OR Apache-2.0 |
| globset | 0.4.22 | Unlicense OR MIT |
| hashbrown | 0.15.5 | MIT OR Apache-2.0 |
| hashbrown | 0.16.1 | MIT OR Apache-2.0 |
| hashbrown | 0.17.2 | MIT OR Apache-2.0 |
| hashlink | 0.10.0 | MIT OR Apache-2.0 |
| heck | 0.5.0 | MIT OR Apache-2.0 |
| http | 1.5.0 | MIT OR Apache-2.0 |
| http-body | 1.1.0 | MIT |
| http-body-util | 0.1.5 | MIT |
| http-range-header | 0.4.2 | MIT |
| httparse | 1.10.1 | MIT OR Apache-2.0 |
| httpdate | 1.0.3 | MIT OR Apache-2.0 |
| hybrid-array | 0.4.15 | MIT OR Apache-2.0 |
| hyper | 1.13.0 | MIT |
| hyper-rustls | 0.27.10 | Apache-2.0 OR ISC OR MIT |
| hyper-util | 0.1.21 | MIT |
| iana-time-zone | 0.1.65 | MIT OR Apache-2.0 |
| icu_collections | 2.4.0 | Unicode-3.0 |
| icu_locale_core | 2.4.0 | Unicode-3.0 |
| icu_normalizer | 2.4.0 | Unicode-3.0 |
| icu_normalizer_data | 2.4.0 | Unicode-3.0 |
| icu_properties | 2.4.0 | Unicode-3.0 |
| icu_properties_data | 2.4.0 | Unicode-3.0 |
| icu_provider | 2.4.0 | Unicode-3.0 |
| ident_case | 1.0.1 | MIT/Apache-2.0 |
| idna | 1.1.0 | MIT OR Apache-2.0 |
| idna_adapter | 1.2.2 | Apache-2.0 OR MIT |
| include-flate | 0.3.5 | Apache-2.0 |
| include-flate-codegen | 0.3.5 | Apache-2.0 |
| include-flate-compress | 0.3.5 | Apache-2.0 |
| indexmap | 2.14.2 | Apache-2.0 OR MIT |
| ipnet | 2.12.2 | MIT OR Apache-2.0 |
| is_terminal_polyfill | 1.70.2 | MIT OR Apache-2.0 |
| itoa | 1.0.19 | MIT OR Apache-2.0 |
| jieba-macros | 0.8.1 | MIT |
| jieba-rs | 0.8.1 | MIT |
| jobserver | 0.1.35 | MIT OR Apache-2.0 |
| keyring | 3.6.3 | MIT OR Apache-2.0 |
| lazy_static | 1.5.1 | MIT OR Apache-2.0 |
| libc | 0.2.191 | MIT OR Apache-2.0 |
| libflate | 2.3.2 | MIT |
| libflate_lz77 | 2.3.0 | MIT |
| libsqlite3-sys | 0.35.0 | MIT |
| litemap | 0.8.3 | Unicode-3.0 |
| lock_api | 0.4.14 | MIT OR Apache-2.0 |
| log | 0.4.35 | MIT OR Apache-2.0 |
| lru-slab | 0.1.3 | MIT OR Apache-2.0 OR Zlib |
| matchers | 0.2.0 | MIT |
| matchit | 0.8.4 | MIT AND BSD-3-Clause |
| memchr | 2.8.3 | Unlicense OR MIT |
| mime | 0.3.18 | MIT OR Apache-2.0 |
| mime_guess | 2.0.5 | MIT |
| miniz_oxide | 0.9.1 | MIT OR Zlib OR Apache-2.0 |
| mio | 1.2.5 | MIT |
| no_std_io2 | 0.9.4 | Apache-2.0 OR MIT |
| notify | 8.2.2 | CC0-1.0 |
| notify-types | 2.3.1 | MIT OR Apache-2.0 |
| nu-ansi-term | 0.50.3 | MIT |
| num-traits | 0.2.19 | MIT OR Apache-2.0 |
| objc2-core-foundation | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| objc2-core-services | 0.3.2 | Zlib OR Apache-2.0 OR MIT |
| once_cell | 1.21.4 | MIT OR Apache-2.0 |
| once_cell_polyfill | 1.70.2 | MIT OR Apache-2.0 |
| option-ext | 0.2.0 | MPL-2.0 |
| parking_lot | 0.12.5 | MIT OR Apache-2.0 |
| parking_lot_core | 0.9.12 | MIT OR Apache-2.0 |
| pastey | 0.2.3 | MIT OR Apache-2.0 |
| percent-encoding | 2.3.2 | MIT OR Apache-2.0 |
| phf | 0.13.1 | MIT |
| phf_codegen | 0.13.1 | MIT |
| phf_generator | 0.13.1 | MIT |
| phf_shared | 0.13.1 | MIT |
| pin-project-lite | 0.2.18 | Apache-2.0 OR MIT |
| pkg-config | 0.3.34 | MIT OR Apache-2.0 |
| potential_utf | 0.1.6 | Unicode-3.0 |
| proc-macro-error-attr3 | 3.1.1 | MIT OR Apache-2.0 |
| proc-macro-error3 | 3.1.1 | MIT OR Apache-2.0 |
| proc-macro2 | 1.0.107 | MIT OR Apache-2.0 |
| quinn | 0.11.12 | MIT OR Apache-2.0 |
| quinn-proto | 0.11.19 | MIT OR Apache-2.0 |
| quinn-udp | 0.5.16 | MIT OR Apache-2.0 |
| quote | 1.0.47 | MIT OR Apache-2.0 |
| rand | 0.10.4 | MIT OR Apache-2.0 |
| rand_core | 0.10.1 | MIT OR Apache-2.0 |
| rand_pcg | 0.10.3 | MIT OR Apache-2.0 |
| ref-cast | 1.0.27 | MIT OR Apache-2.0 |
| ref-cast-impl | 1.0.27 | MIT OR Apache-2.0 |
| regex | 1.13.2 | MIT OR Apache-2.0 |
| regex-automata | 0.4.19 | MIT OR Apache-2.0 |
| regex-syntax | 0.8.12 | MIT OR Apache-2.0 |
| reqwest | 0.12.28 | MIT OR Apache-2.0 |
| ring | 0.17.14 | Apache-2.0 AND ISC |
| rle-decode-fast | 1.0.3 | MIT OR Apache-2.0 |
| rmcp | 3.11.0 | Apache-2.0 |
| rmcp-macros | 3.11.0 | Apache-2.0 |
| rusqlite | 0.37.0 | MIT |
| rustc-hash | 2.1.3 | Apache-2.0 OR MIT |
| rustix | 1.1.6 | Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT |
| rustls | 0.23.46 | Apache-2.0 OR ISC OR MIT |
| rustls-native-certs | 0.8.5 | Apache-2.0 OR ISC OR MIT |
| rustls-pki-types | 1.16.0 | MIT OR Apache-2.0 |
| rustls-webpki | 0.103.17 | ISC |
| ryu | 1.0.23 | Apache-2.0 OR BSL-1.0 |
| same-file | 1.0.6 | Unlicense/MIT |
| schannel | 0.1.30 | MIT |
| schemars | 1.2.3 | MIT |
| schemars_derive | 1.2.3 | MIT |
| scopeguard | 1.2.0 | MIT OR Apache-2.0 |
| security-framework | 3.7.0 | MIT OR Apache-2.0 |
| security-framework-sys | 2.17.0 | MIT OR Apache-2.0 |
| serde | 1.0.229 | MIT OR Apache-2.0 |
| serde_core | 1.0.229 | MIT OR Apache-2.0 |
| serde_derive | 1.0.229 | MIT OR Apache-2.0 |
| serde_derive_internals | 0.30.0 | MIT OR Apache-2.0 |
| serde_json | 1.0.154 | MIT OR Apache-2.0 |
| serde_path_to_error | 0.1.20 | MIT OR Apache-2.0 |
| serde_urlencoded | 0.7.1 | MIT/Apache-2.0 |
| sha2 | 0.11.1 | MIT OR Apache-2.0 |
| sharded-slab | 0.1.7 | MIT |
| shlex | 2.0.1 | MIT OR Apache-2.0 |
| signal-hook-registry | 1.4.8 | MIT OR Apache-2.0 |
| simd-adler32 | 0.3.10 | MIT |
| siphasher | 1.0.4 | MIT OR Apache-2.0 |
| slab | 0.4.12 | MIT |
| smallvec | 1.16.6 | MIT OR Apache-2.0 |
| socket2 | 0.6.5 | MIT OR Apache-2.0 |
| sqlite-vec | 0.1.9 | MIT/Apache-2.0 |
| sse-stream | 0.2.6 | MIT OR Apache-2.0 |
| stable_deref_trait | 1.2.1 | MIT OR Apache-2.0 |
| strsim | 0.11.1 | MIT |
| subtle | 2.6.1 | BSD-3-Clause |
| syn | 2.0.119 | MIT OR Apache-2.0 |
| syn | 3.0.10 | MIT OR Apache-2.0 |
| sync_wrapper | 1.0.2 | Apache-2.0 |
| synstructure | 0.14.0 | MIT |
| tempfile | 3.27.0 | MIT OR Apache-2.0 |
| thiserror | 2.0.21 | MIT OR Apache-2.0 |
| thiserror-impl | 2.0.21 | MIT OR Apache-2.0 |
| thread_local | 1.1.10 | MIT OR Apache-2.0 |
| tinystr | 0.8.5 | Unicode-3.0 |
| tinyvec | 1.13.3 | Zlib OR Apache-2.0 OR MIT |
| tokio | 1.54.1 | MIT |
| tokio-macros | 2.7.2 | MIT |
| tokio-rustls | 0.26.6 | MIT OR Apache-2.0 |
| tokio-stream | 0.1.19 | MIT |
| tokio-util | 0.7.20 | MIT |
| tower | 0.5.3 | MIT |
| tower-http | 0.6.11 | MIT |
| tower-layer | 0.3.3 | MIT |
| tower-service | 0.3.3 | MIT |
| tracing | 0.1.44 | MIT |
| tracing-attributes | 0.1.31 | MIT |
| tracing-core | 0.1.36 | MIT |
| tracing-log | 0.2.0 | MIT |
| tracing-subscriber | 0.3.23 | MIT |
| try-lock | 0.2.5 | MIT |
| typenum | 1.20.1 | MIT OR Apache-2.0 |
| unicase | 2.10.0 | MIT OR Apache-2.0 |
| unicode-general-category | 1.1.0 | Apache-2.0 |
| unicode-ident | 1.0.27 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| unicode-normalization | 0.1.26 | (MIT OR Apache-2.0) AND Unicode-3.0 |
| untrusted | 0.9.0 | ISC |
| url | 2.5.9 | MIT OR Apache-2.0 |
| utf_types | 1.0.0 | Apache-2.0 OR MIT |
| utf8_iter | 1.1.0 | Apache-2.0 OR MIT |
| utf8parse | 0.2.2 | Apache-2.0 OR MIT |
| uuid | 1.29.3 | Apache-2.0 OR MIT |
| vcpkg | 0.2.15 | MIT/Apache-2.0 |
| walkdir | 2.5.1 | Unlicense/MIT |
| want | 0.3.2 | MIT |
| winapi | 0.3.9 | MIT/Apache-2.0 |
| winapi-util | 0.1.11 | Unlicense OR MIT |
| winapi-x86_64-pc-windows-gnu | 0.4.0 | MIT/Apache-2.0 |
| windows_x86_64_gnu | 0.53.1 | MIT OR Apache-2.0 |
| windows-core | 0.62.2 | MIT OR Apache-2.0 |
| windows-implement | 0.60.2 | MIT OR Apache-2.0 |
| windows-interface | 0.59.3 | MIT OR Apache-2.0 |
| windows-link | 0.2.1 | MIT OR Apache-2.0 |
| windows-result | 0.4.1 | MIT OR Apache-2.0 |
| windows-strings | 0.5.1 | MIT OR Apache-2.0 |
| windows-sys | 0.60.2 | MIT OR Apache-2.0 |
| windows-sys | 0.61.2 | MIT OR Apache-2.0 |
| windows-targets | 0.53.5 | MIT OR Apache-2.0 |
| writeable | 0.6.4 | Unicode-3.0 |
| yoke | 0.8.4 | Unicode-3.0 |
| yoke-derive | 0.8.4 | Unicode-3.0 |
| zerofrom | 0.1.8 | Unicode-3.0 |
| zerofrom-derive | 0.1.8 | Unicode-3.0 |
| zeroize | 1.9.2 | Apache-2.0 OR MIT |
| zerotrie | 0.2.5 | Unicode-3.0 |
| zerovec | 0.11.9 | Unicode-3.0 |
| zerovec-derive | 0.11.7 | Unicode-3.0 |
| zip | 2.6.1 | MIT |
| zmij | 1.0.23 | MIT |
| zopfli | 0.8.4 | Apache-2.0 |
| zstd | 0.14.5 | BSD-3-Clause |
| zstd-safe | 8.4.0 | BSD-3-Clause |
| zstd-sys | 2.2.0+zstd.1.6.0 | BSD-3-Clause AND BSD-2-Clause |

组件源码地址与版本记录见 licenses/components.json。Electron 自身包含 Chromium 与其他基础库的许可证清单。

Component source repositories and versions are listed in `licenses/components.json`. Electron includes its own Chromium and bundled-library license inventory.
