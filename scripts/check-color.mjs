#!/usr/bin/env node
// Zero-dependency color-token validator for src/index.css.
//
// Checks:
//   WCAG contrast of interactive foreground/background token pairs (must pass).
//   Perceptual OKLab ΔE separation for L0 pairs (must pass) and L1 pairs (warn).
//   Colour-vision-deficiency simulation: Machado, Oliveira & Fernandes (2009),
//   "A Physiologically-based Model for Simulation of Color Vision Deficiency",
//   severity 1.0 matrices applied in linear sRGB.
//
// Thresholds (Euclidean distance in OKLab): normal vision >= 0.10, each of
// protan/deutan/tritan >= 0.05. L0 failures exit 1 unless listed in EXEMPTIONS.
//
// This is a validation tool, not a test framework: the front-end still has no
// test runner (see docs/architecture.md). It reads token values only.

import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const ROOT = join(dirname(fileURLToPath(import.meta.url)), "..");
const CSS_PATH = join(ROOT, "src", "index.css");

const NORMAL_MIN = 0.1;
const CVD_MIN = 0.05;
const WCAG_MIN = 4.5;

// Machado et al. (2009), severity 1.0, linear-RGB matrices.
const CVD_MATRICES = {
  protan: [
    [0.152286, 1.052583, -0.204868],
    [0.114503, 0.786281, 0.099216],
    [-0.003882, -0.048116, 1.051998],
  ],
  deutan: [
    [0.367322, 0.860646, -0.227968],
    [0.280085, 0.672501, 0.047413],
    [-0.01182, 0.04294, 0.968881],
  ],
  tritan: [
    [1.255528, -0.076749, -0.178779],
    [-0.078411, 0.930809, 0.147602],
    [0.004733, 0.691367, 0.3039],
  ],
};

// Explicit L2 exemptions: L0 pairs that cannot be separated within the fixed
// brand/identity palette. Every entry carries a reason; silent skips are banned.
const EXEMPTIONS = [
  {
    theme: "dark",
    pair: "primary~workbuddy",
    reason:
      "深色下品牌主色（浅莓紫 #e787d3）与 WorkBuddy 青色（oklch 0.74 0.12 220）均为已冻结值，deuteran 模拟 ΔE≈0.038 低于 0.05；两者分属交互强调簇与身份色簇，实际布局不并排。常态 ΔE=0.229（仅 deutan 0.038 受限）。",
  },
  {
    theme: "dark",
    pair: "primary~grok",
    reason:
      "Grok 身份色 oklch(0.76 0.04 250) 近中性灰（chroma 0.04），与浅莓紫主色在 deuteran 模拟下 ΔE≈0.015；两值均在授权范围外（主色已拍板、身份色不可改）。常态 ΔE=0.152（仅 deutan 0.015 受限）。",
  },
  {
    theme: "dark",
    pair: "primary~mcode",
    reason:
      "MCode 身份色 oklch(0.76 0.14 15) 与浅莓紫主色在 tritan 模拟下 ΔE≈0.042，仅差 0.008；主色与身份色均不可改，实际分属不同簇。常态 ΔE=0.100（仅 tritan 0.042 受限；全表最薄弱对，蓝紫簇专项时复评）。",
  },
];

const clamp01 = (x) => Math.min(1, Math.max(0, x));

function oklabToLinear([L, a, b]) {
  const l_ = L + 0.3963377774 * a + 0.2158037573 * b;
  const m_ = L - 0.1055613458 * a - 0.0638541728 * b;
  const s_ = L - 0.0894841775 * a - 1.291485548 * b;
  const l = l_ ** 3;
  const m = m_ ** 3;
  const s = s_ ** 3;
  return [
    4.0767416621 * l - 3.3077115913 * m + 0.2309699292 * s,
    -1.2684380046 * l + 2.6097574011 * m - 0.3413193965 * s,
    -0.0041960863 * l - 0.7034186147 * m + 1.707614701 * s,
  ];
}

const linearToSrgb = (c) => (c <= 0.0031308 ? 12.92 * c : 1.055 * c ** (1 / 2.4) - 0.055);
const srgbToLinear = (c) => (c <= 0.04045 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4);

function oklchToRgb(L, C, h) {
  const rad = (h * Math.PI) / 180;
  return oklabToLinear([L, C * Math.cos(rad), C * Math.sin(rad)]).map((c) =>
    clamp01(linearToSrgb(c)),
  );
}

function hexToRgb(value) {
  const hex = value.replace("#", "").trim();
  const full = hex.length === 3 ? hex.replace(/./g, (c) => c + c) : hex;
  return [0, 2, 4].map((i) => parseInt(full.slice(i, i + 2), 16) / 255);
}

function parseColor(raw) {
  const value = raw.trim();
  if (value.startsWith("#")) return hexToRgb(value);
  const match = value.match(/^oklch\(\s*([\d.]+)\s+([\d.]+)\s+([\d.]+)/i);
  if (match) return oklchToRgb(Number(match[1]), Number(match[2]), Number(match[3]));
  throw new Error(`Unsupported color value: ${value}`);
}

function parseBlock(css, pattern) {
  const match = pattern.exec(css);
  if (!match || match.index === undefined)
    throw new Error(`Missing ${pattern} block in ${CSS_PATH}`);
  const open = css.indexOf("{", match.index + match[0].length - 1);
  const end = css.indexOf("}", open);
  const body = css.slice(open + 1, end);
  const tokens = {};
  for (const line of body.split("\n")) {
    const match = line.match(/--([a-z0-9-]+)\s*:\s*([^;]+);/i);
    if (!match) continue;
    try {
      tokens[match[1]] = parseColor(match[2]);
    } catch {
      // Non-color custom properties (e.g. --radius) are ignored.
    }
  }
  return tokens;
}

function relativeLuminance(rgb) {
  const [r, g, b] = rgb.map(srgbToLinear);
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

function contrast(a, b) {
  const x = relativeLuminance(a);
  const y = relativeLuminance(b);
  return (Math.max(x, y) + 0.05) / (Math.min(x, y) + 0.05);
}

function srgbToOklab(rgb) {
  const [r, g, b] = rgb.map(srgbToLinear);
  const l = Math.cbrt(0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b);
  const m = Math.cbrt(0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b);
  const s = Math.cbrt(0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b);
  return [
    0.2104542553 * l + 0.793617785 * m - 0.0040720468 * s,
    1.9779984951 * l - 2.428592205 * m + 0.4505937099 * s,
    0.0259040371 * l + 0.7827717662 * m - 0.808675766 * s,
  ];
}

function deltaE(a, b) {
  const A = srgbToOklab(a);
  const B = srgbToOklab(b);
  return Math.hypot(A[0] - B[0], A[1] - B[1], A[2] - B[2]);
}

function applyMatrix(matrix, linear) {
  return matrix.map((row) => row[0] * linear[0] + row[1] * linear[1] + row[2] * linear[2]);
}

function simulate(kind, rgb) {
  return applyMatrix(CVD_MATRICES[kind], rgb.map(srgbToLinear)).map((c) =>
    clamp01(linearToSrgb(c)),
  );
}

function separation(a, b) {
  return {
    normal: deltaE(a, b),
    protan: deltaE(simulate("protan", a), simulate("protan", b)),
    deutan: deltaE(simulate("deutan", a), simulate("deutan", b)),
    tritan: deltaE(simulate("tritan", a), simulate("tritan", b)),
  };
}

function passes(m) {
  return (
    m.normal >= NORMAL_MIN && m.protan >= CVD_MIN && m.deutan >= CVD_MIN && m.tritan >= CVD_MIN
  );
}

const fmt = (n) => n.toFixed(3);

let failures = 0;
let warnings = 0;

function main() {
  const css = readFileSync(CSS_PATH, "utf8");
  const themes = {
    light: parseBlock(css, /:root\s*\{/),
    dark: parseBlock(css, /\.dark\s*\{/),
  };

  const exemptionKeys = new Set(EXEMPTIONS.map((e) => `${e.theme}:${e.pair}`));

  for (const [theme, tokens] of Object.entries(themes)) {
    console.log(`\n=== theme: ${theme} ===`);
    const charts = [1, 2, 3, 4, 5].map((i) => [`chart-${i}`, tokens[`chart-${i}`]]);
    const clients = Object.entries(tokens)
      .filter(([name]) => name.startsWith("client-"))
      .map(([name, rgb]) => [name.replace(/^client-/, ""), rgb]);

    for (const [name, bg, fg] of [
      ["primary", tokens.primary, tokens["primary-foreground"]],
      ["accent", tokens.accent, tokens["accent-foreground"]],
      ["sidebar-accent", tokens["sidebar-accent"], tokens["sidebar-accent-foreground"]],
    ]) {
      const ratio = contrast(fg, bg);
      const ok = ratio >= WCAG_MIN;
      if (!ok) failures++;
      console.log(
        `${ok ? "PASS" : "FAIL"}  WCAG ${name}-foreground/${name} = ${ratio.toFixed(2)}:1 (>= ${WCAG_MIN})`,
      );
    }

    const l0 = [];
    for (const [name, rgb] of clients) l0.push([`primary~${name}`, tokens.primary, rgb]);
    for (const [name, rgb] of charts) l0.push([`primary~${name}`, tokens.primary, rgb]);
    l0.push(["chart-1~codex", tokens["chart-1"], tokens["client-codex"]]);
    l0.push(["chart-5~claude", tokens["chart-5"], tokens["client-claude"]]);
    for (let i = 0; i < charts.length; i++)
      for (let j = i + 1; j < charts.length; j++)
        l0.push([`chart-${i + 1}~chart-${j + 1}`, charts[i][1], charts[j][1]]);

    let l0Pass = 0;
    for (const [pair, a, b] of l0) {
      const m = separation(a, b);
      if (passes(m)) {
        l0Pass += 1;
        console.log(`PASS  L0 ${pair}`);
        continue;
      }
      if (exemptionKeys.has(`${theme}:${pair}`)) {
        console.log(
          `L2    L0 ${pair} exempted (n=${fmt(m.normal)} p=${fmt(m.protan)} d=${fmt(m.deutan)} t=${fmt(m.tritan)})`,
        );
        continue;
      }
      failures++;
      console.log(
        `FAIL  L0 ${pair} n=${fmt(m.normal)} p=${fmt(m.protan)} d=${fmt(m.deutan)} t=${fmt(m.tritan)}`,
      );
    }
    console.log(`L0 summary (${theme}): ${l0Pass} pass / ${l0.length} checked`);

    const l1 = [];
    for (const [cn, crgb] of clients)
      for (const [hn, hrgb] of charts) {
        if ((cn === "codex" && hn === "chart-1") || (cn === "claude" && hn === "chart-5")) continue;
        l1.push([`client:${cn}~${hn}`, crgb, hrgb]);
      }
    for (let i = 0; i < clients.length; i++)
      for (let j = i + 1; j < clients.length; j++)
        l1.push([`client:${clients[i][0]}~${clients[j][0]}`, clients[i][1], clients[j][1]]);

    const l1Failed = [];
    for (const [pair, a, b] of l1) {
      const m = separation(a, b);
      if (!passes(m))
        l1Failed.push(
          `${pair} (n=${fmt(m.normal)} p=${fmt(m.protan)} d=${fmt(m.deutan)} t=${fmt(m.tritan)})`,
        );
    }
    warnings += l1Failed.length;
    console.log(`WARN  L1 soft-separation pairs below threshold: ${l1Failed.length}`);
    for (const line of l1Failed) console.log(`      ${line}`);
  }

  console.log("\n=== L2 exemptions (explicit, never silent) ===");
  for (const e of EXEMPTIONS) console.log(`  ${e.theme}:${e.pair} — ${e.reason}`);

  console.log(
    `\nSummary: L0/WCAG failures=${failures}, L1 warnings=${warnings}. ` +
      `Thresholds: normal>=${NORMAL_MIN}, protan/deutan/tritan>=${CVD_MIN}.`,
  );
  if (failures > 0) {
    console.error(`\ncolor:check failed with ${failures} L0/WCAG failure(s).`);
    process.exit(1);
  }
  console.log("color:check passed.");
}

main();
