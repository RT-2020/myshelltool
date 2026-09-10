// vite.config.ts（tsconfig.node.json 项目）所需的最小类型垫片。
// 项目依赖红线：TS 迁移 Phase 1 仅允许新增 typescript / vue-tsc，不引入 @types/node；
// 因此只声明 vite.config.ts 实际用到的 node:url 成员与 import.meta.url。
// 本文件只被 tsconfig.node.json include，不进入 src/ 应用项目。
interface ImportMeta {
  url: string;
}

declare module 'node:url' {
  export interface URL {
    href: string;
  }
  export function fileURLToPath(url: URL | string): string;
  export const URL: new (url: string | URL, base?: string | URL) => URL;
}
