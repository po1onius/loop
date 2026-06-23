import { build } from "esbuild";
import { mkdir, writeFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const entry = resolve(root, "lib/rich-editor-webview.ts");
const output = resolve(root, "lib/rich-editor-html.ts");

const result = await build({
  absWorkingDir: root,
  bundle: true,
  entryPoints: [entry],
  format: "iife",
  logLevel: "silent",
  minify: true,
  platform: "browser",
  target: ["es2020"],
  treeShaking: true,
  write: false,
});

const script = result.outputFiles[0]?.text;
if (!script) {
  throw new Error("rich editor bundle is empty");
}

const html = `<!doctype html>
<html lang="zh-CN">
  <head>
    <meta charset="utf-8" />
    <meta
      name="viewport"
      content="width=device-width, initial-scale=1, maximum-scale=1, user-scalable=no"
    />
    <style>
      :root {
        color-scheme: light dark;
        --bg: #ffffff;
        --panel: #f6f8fb;
        --border: #d9e0ea;
        --text: #11181c;
        --muted: #687076;
        --accent: #0a7ea4;
        --quote: #eef7fb;
        --image-check-a: #eef2f6;
        --image-check-b: #ffffff;
        --danger: #c2410c;
      }

      @media (prefers-color-scheme: dark) {
        :root {
          --bg: #151718;
          --panel: #1e252c;
          --border: #2f3a45;
          --text: #ecedee;
          --muted: #9ba1a6;
          --quote: #172c35;
          --image-check-a: #2c333a;
          --image-check-b: #20272e;
          --danger: #fb923c;
        }
      }

      * {
        box-sizing: border-box;
      }

      html,
      body {
        width: 100%;
        height: 100%;
        margin: 0;
        background: var(--bg);
        color: var(--text);
        font-family:
          -apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC",
          "Hiragino Sans GB", "Microsoft YaHei", sans-serif;
      }

      body {
        display: flex;
        flex-direction: column;
        overflow: hidden;
        overscroll-behavior: contain;
      }

      .toolbar {
        flex: 0 0 auto;
        z-index: 5;
        display: flex;
        flex-wrap: wrap;
        align-items: center;
        gap: 6px;
        overflow-x: hidden;
        overflow-y: visible;
        padding: 8px;
        border-bottom: 1px solid var(--border);
        background: var(--bg);
        background: color-mix(in srgb, var(--bg) 92%, transparent);
        -webkit-backdrop-filter: blur(14px);
        backdrop-filter: blur(14px);
      }

      .toolbar button {
        flex: 0 1 auto;
        min-width: 38px;
        height: 34px;
        padding: 0 10px;
        border: 1px solid var(--border);
        border-radius: 8px;
        background: var(--panel);
        color: var(--text);
        font-size: 14px;
        font-weight: 600;
      }

      .toolbar .primary {
        border-color: var(--accent);
        background: color-mix(in srgb, var(--accent) 12%, var(--panel));
        color: var(--accent);
      }

      .toolbar button.active {
        border-color: var(--accent);
        background: var(--accent);
        color: #ffffff;
      }

      #editor {
        flex: 1 1 auto;
        min-height: 0;
        overflow-y: auto;
        -webkit-overflow-scrolling: touch;
        outline: none;
      }

      .tiptap {
        min-height: 100%;
        padding: 16px 14px 42px;
        outline: none;
        -webkit-user-select: text;
        user-select: text;
        word-break: break-word;
      }

      .tiptap p.is-editor-empty:first-child::before {
        content: attr(data-placeholder);
        float: left;
        height: 0;
        color: var(--muted);
        pointer-events: none;
      }

      p,
      h2,
      h3,
      blockquote {
        margin: 0 0 14px;
      }

      p {
        min-height: 24px;
        font-size: 17px;
        line-height: 1.65;
      }

      h2 {
        margin-top: 6px;
        font-size: 23px;
        line-height: 1.35;
      }

      h3 {
        margin-top: 4px;
        font-size: 19px;
        line-height: 1.45;
      }

      blockquote {
        border-left: 4px solid var(--accent);
        border-radius: 8px;
        padding: 10px 12px;
        background: var(--quote);
        color: var(--text);
        font-size: 16px;
        line-height: 1.6;
      }

      figure {
        margin: 16px 0;
        border: 1px solid var(--border);
        border-radius: 10px;
        overflow: hidden;
        background: var(--panel);
      }

      figure[data-upload-state="uploading"] {
        border-style: dashed;
      }

      figure[data-upload-state="failed"] {
        border-color: var(--danger);
      }

      figure img {
        display: block;
        width: 100%;
        max-height: 480px;
        object-fit: contain;
        background-color: var(--image-check-b);
        background-image:
          linear-gradient(45deg, var(--image-check-a) 25%, transparent 25%),
          linear-gradient(-45deg, var(--image-check-a) 25%, transparent 25%),
          linear-gradient(45deg, transparent 75%, var(--image-check-a) 75%),
          linear-gradient(-45deg, transparent 75%, var(--image-check-a) 75%);
        background-position:
          0 0,
          0 8px,
          8px -8px,
          -8px 0;
        background-size: 16px 16px;
      }

      figure figcaption {
        padding: 8px 10px;
        color: var(--muted);
        font-size: 13px;
        line-height: 18px;
      }

      figure[data-upload-state="uploading"] figcaption {
        color: var(--accent);
      }

      figure[data-upload-state="failed"] figcaption {
        color: var(--danger);
      }

      hr {
        height: 1px;
        margin: 18px 0;
        border: 0;
        background: var(--border);
      }

      a {
        color: var(--accent);
      }
    </style>
  </head>
  <body>
    <nav class="toolbar" aria-label="编辑工具">
      <button type="button" data-block="paragraph">正文</button>
      <button type="button" data-block="heading2">H2</button>
      <button type="button" data-block="heading3">H3</button>
      <button type="button" data-command="bold">B</button>
      <button type="button" data-command="italic">I</button>
      <button type="button" data-command="underline">U</button>
      <button type="button" data-action="link">链接</button>
      <button type="button" data-block="blockquote">引用</button>
      <button type="button" data-action="divider">分割线</button>
      <button type="button" class="primary" data-action="image">图片</button>
    </nav>
    <main id="editor"></main>
    <script>${script}</script>
  </body>
</html>`;

const escaped = JSON.stringify(html);
await mkdir(dirname(output), { recursive: true });
await writeFile(
  output,
  `// Generated by scripts/build-rich-editor.mjs. Do not edit by hand.\nexport const EVENT_RICH_EDITOR_HTML = ${escaped};\n`,
);

console.info(`Generated ${output}`);
