// Lightweight, safe Markdown renderer for ToxSocial posts.
// HTML is escaped first, then a small subset of Markdown is applied.
// Only http/https/mailto links are allowed.

function escapeHtml(s: string): string {
  return s
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#39;");
}

function safeUrl(url: string): string {
  const trimmed = url.trim();
  if (/^(https?:|mailto:)/i.test(trimmed)) return trimmed;
  return "#";
}

function inline(text: string): string {
  let s = text;

  // Images / videos: ![alt](url), with `video:` prefix for video links
  s = s.replace(
    /!\[([^\]]*)\]\(([^)\s]+)\)/g,
    (_m, alt: string, url: string) => {
      const safe = safeUrl(url);
      if (alt.trim().toLowerCase().startsWith("video:")) {
        return `<video src="${safe}" controls preload="metadata"></video>`;
      }
      return `<img src="${safe}" alt="${alt}" loading="lazy" />`;
    },
  );

  // Links: [text](url)
  s = s.replace(
    /\[([^\]]+)\]\(([^)\s]+)\)/g,
    (_m, text: string, url: string) =>
      `<a href="${safeUrl(url)}" target="_blank" rel="noopener noreferrer">${text}</a>`,
  );

  // Inline code: `code`
  s = s.replace(/`([^`]+)`/g, "<code>$1</code>");

  // Bold: **text**
  s = s.replace(/\*\*([^*]+)\*\*/g, "<strong>$1</strong>");

  // Italic: *text* and _text_ (avoid matching inside words too aggressively)
  s = s.replace(/(^|[^*])\*([^*\n]+)\*/g, "$1<em>$2</em>");
  s = s.replace(/(^|[^_])_([^_\n]+)_/g, "$1<em>$2</em>");

  return s;
}

export function renderMarkdown(src: string): string {
  const escaped = escapeHtml(src);
  const lines = escaped.split(/\r?\n/);
  let html = "";
  let i = 0;
  let inCode = false;
  let codeBuf: string[] = [];
  let listType: "ul" | "ol" | null = null;
  let paragraph: string[] = [];

  const flushParagraph = () => {
    if (paragraph.length > 0) {
      html += `<p>${inline(paragraph.join("<br>"))}</p>`;
      paragraph = [];
    }
  };

  const flushList = () => {
    if (listType) {
      html += `</${listType}>`;
      listType = null;
    }
  };

  while (i < lines.length) {
    const line = lines[i];
    const trimmed = line.trim();

    if (/^```/.test(trimmed)) {
      flushParagraph();
      flushList();
      if (!inCode) {
        inCode = true;
        codeBuf = [];
      } else {
        html += `<pre><code>${codeBuf.join("\n")}</code></pre>`;
        inCode = false;
      }
      i++;
      continue;
    }

    if (inCode) {
      codeBuf.push(line);
      i++;
      continue;
    }

    if (!trimmed) {
      flushParagraph();
      flushList();
      i++;
      continue;
    }

    const heading = trimmed.match(/^(#{1,6})\s+(.*)$/);
    if (heading) {
      flushParagraph();
      flushList();
      const level = heading[1].length;
      html += `<h${level}>${inline(heading[2])}</h${level}>`;
      i++;
      continue;
    }

    if (/^([-*_])\s*(\1\s*){2,}$/.test(trimmed)) {
      flushParagraph();
      flushList();
      html += "<hr>";
      i++;
      continue;
    }

    if (/^>\s?/.test(trimmed)) {
      flushParagraph();
      flushList();
      html += `<blockquote>${inline(trimmed.replace(/^>\s?/, ""))}</blockquote>`;
      i++;
      continue;
    }

    const ul = trimmed.match(/^[-*+]\s+(.*)$/);
    const ol = trimmed.match(/^\d+\.\s+(.*)$/);
    if (ul || ol) {
      flushParagraph();
      const type = ul ? "ul" : "ol";
      const content = ul ? ul[1] : ol![1];
      if (listType !== type) {
        flushList();
        listType = type;
        html += `<${type}>`;
      }
      html += `<li>${inline(content)}</li>`;
      i++;
      continue;
    }

    flushList();
    paragraph.push(line);
    i++;
  }

  flushParagraph();
  flushList();
  if (inCode) {
    html += `<pre><code>${codeBuf.join("\n")}</code></pre>`;
  }

  return html;
}
