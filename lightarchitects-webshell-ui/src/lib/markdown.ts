/**
 * Markdown-to-HTML rendering for agent/system chat bubbles.
 *
 * Agent responses (from `claude --resume`, Codex, etc.) come as GFM
 * markdown: `**bold**`, `` `code` ``, fenced code blocks, lists, etc.
 * Rendering them as plain text leaves the raw syntax visible to the
 * user, which is ugly. This module pipes the text through `marked`
 * (CommonMark + GFM) and then `DOMPurify` so the returned HTML is safe
 * to drop into Svelte's `{@html ...}` without XSS risk — even if a
 * compromised upstream agent emits `<script>` tags in its response.
 *
 * Keep this helper minimal: just markdown → sanitized HTML. Styling
 * lives in the consuming component's CSS.
 */
import { marked, type MarkedOptions } from 'marked';
import DOMPurify from 'dompurify';

// Inline-friendly markdown config. `breaks: true` turns single \n into
// <br> so multi-line agent replies render with their intended line
// breaks. `gfm: true` enables GitHub-flavored extensions (fenced code,
// tables, task lists, autolinks). `async: false` keeps the call sync
// so components can use it in reactive expressions without await.
const options: MarkedOptions = {
  gfm: true,
  breaks: true,
  async: false,
};

/**
 * Parse `text` as GFM markdown, then sanitize the resulting HTML.
 *
 * Returns an HTML string safe to embed via `{@html ...}`. Blank / null
 * input returns an empty string. Any parse/sanitize failure falls back
 * to the original text escaped for safety — the user sees their content
 * rather than a broken bubble.
 */
export function renderMarkdown(text: string | null | undefined): string {
  if (!text) return '';
  try {
    const rawHtml = marked.parse(text, options) as string;
    return DOMPurify.sanitize(rawHtml, {
      USE_PROFILES: { html: true },
      // Strip any remaining script/event-handler attributes as belt-and-suspenders
      FORBID_TAGS: ['script', 'style', 'iframe', 'object', 'embed'],
      FORBID_ATTR: ['onerror', 'onload', 'onclick', 'onmouseover', 'onfocus'],
    });
  } catch {
    // Escape-and-pass-through so the user still sees their text
    return text
      .replace(/&/g, '&amp;')
      .replace(/</g, '&lt;')
      .replace(/>/g, '&gt;');
  }
}
