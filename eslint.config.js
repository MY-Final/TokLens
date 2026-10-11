import js from "@eslint/js";
import prettier from "eslint-config-prettier";
import reactHooks from "eslint-plugin-react-hooks";
import reactRefresh from "eslint-plugin-react-refresh";
import { defineConfig, globalIgnores } from "eslint/config";
import globals from "globals";
import tseslint from "typescript-eslint";

/**
 * shadcn CLI 生成的文件（`pnpm dlx shadcn@latest add ...`）。
 * 这些文件视为 vendor 代码：不参与 Prettier 格式化（见 .prettierignore），
 * 只保留通用规则，不套用只对自研组件有意义的规则。
 * 新增 shadcn 生成文件时请同步更新本列表与 .prettierignore。
 */
const shadcnGenerated = ["src/components/ui/**/*.{ts,tsx}", "src/hooks/use-mobile.ts"];

/**
 * React Compiler 专属规则。项目尚未接入 React Compiler，
 * 因此先降级为 warn：新代码不应引入新的告警，接入 compiler 后应删除本块。
 */
const reactCompilerRules = [
  "react-hooks/static-components",
  "react-hooks/use-memo",
  "react-hooks/preserve-manual-memoization",
  "react-hooks/incompatible-library",
  "react-hooks/immutability",
  "react-hooks/globals",
  "react-hooks/refs",
  "react-hooks/set-state-in-effect",
  "react-hooks/error-boundaries",
  "react-hooks/purity",
  "react-hooks/set-state-in-render",
  "react-hooks/unsupported-syntax",
];

const compilerRulesAsWarn = Object.fromEntries(reactCompilerRules.map((rule) => [rule, "warn"]));
const compilerRulesOff = Object.fromEntries(reactCompilerRules.map((rule) => [rule, "off"]));

export default defineConfig([
  // 构建产物、生成物与依赖目录不参与检查。
  globalIgnores(["dist", "node_modules", "src-tauri/target", "src-tauri/gen"]),

  {
    name: "toklens/frontend",
    files: ["**/*.{ts,tsx}"],
    extends: [
      js.configs.recommended,
      tseslint.configs.recommended,
      reactHooks.configs.flat.recommended,
      reactRefresh.configs.vite,
    ],
    languageOptions: {
      ecmaVersion: 2022,
      globals: globals.browser,
    },
    rules: {
      ...compilerRulesAsWarn,
      // 桌面应用不应向控制台留下调试输出，仅允许 warn/error。
      "no-console": ["warn", { allow: ["warn", "error"] }],
      eqeqeq: ["error", "smart"],
      "prefer-const": "error",
      "no-var": "error",
      "@typescript-eslint/no-unused-vars": [
        "error",
        { argsIgnorePattern: "^_", varsIgnorePattern: "^_" },
      ],
      // 只导入类型时使用 import type，与 tsconfig 的 verbatimModuleSyntax 保持一致。
      "@typescript-eslint/consistent-type-imports": [
        "error",
        { prefer: "type-imports", fixStyle: "inline-type-imports" },
      ],
    },
  },

  {
    name: "toklens/vite-config",
    files: ["vite.config.ts"],
    languageOptions: {
      globals: globals.node,
    },
  },

  {
    name: "toklens/shadcn-generated",
    files: shadcnGenerated,
    rules: {
      ...compilerRulesOff,
      // 生成组件会导出 variants（buttonVariants 等），fast refresh 规则不适用。
      "react-refresh/only-export-components": "off",
    },
  },

  // 必须放在最后：关闭与 Prettier 冲突的格式类规则。
  prettier,
]);
