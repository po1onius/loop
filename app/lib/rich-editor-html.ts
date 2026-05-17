export const EVENT_RICH_EDITOR_HTML = String.raw`<!doctype html>
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
      }

      @media (prefers-color-scheme: dark) {
        :root {
          --bg: #151718;
          --panel: #1e252c;
          --border: #2f3a45;
          --text: #ecedee;
          --muted: #9ba1a6;
          --quote: #172c35;
        }
      }

      * {
        box-sizing: border-box;
      }

      html,
      body {
        width: 100%;
        min-height: 100%;
        margin: 0;
        background: var(--bg);
        color: var(--text);
        font-family:
          -apple-system, BlinkMacSystemFont, "Segoe UI", "PingFang SC",
          "Hiragino Sans GB", "Microsoft YaHei", sans-serif;
      }

      body {
        overscroll-behavior: contain;
      }

      .toolbar {
        position: sticky;
        top: 0;
        z-index: 5;
        display: flex;
        gap: 6px;
        overflow-x: auto;
        padding: 8px;
        border-bottom: 1px solid var(--border);
        background: var(--bg);
        background: color-mix(in srgb, var(--bg) 92%, transparent);
        -webkit-backdrop-filter: blur(14px);
        backdrop-filter: blur(14px);
      }

      .toolbar button {
        flex: 0 0 auto;
        min-width: 38px;
        height: 34px;
        border: 1px solid var(--border);
        border-radius: 8px;
        background: var(--panel);
        color: var(--text);
        font-size: 14px;
        font-weight: 600;
      }

      .toolbar .primary {
        border-color: var(--accent);
        background: var(--accent);
        color: #ffffff;
      }

      #editor {
        min-height: calc(100vh - 52px);
        padding: 16px 14px 42px;
        outline: none;
        -webkit-user-select: text;
        user-select: text;
        word-break: break-word;
      }

      #editor:empty::before {
        content: attr(data-placeholder);
        color: var(--muted);
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

      figure img {
        display: block;
        width: 100%;
        max-height: 480px;
        object-fit: contain;
        background: #000000;
      }

      figure figcaption {
        padding: 8px 10px;
        color: var(--muted);
        font-size: 13px;
        line-height: 18px;
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
      <button type="button" data-block="P">正文</button>
      <button type="button" data-block="H2">H2</button>
      <button type="button" data-block="H3">H3</button>
      <button type="button" data-command="bold">B</button>
      <button type="button" data-command="italic">I</button>
      <button type="button" data-command="underline">U</button>
      <button type="button" data-block="BLOCKQUOTE">引用</button>
      <button type="button" data-action="divider">分割线</button>
      <button type="button" class="primary" data-action="image">图片</button>
    </nav>
    <main
      id="editor"
      contenteditable="true"
      data-placeholder="写下任务详情，可以像专栏一样穿插图片。"
    >
      <p data-block-id="p_initial"><br /></p>
    </main>

    <script>
      (() => {
        const editor = document.getElementById("editor");
        let savedRange = null;

        const post = (message) => {
          window.ReactNativeWebView?.postMessage(JSON.stringify(message));
        };

        const log = (level, message, extra) => {
          post({ type: "log", level, message, extra: extra || null });
        };

        const createBlockId = () =>
          "b_" +
          Date.now().toString(36) +
          "_" +
          Math.random().toString(36).slice(2, 8);

        const saveSelection = () => {
          const selection = window.getSelection();
          if (!selection || selection.rangeCount === 0) {
            return;
          }
          const range = selection.getRangeAt(0);
          if (editor.contains(range.commonAncestorContainer)) {
            savedRange = range.cloneRange();
          }
        };

        const restoreSelection = () => {
          if (!savedRange) {
            editor.focus();
            return;
          }
          const selection = window.getSelection();
          selection.removeAllRanges();
          selection.addRange(savedRange);
        };

        const getBlockElement = (node) => {
          let current = node?.nodeType === Node.ELEMENT_NODE ? node : node?.parentElement;
          while (current && current !== editor) {
            if (
              ["P", "H2", "H3", "BLOCKQUOTE", "FIGURE", "HR"].includes(
                current.tagName,
              )
            ) {
              return current;
            }
            current = current.parentElement;
          }
          return null;
        };

        const ensureBlockId = (element) => {
          if (!element.dataset.blockId) {
            element.dataset.blockId = createBlockId();
          }
          return element.dataset.blockId;
        };

        const emptyParagraph = () => {
          const paragraph = document.createElement("p");
          paragraph.dataset.blockId = createBlockId();
          paragraph.appendChild(document.createElement("br"));
          return paragraph;
        };

        const insertNodeAfterCurrentBlock = (node) => {
          restoreSelection();
          const selection = window.getSelection();
          const range = selection.rangeCount > 0 ? selection.getRangeAt(0) : null;
          const currentBlock = getBlockElement(range?.commonAncestorContainer);
          if (currentBlock && currentBlock.parentNode === editor) {
            currentBlock.insertAdjacentElement("afterend", node);
          } else {
            editor.appendChild(node);
          }

          const nextParagraph = emptyParagraph();
          node.insertAdjacentElement("afterend", nextParagraph);
          const nextRange = document.createRange();
          nextRange.selectNodeContents(nextParagraph);
          nextRange.collapse(true);
          selection.removeAllRanges();
          selection.addRange(nextRange);
          savedRange = nextRange.cloneRange();
        };

        const applyBlock = (tagName) => {
          restoreSelection();
          document.execCommand("formatBlock", false, tagName);
          saveSelection();
        };

        const insertDivider = () => {
          const divider = document.createElement("hr");
          divider.dataset.blockId = createBlockId();
          insertNodeAfterCurrentBlock(divider);
        };

        const insertUploadedImage = (payload) => {
          const figure = document.createElement("figure");
          figure.dataset.blockId = createBlockId();
          figure.dataset.assetId = payload.assetId;
          figure.dataset.width = String(payload.width || 1);
          figure.dataset.height = String(payload.height || 1);
          figure.contentEditable = "false";

          const image = document.createElement("img");
          image.src = payload.uri || payload.publicUrl || "";
          image.alt = payload.alt || "任务图片";
          image.loading = "lazy";

          const caption = document.createElement("figcaption");
          caption.textContent = "图片已上传";

          figure.appendChild(image);
          figure.appendChild(caption);
          insertNodeAfterCurrentBlock(figure);
        };

        const mergeMark = (marks, mark) => {
          if (!marks.some((item) => item.type === mark.type)) {
            marks.push(mark);
          }
          return marks;
        };

        const textOf = (node) => (node.textContent || "").replace(/\s+/g, " ");

        const sameMarks = (left, right) =>
          left.length === right.length &&
          left.every((item, index) => item.type === right[index].type);

        const pushText = (nodes, text, marks) => {
          const normalized = text.replace(/\s+/g, " ");
          if (!normalized.trim()) {
            return;
          }
          const last = nodes[nodes.length - 1];
          if (last?.type === "text" && sameMarks(last.marks, marks)) {
            last.text += normalized;
            return;
          }
          nodes.push({ type: "text", text: normalized, marks });
        };

        const collectInlineNodes = (root) => {
          const nodes = [];
          const walk = (node, inheritedMarks) => {
            if (node.nodeType === Node.TEXT_NODE) {
              pushText(nodes, node.textContent || "", inheritedMarks);
              return;
            }
            if (node.nodeType !== Node.ELEMENT_NODE) {
              return;
            }

            const element = node;
            if (element.tagName === "BR") {
              pushText(nodes, " ", inheritedMarks);
              return;
            }

            if (element.tagName === "A") {
              const text = textOf(element).trim();
              const url = element.getAttribute("href") || "";
              if (text && /^https?:\/\//i.test(url)) {
                nodes.push({ type: "link", text, url });
                return;
              }
            }

            const marks = inheritedMarks.slice();
            const style = window.getComputedStyle(element);
            if (["B", "STRONG"].includes(element.tagName) || style.fontWeight >= 600) {
              mergeMark(marks, { type: "bold" });
            }
            if (["I", "EM"].includes(element.tagName) || style.fontStyle === "italic") {
              mergeMark(marks, { type: "italic" });
            }
            if (
              element.tagName === "U" ||
              style.textDecorationLine.includes("underline")
            ) {
              mergeMark(marks, { type: "underline" });
            }

            Array.from(element.childNodes).forEach((child) => walk(child, marks));
          };

          Array.from(root.childNodes).forEach((child) => walk(child, []));
          return nodes;
        };

        const serializeTextBlock = (element, type, level) => {
          const children = collectInlineNodes(element);
          if (children.length === 0) {
            return null;
          }
          const block = {
            type,
            id: ensureBlockId(element),
            children,
          };
          if (typeof level === "number") {
            block.level = level;
          }
          return block;
        };

        const serializeElement = (element) => {
          switch (element.tagName) {
            case "H2":
              return serializeTextBlock(element, "heading", 2);
            case "H3":
              return serializeTextBlock(element, "heading", 3);
            case "BLOCKQUOTE":
              return serializeTextBlock(element, "quote");
            case "FIGURE": {
              const assetId = element.dataset.assetId;
              if (!assetId) {
                return null;
              }
              return {
                type: "image",
                id: ensureBlockId(element),
                item: {
                  asset_id: assetId,
                  width: Math.max(1, Number(element.dataset.width) || 1),
                  height: Math.max(1, Number(element.dataset.height) || 1),
                  alt: null,
                },
                caption: null,
              };
            }
            case "HR":
              return { type: "divider", id: ensureBlockId(element) };
            case "P":
            case "DIV":
            default:
              return serializeTextBlock(element, "paragraph");
          }
        };

        const exportContent = (requestId) => {
          try {
            const blocks = Array.from(editor.childNodes)
              .map((node) => {
                if (node.nodeType === Node.TEXT_NODE) {
                  const paragraph = document.createElement("p");
                  paragraph.textContent = node.textContent || "";
                  return serializeTextBlock(paragraph, "paragraph");
                }
                if (node.nodeType === Node.ELEMENT_NODE) {
                  return serializeElement(node);
                }
                return null;
              })
              .filter(Boolean);

            const textLength = blocks.reduce((total, block) => {
              if (!block.children) {
                return total;
              }
              return (
                total +
                block.children.reduce((sum, child) => {
                  if (child.type === "text" || child.type === "link") {
                    return sum + child.text.length;
                  }
                  if (child.type === "mention") {
                    return sum + child.label.length;
                  }
                  if (child.type === "hashtag") {
                    return sum + child.text.length;
                  }
                  return sum;
                }, 0)
              );
            }, 0);

            post({
              type: "content",
              requestId,
              doc: {
                schema_version: 2,
                blocks,
              },
              textLength,
              imageCount: blocks.filter(
                (block) => block.type === "image" || block.type === "image_grid",
              ).length,
            });
          } catch (error) {
            log("error", "export editor content failed", String(error));
          }
        };

        document.addEventListener("selectionchange", saveSelection);
        editor.addEventListener("focus", saveSelection);
        editor.addEventListener("keyup", saveSelection);
        editor.addEventListener("mouseup", saveSelection);

        document.querySelectorAll("button[data-command]").forEach((button) => {
          button.addEventListener("click", () => {
            restoreSelection();
            document.execCommand(button.dataset.command, false);
            saveSelection();
          });
        });

        document.querySelectorAll("button[data-block]").forEach((button) => {
          button.addEventListener("click", () => applyBlock(button.dataset.block));
        });

        document
          .querySelector("[data-action='divider']")
          .addEventListener("click", insertDivider);

        document.querySelector("[data-action='image']").addEventListener("click", () => {
          saveSelection();
          post({ type: "pick_image" });
        });

        window.loopEditor = {
          exportContent,
          insertUploadedImage,
        };

        post({ type: "ready" });
      })();
    </script>
  </body>
</html>`;
