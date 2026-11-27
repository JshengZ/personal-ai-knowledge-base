import { Fragment } from "react";
import type { ComponentPropsWithRef } from "react";
import type { Snapshot } from "../types";
import { Select } from "./ui";

export function scopeParameters(value: string): Record<string, string> {
  return value.startsWith("collection:")
    ? { collection_id: value.slice(11) }
    : value
      ? { project_id: value }
      : {};
}
export function validScope(value: string, data: Snapshot) {
  return (
    !value ||
    (value.startsWith("collection:")
      ? data.roots.some((root) => root.id === value.slice(11))
      : data.projects.some((project) => project.id === value))
  );
}
const clean = (path: string) =>
  path
    .replace(/^\\\\\?\\/, "")
    .replaceAll("\\", "/")
    .replace(/\/$/, "");
export function ScopePicker({
  data,
  ...props
}: ComponentPropsWithRef<"select"> & { data: Snapshot }) {
  return (
    <Select {...props}>
      <option value="">全部资料</option>
      {data.roots.map((root) => (
        <Fragment key={root.id}>
          <option value={"collection:" + root.id}>
            {root.name}
            {root.enabled ? "" : "（已暂停）"}
          </option>
          {data.projects
            .filter(
              (project) =>
                project.root_id === root.id ||
                (!project.root_id &&
                  clean(project.root_path).startsWith(clean(root.path))),
            )
            .map((project) => (
              <option key={project.id} value={project.id}>
                　
                {clean(project.root_path) === clean(root.path)
                  ? "根目录文件"
                  : project.name}
              </option>
            ))}
        </Fragment>
      ))}
    </Select>
  );
}
