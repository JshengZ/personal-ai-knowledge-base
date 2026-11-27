import type { IndexFilters } from "../types";

export const defaultIndexFilters: IndexFilters = {
  automatic: true,
  documents: true,
  code: true,
  images: true,
  excluded_paths: [],
  included_paths: [],
};
const lines = (value: string) =>
  value
    .split("\n")
    .map((line) => line.trim())
    .filter(Boolean);
export function IndexFilterEditor({
  value,
  exclusions,
  onChange,
  onExclusions,
}: {
  value: IndexFilters;
  exclusions: string;
  onChange: (value: IndexFilters) => void;
  onExclusions: (value: string) => void;
}) {
  return (
    <div className="index-filter-editor">
      <fieldset className="index-types">
        <legend>索引内容</legend>
        {(
          [
            ["documents", "文档与笔记"],
            ["code", "源代码与配置"],
            ["images", "图片文字 OCR"],
          ] as const
        ).map(([key, label]) => (
          <label className="checkbox-label" key={key}>
            <input
              type="checkbox"
              checked={value[key]}
              onChange={(event) =>
                onChange({ ...value, [key]: event.target.checked })
              }
            />
            {label}
          </label>
        ))}
      </fieldset>
      <label className="checkbox-label">
        <input
          type="checkbox"
          checked={value.automatic}
          onChange={(event) =>
            onChange({ ...value, automatic: event.target.checked })
          }
        />
        自动排除构建产物、依赖与缓存
      </label>
      <small className="muted">
        包含 Nuitka 的
        .build、.dist、.onefile-build。安装包、压缩包及视频不参与索引。
      </small>
      <details className="advanced">
        <summary>自定义索引规则</summary>
        <label>
          排除文件或目录
          <textarea
            rows={3}
            value={value.excluded_paths.join("\n")}
            onChange={(event) =>
              onChange({ ...value, excluded_paths: lines(event.target.value) })
            }
            placeholder="相对于资料目录，每行一项，例如 临时资料"
          />
          <small>
            目录下的新增文件也会排除。移除对应行并保存，可恢复纳入。
          </small>
        </label>
        <label>
          额外纳入的路径
          <textarea
            rows={2}
            value={value.included_paths.join("\n")}
            onChange={(event) =>
              onChange({ ...value, included_paths: lines(event.target.value) })
            }
            placeholder="需要保留的自动排除目录，每行一项"
          />
          <small>
            可覆盖自动规则；明确排除的路径和下方匹配规则优先。填写 .
            表示当前目录。
          </small>
        </label>
        <label>
          排除匹配规则
          <textarea
            rows={3}
            value={exclusions}
            onChange={(event) => onExclusions(event.target.value)}
          />
          <small>每行一条，例如 **/*.tmp。保存后生效，原始文件保留。</small>
        </label>
      </details>
    </div>
  );
}
