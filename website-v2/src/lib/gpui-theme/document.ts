/**
 * 主题文件 token 层的路径读写（`group.key` 按 `.` 嵌套），与 Rust `document.rs` 的
 * `get_path` / `set_path` / `remove_path` 同语义。写操作就地修改传入的文档。
 */
import type { Json, JsonObject, ThemeDocument, TokenLayer } from "./types";
import { isJsonObject } from "./types";

export function getPath(map: JsonObject, path: string): Json | undefined {
  let current: Json | undefined = map;
  for (const segment of path.split(".")) {
    if (!isJsonObject(current) || !Object.hasOwn(current, segment)) return undefined;
    current = current[segment];
  }
  return current;
}

/** 写入路径，按需创建（或把非对象替换为）中间对象。 */
export function setPath(map: JsonObject, path: string, value: Json): void {
  const dot = path.indexOf(".");
  if (dot < 0) {
    map[path] = value;
    return;
  }
  const head = path.slice(0, dot);
  let child = map[head];
  if (!isJsonObject(child)) {
    child = {};
    map[head] = child;
  }
  setPath(child, path.slice(dot + 1), value);
}

/** 删除路径，并清理因此变空的父对象；返回被删的值。 */
export function removePath(map: JsonObject, path: string): Json | undefined {
  const dot = path.indexOf(".");
  if (dot < 0) {
    if (!Object.hasOwn(map, path)) return undefined;
    const removed = map[path];
    delete map[path];
    return removed;
  }
  const head = path.slice(0, dot);
  const child = map[head];
  if (!isJsonObject(child)) return undefined;
  const removed = removePath(child, path.slice(dot + 1));
  if (removed !== undefined && Object.keys(child).length === 0) delete map[head];
  return removed;
}

export function tokenValue(document: ThemeDocument, layer: TokenLayer, path: string): Json | undefined {
  return getPath(document[layer], path);
}

export function setToken(document: ThemeDocument, layer: TokenLayer, path: string, value: Json): void {
  setPath(document[layer], path, value);
}

export function removeToken(document: ThemeDocument, layer: TokenLayer, path: string): Json | undefined {
  return removePath(document[layer], path);
}
