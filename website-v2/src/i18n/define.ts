/**
 * 文案按命名空间拆成 `src/i18n/messages/<ns>.ts`,每个模块用 `defineMessages` 同时声明 en 与 zh。
 *
 * en 是基线:zh 必须与 en 结构完全一致(键、数组、函数签名),缺键/多键在 `astro check` 阶段报错,
 * 所以不存在运行时回退。需要插值时直接写函数:`count: (n: number) => \`${n} items\``。
 */
import type { Lang } from "./config";

/** 把 en 的字面量类型放宽为同构的可替换形状(字符串 → string,保留结构与函数签名)。 */
export type Shape<T> = T extends string
  ? string
  : T extends (...args: infer A) => infer R
    ? (...args: A) => R extends string ? string : Shape<R>
    : T extends readonly (infer U)[]
      ? readonly Shape<U>[]
      : T extends object
        ? { readonly [K in keyof T]: Shape<T[K]> }
        : T;

export type Messages<T> = Record<Lang, Shape<T>>;

export function defineMessages<const T>(messages: { en: T; zh: Shape<T> }): Messages<T> {
  return messages as Messages<T>;
}
