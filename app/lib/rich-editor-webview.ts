import CharacterCount from "@tiptap/extension-character-count";
import Link from "@tiptap/extension-link";
import Placeholder from "@tiptap/extension-placeholder";
import Underline from "@tiptap/extension-underline";
import { Editor, Extension, Node, mergeAttributes } from "@tiptap/core";
import StarterKit from "@tiptap/starter-kit";

type EventTextMark =
  | { type: "bold" }
  | { type: "italic" }
  | { type: "underline" };

type EventInlineNode =
  | { type: "text"; text: string; marks: EventTextMark[] }
  | { type: "link"; text: string; url: string };

type EventContentImage = {
  asset_id: string;
  width: number;
  height: number;
  alt: string | null;
};

type EventContentBlock =
  | { type: "heading"; id: string; level: number; children: EventInlineNode[] }
  | { type: "paragraph"; id: string; children: EventInlineNode[] }
  | { type: "quote"; id: string; children: EventInlineNode[] }
  | { type: "image"; id: string; item: EventContentImage; caption: string | null }
  | { type: "divider"; id: string };

type UploadedImagePayload = {
  assetId: string;
  uri: string;
  publicUrl?: string;
  width: number;
  height: number;
  alt?: string;
};

type ProseMirrorNode = {
  type?: string;
  text?: string;
  attrs?: Record<string, unknown> | null;
  marks?: ProseMirrorMark[];
  content?: ProseMirrorNode[];
};

type ProseMirrorMark = {
  type?: string;
  attrs?: Record<string, unknown> | null;
};

type NativeBridge = {
  postMessage?: (message: string) => void;
};

declare global {
  interface Window {
    ReactNativeWebView?: NativeBridge;
    loopEditor?: {
      exportContent: (requestId: string) => void;
      insertUploadedImage: (payload: UploadedImagePayload) => void;
    };
  }
}

const EVENT_CONTENT_VERSION = 1;
const MAX_TEXT_CHARS = 10_000;

const post = (message: Record<string, unknown>) => {
  window.ReactNativeWebView?.postMessage?.(JSON.stringify(message));
};

const log = (level: "info" | "warn" | "error", message: string, extra?: unknown) => {
  post({ type: "log", level, message, extra: extra ?? null });
};

const createBlockId = () =>
  `b_${Date.now().toString(36)}_${Math.random().toString(36).slice(2, 8)}`;

let editor: Editor | null = null;
const buttons = Array.from(document.querySelectorAll<HTMLButtonElement>(".toolbar button"));

const EventBlockId = Extension.create({
  name: "eventBlockId",

  addGlobalAttributes() {
    return [
      {
        types: ["paragraph", "heading", "blockquote", "horizontalRule"],
        attributes: {
          eventBlockId: {
            default: null,
            parseHTML: (element) => element.getAttribute("data-event-block-id"),
            renderHTML: (attributes) => {
              const id = attributes["eventBlockId"];
              return typeof id === "string" && id.trim()
                ? { "data-event-block-id": id }
                : {};
            },
          },
        },
      },
    ];
  },
});

const EventImage = Node.create({
  name: "eventImage",
  group: "block",
  atom: true,
  draggable: true,

  addAttributes() {
    return {
      assetId: { default: "" },
      src: { default: "" },
      width: { default: 1 },
      height: { default: 1 },
      alt: { default: null },
      eventBlockId: { default: null },
    };
  },

  parseHTML() {
    return [{ tag: "figure[data-event-image]" }];
  },

  renderHTML({ HTMLAttributes }) {
    const src = String(HTMLAttributes["src"] || "");
    const alt = String(HTMLAttributes["alt"] || "活动图片");
    const width = Number(HTMLAttributes["width"]) || 1;
    const height = Number(HTMLAttributes["height"]) || 1;
    const ratio = Math.min(Math.max(width / height, 0.56), 2.4);
    return [
      "figure",
      mergeAttributes(HTMLAttributes, {
        "data-event-image": "true",
        contenteditable: "false",
      }),
      ["img", { src, alt, style: `aspect-ratio: ${ratio}` }],
      ["figcaption", {}, "图片已上传"],
    ];
  },
});

editor = new Editor({
  element: document.querySelector("#editor") as HTMLElement,
  extensions: [
    StarterKit.configure({
      blockquote: {},
      bold: {},
      bulletList: false,
      code: false,
      codeBlock: false,
      heading: { levels: [2, 3] },
      italic: {},
      link: false,
      listItem: false,
      orderedList: false,
      strike: false,
      underline: false,
    }),
    Underline,
    EventBlockId,
    Link.configure({
      autolink: true,
      defaultProtocol: "https",
      linkOnPaste: true,
      openOnClick: false,
    }),
    EventImage,
    Placeholder.configure({
      placeholder: "写下活动详情，可以像专栏一样穿插图片。",
    }),
    CharacterCount.configure({ limit: MAX_TEXT_CHARS }),
  ],
  content: { type: "doc", content: [{ type: "paragraph" }] },
  editorProps: {
    attributes: {
      "aria-label": "活动正文编辑器",
      spellcheck: "true",
    },
  },
  onCreate: () => {
    post({ type: "ready" });
    queueMicrotask(updateToolbarState);
  },
  onSelectionUpdate: () => updateToolbarState(),
  onTransaction: () => updateToolbarState(),
});

function setButtonActive(button: HTMLButtonElement, active: boolean) {
  button.classList.toggle("active", active);
  button.setAttribute("aria-pressed", active ? "true" : "false");
}

function updateToolbarState() {
  const currentEditor = editor;
  if (!currentEditor) {
    return;
  }

  buttons.forEach((button) => {
    const action = button.dataset["action"];
    const block = button.dataset["block"];
    const command = button.dataset["command"];
    if (block === "paragraph") {
      setButtonActive(button, currentEditor.isActive("paragraph"));
      return;
    }
    if (block === "heading2") {
      setButtonActive(button, currentEditor.isActive("heading", { level: 2 }));
      return;
    }
    if (block === "heading3") {
      setButtonActive(button, currentEditor.isActive("heading", { level: 3 }));
      return;
    }
    if (block === "blockquote") {
      setButtonActive(button, currentEditor.isActive("blockquote"));
      return;
    }
    if (command) {
      setButtonActive(button, currentEditor.isActive(command));
      return;
    }
    setButtonActive(button, action === "link" && currentEditor.isActive("link"));
  });
}

function applyToolbarAction(button: HTMLButtonElement) {
  const currentEditor = editor;
  if (!currentEditor) {
    return;
  }

  const block = button.dataset["block"];
  const command = button.dataset["command"];
  const action = button.dataset["action"];

  if (block === "paragraph") {
    currentEditor.chain().focus().setParagraph().run();
    return;
  }
  if (block === "heading2") {
    currentEditor.chain().focus().toggleHeading({ level: 2 }).run();
    return;
  }
  if (block === "heading3") {
    currentEditor.chain().focus().toggleHeading({ level: 3 }).run();
    return;
  }
  if (block === "blockquote") {
    currentEditor.chain().focus().toggleBlockquote().run();
    return;
  }
  if (command === "bold") {
    currentEditor.chain().focus().toggleBold().run();
    return;
  }
  if (command === "italic") {
    currentEditor.chain().focus().toggleItalic().run();
    return;
  }
  if (command === "underline") {
    currentEditor.chain().focus().toggleUnderline().run();
    return;
  }
  if (action === "divider") {
    currentEditor.chain().focus().setHorizontalRule().run();
    return;
  }
  if (action === "link") {
    toggleLink();
    return;
  }
  if (action === "image") {
    post({ type: "pick_image" });
  }
}

function toggleLink() {
  const currentEditor = editor;
  if (!currentEditor) {
    return;
  }

  const previousUrl = currentEditor.getAttributes("link")["href"] as
    | string
    | undefined;
  if (previousUrl) {
    currentEditor.chain().focus().unsetLink().run();
    return;
  }
  const url = window.prompt("输入链接地址");
  const normalizedUrl = normalizeUrl(url);
  if (!normalizedUrl) {
    return;
  }
  currentEditor
    .chain()
    .focus()
    .extendMarkRange("link")
    .setLink({ href: normalizedUrl })
    .run();
}

function normalizeUrl(url: string | null): string | null {
  const value = url?.trim();
  if (!value) {
    return null;
  }
  if (/^https?:\/\//i.test(value)) {
    return value;
  }
  return `https://${value}`;
}

buttons.forEach((button) => {
  button.addEventListener("click", () => applyToolbarAction(button));
});

function insertUploadedImage(payload: UploadedImagePayload) {
  const currentEditor = editor;
  if (!currentEditor) {
    return;
  }

  currentEditor
    .chain()
    .focus()
    .insertContent({
      type: "eventImage",
      attrs: {
        assetId: payload.assetId,
        src: payload.uri || payload.publicUrl || "",
        width: Math.max(1, Number(payload.width) || 1),
        height: Math.max(1, Number(payload.height) || 1),
        alt: payload.alt || "活动图片",
        eventBlockId: createBlockId(),
      },
    })
    .run();
}

function exportContent(requestId: string) {
  try {
    const currentEditor = editor;
    if (!currentEditor) {
      throw new Error("editor is not initialized");
    }
    assignMissingBlockIds(currentEditor);
    const doc = currentEditor.getJSON() as ProseMirrorNode;
    const blocks = toEventBlocks(doc);
    const textLength = countText(blocks);
    const imageCount = blocks.filter((block) => block.type === "image").length;
    post({
      type: "content",
      requestId,
      doc: {
        version: EVENT_CONTENT_VERSION,
        blocks,
      },
      textLength,
      imageCount,
    });
  } catch (error) {
    log("error", "export editor content failed", String(error));
  }
}

function assignMissingBlockIds(currentEditor: Editor) {
  let tr = currentEditor.state.tr;
  currentEditor.state.doc.forEach((node, offset) => {
    if (typeof node.attrs["eventBlockId"] === "string" && node.attrs["eventBlockId"].trim()) {
      return;
    }
    tr = tr.setNodeMarkup(offset, undefined, {
      ...node.attrs,
      eventBlockId: createBlockId(),
    });
  });
  if (tr.docChanged) {
    currentEditor.view.dispatch(tr);
  }
}

function toEventBlocks(doc: ProseMirrorNode): EventContentBlock[] {
  const content = Array.isArray(doc.content) ? doc.content : [];
  return content
    .map((node) => toEventBlock(node))
    .filter((block): block is EventContentBlock => Boolean(block));
}

function toEventBlock(node: ProseMirrorNode): EventContentBlock | null {
  const id = blockId(node);
  switch (node.type) {
    case "heading": {
      const children = inlineNodes(node);
      if (!children.length) {
        return null;
      }
      return {
        type: "heading",
        id,
        level: Number(node.attrs?.["level"]) === 3 ? 3 : 2,
        children,
      };
    }
    case "blockquote": {
      const children = flattenBlockquoteText(node);
      if (!children.length) {
        return null;
      }
      return { type: "quote", id, children };
    }
    case "eventImage":
      return {
        type: "image",
        id,
        item: {
          asset_id: String(node.attrs?.["assetId"] || ""),
          width: Math.max(1, Number(node.attrs?.["width"]) || 1),
          height: Math.max(1, Number(node.attrs?.["height"]) || 1),
          alt: nullableText(node.attrs?.["alt"]),
        },
        caption: null,
      };
    case "horizontalRule":
      return { type: "divider", id };
    case "paragraph":
    default: {
      const children = inlineNodes(node);
      if (!children.length) {
        return null;
      }
      return { type: "paragraph", id, children };
    }
  }
}

function blockId(node: ProseMirrorNode): string {
  const value = node.attrs?.["eventBlockId"];
  return typeof value === "string" && value.trim() ? value : createBlockId();
}

function inlineNodes(node: ProseMirrorNode): EventInlineNode[] {
  const result: EventInlineNode[] = [];
  for (const child of node.content || []) {
    appendInlineNode(result, child);
  }
  return mergeAdjacentText(result);
}

function flattenBlockquoteText(node: ProseMirrorNode): EventInlineNode[] {
  const result: EventInlineNode[] = [];
  for (const child of node.content || []) {
    if (child.content?.length) {
      inlineNodes(child).forEach((inlineNode) => result.push(inlineNode));
      continue;
    }
    appendInlineNode(result, child);
  }
  return mergeAdjacentText(result);
}

function appendInlineNode(result: EventInlineNode[], node: ProseMirrorNode) {
  if (node.type === "text") {
    const text = normalizeWhitespace(node.text || "");
    if (!text.trim()) {
      return;
    }
    const link = node.marks?.find((mark) => mark.type === "link");
    if (link?.attrs?.["href"]) {
      result.push({
        type: "link",
        text,
        url: String(link.attrs["href"]),
      });
      return;
    }
    result.push({ type: "text", text, marks: marksOf(node.marks || []) });
    return;
  }
  if (node.type === "hardBreak") {
    result.push({ type: "text", text: " ", marks: [] });
    return;
  }
  for (const child of node.content || []) {
    appendInlineNode(result, child);
  }
}

function marksOf(marks: ProseMirrorMark[]): EventTextMark[] {
  const result: EventTextMark[] = [];
  if (marks.some((mark) => mark.type === "bold")) {
    result.push({ type: "bold" });
  }
  if (marks.some((mark) => mark.type === "italic")) {
    result.push({ type: "italic" });
  }
  if (marks.some((mark) => mark.type === "underline")) {
    result.push({ type: "underline" });
  }
  return result;
}

function mergeAdjacentText(nodes: EventInlineNode[]): EventInlineNode[] {
  const merged: EventInlineNode[] = [];
  for (const node of nodes) {
    const last = merged[merged.length - 1];
    if (
      last?.type === "text" &&
      node.type === "text" &&
      JSON.stringify(last.marks) === JSON.stringify(node.marks)
    ) {
      last.text += node.text;
      continue;
    }
    merged.push(node);
  }
  return merged;
}

function normalizeWhitespace(value: string): string {
  return value.replace(/\s+/g, " ");
}

function nullableText(value: unknown): string | null {
  return typeof value === "string" && value.trim() ? value.trim() : null;
}

function countText(blocks: EventContentBlock[]): number {
  return blocks.reduce((total, block) => {
    if (!("children" in block)) {
      return total;
    }
    return (
      total +
      block.children.reduce((sum, child) => {
        if (child.type === "link" || child.type === "text") {
          return sum + child.text.length;
        }
        return sum;
      }, 0)
    );
  }, 0);
}

window.loopEditor = {
  exportContent,
  insertUploadedImage,
};
