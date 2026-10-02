// What an edit changed, line by line, for the flyout's code view. Pure, like
// sessions.ts, so it is tested directly.
//
// Two sources. Before an Edit or Write runs (and while it waits for your
// permission) only its text is known: old against new, without line numbers.
// Once it ran, the relay passes on Claude Code's own patch (and nothing else of
// the result, see hook/src/patch.rs), with the file's line numbers.

export type LineKind = "ctx" | "del" | "add" | "gap";

export interface EditLine {
  kind: LineKind;
  /** Line number: in the file before for a removed line, after for the rest. */
  num: number | null;
  text: string;
}

export type EditStatus = "pending" | "applied" | "failed";

export interface FileEdit {
  path: string;
  name: string;
  /** "TS", "RS", "MD"… from the extension. */
  badge: string;
  lines: EditLine[];
  /** Line numbers are the file's own (from Claude Code's patch), not guessed. */
  numbered: boolean;
  status: EditStatus;
  added: number;
  removed: number;
  /** Not everything is shown: a long text was cut on the way, or the patch was capped. */
  cut: boolean;
  replaceAll: boolean;
  created: boolean;
}

/** Lines kept per edit. */
export const SHOWN_LINES = 40;

/** The relay cuts every string at this many bytes and appends "…" (hook/src/main.rs). */
const RELAY_FIELD_CAP = 2000;
/** Above this many old × new lines the diff is not worth working out. */
const MAX_DIFF_CELLS = 250_000;

const utf8 = new TextEncoder();

/** Cut by the relay: as long as it lets through, ending in its ellipsis. */
function isCut(s: string): boolean {
  return s.endsWith("…") && utf8.encode(s).length >= RELAY_FIELD_CAP;
}

function baseName(p: string): string {
  const cleaned = p.replace(/[\\/]+$/, "");
  return cleaned.slice(Math.max(cleaned.lastIndexOf("\\"), cleaned.lastIndexOf("/")) + 1);
}

function split(text: string): string[] {
  return text === "" ? [] : text.split(/\r?\n/);
}

function str(input: Record<string, unknown>, key: string): string | null {
  const v = input[key];
  return typeof v === "string" ? v : null;
}

/** `path` relative to the session's folder when it is inside it, else whole. */
export function shownPath(path: string, cwd: string): string {
  // Windows paths: either slash, any case.
  const norm = (p: string) => p.replace(/\//g, "\\").toLowerCase();
  const root = norm(cwd).replace(/\\+$/, "");
  if (!root || !norm(path).startsWith(`${root}\\`)) return path;
  return path.slice(root.length + 1);
}

export function fileBadge(path: string): string {
  const name = baseName(path);
  const dot = name.lastIndexOf(".");
  const ext = dot >= 0 ? name.slice(dot + 1) : "";
  return ext ? ext.slice(0, 4).toUpperCase() : "TXT";
}

const line = (kind: LineKind, text: string, num: number | null = null): EditLine => ({ kind, num, text });

/** Old lines against new, removals before additions; no line numbers. */
export function diffLines(before: string[], after: string[]): EditLine[] {
  const n = before.length;
  const m = after.length;
  if (n * m > MAX_DIFF_CELLS) {
    return [...before.map((t) => line("del", t)), ...after.map((t) => line("add", t))];
  }
  // Longest common subsequence, from the end.
  const w = m + 1;
  const t = new Uint32Array((n + 1) * w);
  for (let i = n - 1; i >= 0; i--) {
    for (let j = m - 1; j >= 0; j--) {
      t[i * w + j] = before[i] === after[j] ? t[(i + 1) * w + j + 1] + 1 : Math.max(t[(i + 1) * w + j], t[i * w + j + 1]);
    }
  }
  const out: EditLine[] = [];
  let i = 0;
  let j = 0;
  while (i < n && j < m) {
    if (before[i] === after[j]) {
      out.push(line("ctx", before[i]));
      i++;
      j++;
    } else if (t[(i + 1) * w + j] >= t[i * w + j + 1]) {
      out.push(line("del", before[i++]));
    } else {
      out.push(line("add", after[j++]));
    }
  }
  while (i < n) out.push(line("del", before[i++]));
  while (j < m) out.push(line("add", after[j++]));
  return out;
}

interface Facts {
  numbered: boolean;
  status: EditStatus;
  cut: boolean;
  replaceAll: boolean;
  created: boolean;
}

function build(path: string, lines: EditLine[], facts: Facts): FileEdit {
  return {
    path,
    name: baseName(path),
    badge: fileBadge(path),
    lines: lines.slice(0, SHOWN_LINES),
    added: lines.filter((l) => l.kind === "add").length,
    removed: lines.filter((l) => l.kind === "del").length,
    ...facts,
    cut: facts.cut || lines.length > SHOWN_LINES,
  };
}

/** A whole file's text as new lines, numbered from one. */
function allNew(content: string): EditLine[] {
  return split(content).map((text, i) => line("add", text, i + 1));
}

/** An Edit or Write about to run (or waiting for permission): its text, as far as it goes. */
export function editFromPre(tool: string, input: Record<string, unknown>): FileEdit | null {
  const path = str(input, "file_path");
  if (!path) return null;
  if (tool === "Edit") {
    const before = str(input, "old_string") ?? "";
    const after = str(input, "new_string") ?? "";
    return build(path, diffLines(split(before), split(after)), {
      numbered: false, status: "pending", cut: isCut(before) || isCut(after), replaceAll: input.replace_all === true, created: false,
    });
  }
  if (tool === "Write") {
    const content = str(input, "content") ?? "";
    return build(path, allNew(content), { numbered: true, status: "pending", cut: isCut(content), replaceAll: false, created: false });
  }
  return null;
}

/** An Edit or Write that ran, from the patch the relay passed on; null without one. */
export function editFromPost(tool: string, input: Record<string, unknown>, response: unknown): FileEdit | null {
  if (tool !== "Edit" && tool !== "Write") return null;
  const path = str(input, "file_path");
  if (!path || !response || typeof response !== "object") return null;
  const r = response as Record<string, unknown>;
  const hunks = r.structuredPatch;
  if (!Array.isArray(hunks)) return null;

  const created = r.type === "create";
  let cut = r.truncated === true;
  let lines: EditLine[] = [];
  if (created && hunks.length === 0) {
    // A new file has no "before": Claude Code sends no hunk, the text is the change.
    const content = str(input, "content") ?? "";
    cut ||= isCut(content);
    lines = allNew(content);
  } else {
    for (const hunk of hunks) {
      if (!hunk || typeof hunk !== "object") continue;
      const h = hunk as Record<string, unknown>;
      if (lines.length > 0) lines.push(line("gap", ""));
      let oldNo = typeof h.oldStart === "number" ? h.oldStart : 0;
      let newNo = typeof h.newStart === "number" ? h.newStart : 0;
      for (const raw of Array.isArray(h.lines) ? h.lines : []) {
        if (typeof raw !== "string") continue;
        cut ||= isCut(raw);
        const text = raw.slice(1).replace(/\r$/, "");
        switch (raw[0]) {
          case " ":
            lines.push(line("ctx", text, newNo++));
            oldNo++;
            break;
          case "-":
            lines.push(line("del", text, oldNo++));
            break;
          case "+":
            lines.push(line("add", text, newNo++));
            break;
          default:
            break; // "\ No newline at end of file" and the like
        }
      }
    }
  }
  return build(path, lines, { numbered: true, status: "applied", cut, replaceAll: input.replace_all === true, created });
}
