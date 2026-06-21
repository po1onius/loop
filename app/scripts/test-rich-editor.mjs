import { JSDOM } from "jsdom";
import { readFile } from "node:fs/promises";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const generatedModule = await readFile(resolve(root, "lib/rich-editor-html.ts"), "utf8");
const htmlMatch = generatedModule.match(/EVENT_RICH_EDITOR_HTML = (.*);\n?$/s);
if (!htmlMatch) {
  throw new Error("generated rich editor module does not export HTML");
}
const EVENT_RICH_EDITOR_HTML = JSON.parse(htmlMatch[1]);

const messages = [];
const dom = new JSDOM(EVENT_RICH_EDITOR_HTML, {
  beforeParse(window) {
    window.ReactNativeWebView = {
      postMessage(message) {
        messages.push(JSON.parse(message));
      },
    };
  },
  pretendToBeVisual: true,
  runScripts: "dangerously",
  url: "https://loop.local/editor",
});

await waitForMessage("ready");

dom.window.loopEditor.exportContent("empty_doc");
const emptyDoc = await waitForMessage("content", (message) => {
  return message.requestId === "empty_doc";
});
assert(emptyDoc.doc.version === 1, "empty doc should use content version 1");
assert(Array.isArray(emptyDoc.doc.blocks), "empty doc blocks should be an array");
assert(emptyDoc.doc.blocks.length === 0, "empty editor should export no blocks");

dom.window.loopEditor.insertUploadedImage({
  assetId: "asset_test",
  uri: "file:///tmp/test.jpg",
  width: 640,
  height: 480,
  alt: "测试图片",
});
dom.window.loopEditor.exportContent("image_doc");
const imageDoc = await waitForMessage("content", (message) => {
  return message.requestId === "image_doc";
});
const imageBlock = imageDoc.doc.blocks.find((block) => block.type === "image");
assert(imageBlock, "inserted image should be exported as image block");
assert(imageBlock.item.asset_id === "asset_test", "image block should preserve asset id");
assert(imageDoc.imageCount === 1, "image count should include inserted image");

dom.window.loopEditor.exportContent("image_doc_again");
const imageDocAgain = await waitForMessage("content", (message) => {
  return message.requestId === "image_doc_again";
});
const imageBlockAgain = imageDocAgain.doc.blocks.find((block) => block.type === "image");
assert(imageBlockAgain, "inserted image should still be exported as image block");
assert(
  imageBlockAgain.id === imageBlock.id,
  "block id should remain stable across repeated exports",
);

console.info("rich editor WebView contract ok");

function waitForMessage(type, predicate = () => true) {
  const existing = messages.find((message) => message.type === type && predicate(message));
  if (existing) {
    return Promise.resolve(existing);
  }
  return new Promise((resolve, reject) => {
    const deadline = Date.now() + 4000;
    const timer = setInterval(() => {
      const message = messages.find((item) => item.type === type && predicate(item));
      if (message) {
        clearInterval(timer);
        resolve(message);
        return;
      }
      if (Date.now() > deadline) {
        clearInterval(timer);
        reject(new Error(`timed out waiting for ${type}`));
      }
    }, 20);
  });
}

function assert(condition, message) {
  if (!condition) {
    throw new Error(message);
  }
}
