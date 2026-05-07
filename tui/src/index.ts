// Retrieval X-Ray TUI
// ---------------------------------------------------------------------------
// Designed for monospace fonts with strong differentiation between similar
// glyphs (0 vs O, 1 vs l, etc.) and clean Unicode coverage. Recommended:
//   • 0xProto       https://github.com/0xType/0xProto  (preferred)
//   • JetBrains Mono, Berkeley Mono, Iosevka, IBM Plex Mono
// All alignment in this file assumes a 1-cell monospace grid.
// ---------------------------------------------------------------------------

process.env.OPENTUI_FORCE_EXPLICIT_WIDTH ??= "false";

import {
  BoxRenderable,
  FrameBufferRenderable,
  InputRenderable,
  InputRenderableEvents,
  RGBA,
  ScrollBoxRenderable,
  SelectRenderable,
  SelectRenderableEvents,
  TextAttributes,
  TextRenderable,
  createCliRenderer,
  type KeyEvent,
  type SelectOption,
} from "@opentui/core";

const BOLD = TextAttributes.BOLD;
import { spawn } from "node:child_process";

type DebugSearchResponse = {
  query: string;
  elapsed_ms: number;
  result_count: number;
  summary: DebugSummary;
  results: DebugHit[];
};

type DebugSummary = {
  query_terms: string[];
  short_query: boolean;
  score_gap_top1_top2: number;
  host_diversity: number;
  avg_vector_score: number;
  avg_lexical_score: number;
  avg_heading_overlap: number;
  avg_body_overlap: number;
  dense_dominant_hits: number;
  lexical_dominant_hits: number;
  authority_boosted_hits: number;
  exact_phrase_hits: number;
  risk_counts: DebugRiskCount[];
};

type DebugRiskCount = {
  code: string;
  label: string;
  count: number;
};

type DebugHit = {
  rank: number;
  chunk_id: string;
  source_url: string;
  display_url: string;
  host: string;
  heading_chain: string[];
  text: string;
  preview: string;
  matched_terms: string[];
  missing_terms: string[];
  diagnostics: DebugDiagnostic[];
  score_breakdown: DebugScoreBreakdown;
};

type DebugDiagnostic = {
  code: string;
  label: string;
  reason: string;
};

type DebugScoreBreakdown = {
  query_mode: string;
  vector_score: number;
  lexical_score: number;
  title_overlap: number;
  heading_overlap: number;
  body_overlap: number;
  vector_contribution: number;
  lexical_contribution: number;
  title_contribution: number;
  heading_contribution: number;
  body_contribution: number;
  base_total: number;
  phrase_bonus: number;
  penalty_multiplier: number;
  pre_authority_score: number;
  authority_bonus: number;
  final_score: number;
  dense_minus_lexical: number;
  exact_heading_phrase: boolean;
  exact_body_phrase: boolean;
  reconstruction_gap: number;
};

type DebugEvalResponse = {
  elapsed_ms: number;
  qrels_path: string;
  queries_path: string;
  top_k: number;
  num_queries: number;
  mrr: number;
  ndcg_at: MetricPoint[];
  recall_at: MetricPoint[];
  worst_queries: DebugQueryEvalRow[];
};

type MetricPoint = {
  k: number;
  value: number;
};

type DebugQueryEvalRow = {
  query_id: string;
  query: string;
  reciprocal_rank: number;
  first_relevant_rank: number | null;
  ndcg_at_top_k: number;
  recall_at_top_k: number;
  returned_relevant: string[];
  missed_relevant: string[];
};

type Palette = {
  BG: string;
  SURFACE: string;
  SURFACE_RAISED: string;
  BORDER: string;
  BORDER_VIS: string;
  TEXT_DIM: string;
  TEXT_SEC: string;
  TEXT_PRI: string;
  TEXT_DISP: string;
  ACCENT: string;
  ACCENT_RED: string;
  UTIL_ORANGE: string;
  SUCCESS: string;
  WARNING: string;
  INFO: string;
};

const DARK: Palette = {
  BG: "#0B0D10",
  SURFACE: "#14171C",
  SURFACE_RAISED: "#1C2026",
  BORDER: "#262B33",
  BORDER_VIS: "#4DA3FF",
  TEXT_DIM: "#6B7280",
  TEXT_SEC: "#A0A8B4",
  TEXT_PRI: "#E5E9F0",
  TEXT_DISP: "#FFFFFF",
  ACCENT: "#4DA3FF",
  ACCENT_RED: "#FF5C5C",
  UTIL_ORANGE: "#F2A65A",
  SUCCESS: "#5CD68F",
  WARNING: "#F2C94C",
  INFO: "#4DA3FF",
};

const LIGHT: Palette = {
  BG: "#F7F8FA",
  SURFACE: "#FFFFFF",
  SURFACE_RAISED: "#EEF1F5",
  BORDER: "#3A4250",
  BORDER_VIS: "#1F6FEB",
  TEXT_DIM: "#6B7280",
  TEXT_SEC: "#374151",
  TEXT_PRI: "#0F1623",
  TEXT_DISP: "#000000",
  ACCENT: "#1F6FEB",
  ACCENT_RED: "#D7263D",
  UTIL_ORANGE: "#E07A1F",
  SUCCESS: "#1B7A45",
  WARNING: "#9A6B00",
  INFO: "#1F6FEB",
};

type ThemeMode = "dark" | "light";
type StatusTone = "neutral" | "success" | "warning";
type ViewMode = "normal" | "detail" | "metrics" | "help";

const RISK_HELP: Record<string, string> = {
  heading_mismatch: "Query terms align with body text but miss heading intent.",
  semantic_confusion:
    "Dense retrieval is high while lexical/structure evidence is weak.",
  context_fragmentation:
    "Partial chunk match likely missing surrounding context.",
  lexical_overfit: "Strong exact term match with weak semantic alignment.",
  authority_bias: "Ranking gain is dominated by host authority boost.",
};

const apiBase = process.env.RETRIEVAL_XRAY_API_URL ?? "http://127.0.0.1:3000";
const qrelsPath =
  process.env.RETRIEVAL_XRAY_QRELS ?? "benchmarks/niche_db/qrels_100.tsv";
const queriesPath =
  process.env.RETRIEVAL_XRAY_QUERIES ?? "benchmarks/niche_db/queries_100.tsv";
const kValues = process.env.RETRIEVAL_XRAY_K ?? "1,3,5,10";
const initialQuery =
  process.argv.slice(2).join(" ") ||
  process.env.RETRIEVAL_XRAY_INITIAL_QUERY ||
  "";

let themeMode: ThemeMode = "dark";
let palette: Palette = DARK;

const queryHistory: string[] = [];
let historyIndex = -1; // -1 = current draft, not navigating history
let historyDraft = "";

let currentSearch: DebugSearchResponse | null = null;
let currentEval: DebugEvalResponse | null = null;
let searchAbort: AbortController | null = null;
let evalAbort: AbortController | null = null;
let searchSeq = 0;
let evalSeq = 0;
let focusedIndex = 0;
let viewMode: ViewMode = "normal";
let lastStatus = "Ready. Type a query and press Enter.";
let lastTone: StatusTone = "neutral";

const renderer = await createCliRenderer({
  exitOnCtrlC: true,
  targetFps: 20,
  backgroundColor: palette.BG,
});

const app = new BoxRenderable(renderer, {
  id: "app",
  width: "100%",
  height: "100%",
  flexDirection: "column",
  padding: 1,
  gap: 1,
  backgroundColor: palette.BG,
});

renderer.root.add(app);

function makeCard(id: string, title: string, flexGrow = 1): BoxRenderable {
  return new BoxRenderable(renderer, {
    id,
    title,
    border: true,
    borderStyle: "rounded",
    borderColor: palette.BORDER,
    focusedBorderColor: palette.BORDER_VIS,
    backgroundColor: palette.SURFACE,
    paddingTop: 1,
    paddingBottom: 1,
    paddingLeft: 2,
    paddingRight: 2,
    flexGrow,
    flexShrink: 1,
  });
}

const headerCard = makeCard("header", " Retrieval X-Ray ", 0);
headerCard.height = 4;
headerCard.flexShrink = 0;

const headerText = new TextRenderable(renderer, {
  id: "header-text",
  content:
    "Hybrid neural + lexical retrieval debugger  ·  Press ? for help  ·  Enter to search  ·  Ctrl+E to eval",
  fg: palette.TEXT_SEC,
  wrapMode: "word",
  attributes: BOLD,
});
headerCard.add(headerText);

const queryCard = makeCard("query", " Query ", 0);
queryCard.height = 5;
queryCard.flexShrink = 0;

const queryInput = new InputRenderable(renderer, {
  id: "query-input",
  value: initialQuery,
  placeholder:
    "Type your query and press Enter   (e.g. wal_level postgresql configuration parameter)",
  backgroundColor: palette.SURFACE_RAISED,
  focusedBackgroundColor: palette.SURFACE_RAISED,
  textColor: palette.TEXT_PRI,
  focusedTextColor: palette.TEXT_DISP,
  placeholderColor: palette.TEXT_DIM,
  cursorColor: palette.TEXT_DISP,
  flexShrink: 0,
});

const queryHint = new TextRenderable(renderer, {
  id: "query-hint",
  content:
    "↵ Search   ·   Ctrl+E Run eval   ·   Ctrl+R Re-run   ·   Ctrl+T Toggle theme",
  fg: palette.TEXT_DIM,
  wrapMode: "word",
  flexGrow: 1,
  attributes: BOLD,
});

queryCard.add(queryInput);
queryCard.add(queryHint);

const mainRow = new BoxRenderable(renderer, {
  id: "main-row",
  flexDirection: "row",
  gap: 1,
  flexGrow: 1,
  backgroundColor: palette.BG,
});

const resultsCard = makeCard("results", " Ranking ", 0);
resultsCard.width = Math.max(
  34,
  Math.floor((process.stdout.columns || 120) * 0.32),
);
resultsCard.flexShrink = 0;

const detailsCard = makeCard("details", " Chunk Detail ", 1);

const resultsList = new SelectRenderable(renderer, {
  id: "results-list",
  options: [],
  flexGrow: 1,
  backgroundColor: palette.SURFACE,
  focusedBackgroundColor: palette.SURFACE,
  textColor: palette.TEXT_PRI,
  focusedTextColor: palette.TEXT_PRI,
  selectedBackgroundColor: palette.SURFACE_RAISED,
  selectedTextColor: palette.TEXT_DISP,
  descriptionColor: palette.TEXT_DIM,
  selectedDescriptionColor: palette.TEXT_SEC,
  showDescription: true,
  showScrollIndicator: true,
  wrapSelection: true,
});

resultsCard.add(resultsList);

const detailsScroll = new ScrollBoxRenderable(renderer, {
  id: "details-scroll",
  flexGrow: 1,
  scrollY: true,
  scrollX: false,
  contentOptions: {
    flexDirection: "column",
    padding: 0,
    backgroundColor: palette.SURFACE,
  },
});

const detailsText = new TextRenderable(renderer, {
  id: "details-text",
  content:
    "  Run a search, then select a result to inspect chunk-level score signals.",
  fg: palette.TEXT_PRI,
  wrapMode: "word",
  attributes: BOLD,
});

detailsScroll.add(detailsText);
detailsCard.add(detailsScroll);

mainRow.add(resultsCard);
mainRow.add(detailsCard);

const bottomRow = new BoxRenderable(renderer, {
  id: "bottom-row",
  flexDirection: "row",
  gap: 1,
  height: 14,
  flexShrink: 0,
  backgroundColor: palette.BG,
});

const summaryCard = makeCard("summary", " Search Summary ", 1);
const evalCard = makeCard("eval", " Eval Metrics ", 1);
const riskCard = makeCard("risk", " Risk Signals ", 1);

const summaryText = new TextRenderable(renderer, {
  id: "summary-text",
  content: "No search yet.\n\nType a query above and press Enter\nto populate this panel.",
  fg: palette.TEXT_PRI,
  wrapMode: "word",
  attributes: BOLD,
});
summaryCard.add(summaryText);

const evalText = new TextRenderable(renderer, {
  id: "eval-text",
  content: "LIVE QUERY\n  Run a search to populate live metrics.\n\nBATCH EVAL  (Ctrl+E)\n  Not run yet — press Ctrl+E",
  fg: palette.TEXT_PRI,
  wrapMode: "word",
  height: 7,
  flexShrink: 0,
  attributes: BOLD,
});

const evalBars = new FrameBufferRenderable(renderer, {
  id: "eval-bars",
  width: 36,
  height: 4,
  flexShrink: 0,
});

evalCard.add(evalText);
evalCard.add(evalBars);

const riskText = new TextRenderable(renderer, {
  id: "risk-text",
  content: "No search yet.\n\nRun a query to surface risk signals.",
  fg: palette.TEXT_PRI,
  wrapMode: "word",
  height: 7,
  flexShrink: 0,
  attributes: BOLD,
});

const riskBars = new FrameBufferRenderable(renderer, {
  id: "risk-bars",
  width: 28,
  height: 3,
  flexShrink: 0,
});

const helpCard = makeCard("help", " Help & Glossary ", 0);
helpCard.height = 14;
helpCard.flexGrow = 1;
helpCard.flexShrink = 0;
helpCard.visible = false;

const helpText = new TextRenderable(renderer, {
  id: "help-text",
  fg: palette.TEXT_PRI,
  wrapMode: "word",
  attributes: BOLD,
  content: (() => {
    const div = "─".repeat(56);
    return [
      "  KEYBOARD SHORTCUTS",
      `  ${div}`,
      "    Enter         Run search on the current query",
      "    Ctrl+R        Re-run the last query",
      "    Ctrl+E        Run batch evaluation (qrels + queries)",
      "    Ctrl+T        Toggle dark / light theme",
      "    Ctrl+P / N    Previous / next query in history",
      "    ↑ / ↓         Move selection in the ranking list",
      "    Tab / S-Tab   Cycle focus between panels",
      "    N / D / M     Switch view: Normal · Detail · Metrics",
      "    ?             Toggle this help panel",
      "    /             Focus query input from anywhere",
      "    C             Copy selected result URL to clipboard",
      "    Esc / Q       Quit",
      "",
      "  LIVE QUERY METRICS",
      `  ${div}`,
      "    top           Top-1 final score",
      "    gap           Score gap between top-1 and top-2 (confidence)",
      "    vec / lex     Average vector & lexical evidence",
      "    share         Fraction of hits dominated by dense vs lex",
      "    phrase        Count of exact-phrase boosted hits",
      "    authority     Count of hits with authority bonus applied",
      "",
      "  BATCH EVAL METRICS  (Ctrl+E)",
      `  ${div}`,
      "    MRR                  Mean reciprocal rank",
      "    NDCG@1/3/5/10        Graded ranking quality at cutoffs",
      "    Recall@1/3/5/10      Fraction of relevant docs retrieved",
      "",
      "  RISK LABELS",
      `  ${div}`,
      "    heading_mismatch        Heading intent mismatch",
      "    semantic_confusion      Dense > lexical mismatch",
      "    context_fragmentation   Missing surrounding context",
      "    lexical_overfit         Exact-term over-reliance",
      "    authority_bias          Authority bonus dominates ranking",
    ].join("\n");
  })(),
});
helpCard.add(helpText);

riskCard.add(riskText);
riskCard.add(riskBars);

bottomRow.add(summaryCard);
bottomRow.add(evalCard);
bottomRow.add(riskCard);

const statusBar = new BoxRenderable(renderer, {
  id: "status",
  height: 4,
  flexShrink: 0,
  border: true,
  borderStyle: "rounded",
  borderColor: palette.BORDER,
  backgroundColor: palette.SURFACE_RAISED,
  paddingLeft: 2,
  paddingRight: 2,
  flexDirection: "column",
});

const keysText = new TextRenderable(renderer, {
  id: "keys-text",
  content: "",
  fg: palette.TEXT_DIM,
  wrapMode: "word",
  attributes: BOLD,
});

const statusText = new TextRenderable(renderer, {
  id: "status-text",
  content: "",
  fg: palette.TEXT_SEC,
  wrapMode: "word",
  attributes: BOLD,
});
statusBar.add(keysText);
statusBar.add(statusText);

app.add(headerCard);
app.add(queryCard);
app.add(mainRow);
app.add(bottomRow);
app.add(helpCard);
app.add(statusBar);

const focusTargets = [
  { box: queryCard, focus: queryInput },
  { box: resultsCard, focus: resultsList },
  { box: detailsCard, focus: detailsScroll },
];

const allCards = [
  headerCard,
  queryCard,
  resultsCard,
  detailsCard,
  summaryCard,
  evalCard,
  riskCard,
  helpCard,
  statusBar,
];

const allPlainText = [
  headerText,
  queryHint,
  detailsText,
  summaryText,
  evalText,
  riskText,
];

function rgba(hex: string): RGBA {
  return RGBA.fromHex(hex);
}

function clamp(value: number, min: number, max: number): number {
  return Math.max(min, Math.min(max, value));
}

function truncate(text: string, max: number): string {
  if (text.length <= max) return text;
  return `${text.slice(0, Math.max(0, max - 1))}…`;
}

function score(value: number): string {
  return Number.isFinite(value) ? value.toFixed(3) : "0.000";
}

function metric(points: MetricPoint[], k: number): number {
  return (
    points.find((point) => point.k === k)?.value ?? points.at(-1)?.value ?? 0
  );
}

function selectedHit(): DebugHit | null {
  return (resultsList.getSelectedOption()?.value ?? null) as DebugHit | null;
}

function setStatus(message: string, tone: StatusTone = "neutral"): void {
  lastStatus = message;
  lastTone = tone;
  renderStatus();
}

function renderStatus(): void {
  const themeLabel = themeMode === "dark" ? "Dark" : "Light";
  const viewLabel =
    viewMode === "normal"
      ? "Normal"
      : viewMode === "detail"
        ? "Detail"
        : viewMode === "metrics"
          ? "Metrics"
          : "Help";

  keysText.content =
    `${themeLabel} · ${viewLabel}   │   ` +
    "↵ search   / focus   Ctrl+P/N history   Ctrl+E eval   Ctrl+R rerun   ↑↓ select   ? help   Esc quit";

  const icon =
    lastTone === "success" ? "✓" : lastTone === "warning" ? "!" : "›";
  statusText.content = `${icon}  ${lastStatus}`;
  statusText.fg =
    lastTone === "success"
      ? palette.SUCCESS
      : lastTone === "warning"
        ? palette.WARNING
        : palette.TEXT_PRI;
}

function focus(index: number): void {
  focusedIndex = (index + focusTargets.length) % focusTargets.length;

  for (const card of allCards) {
    card.borderColor = palette.BORDER;
  }

  const target = focusTargets[focusedIndex];
  target.box.borderColor = palette.BORDER_VIS;
  target.focus.focus();
}

function focusNext(direction: 1 | -1): void {
  focus(focusedIndex + direction);
}

function drawEmptyCanvas(canvas: FrameBufferRenderable): void {
  const fb = canvas.frameBuffer;
  const bg = rgba(palette.SURFACE);
  const dim = rgba(palette.TEXT_DIM);

  for (let y = 0; y < canvas.height; y++) {
    for (let x = 0; x < canvas.width; x++) {
      fb.setCell(x, y, " ", dim, bg);
    }
  }
}

function drawBars(
  canvas: FrameBufferRenderable,
  rows: Array<[string, number]>,
  colorHex: string,
): void {
  const fb = canvas.frameBuffer;
  const bg = rgba(palette.SURFACE);
  const dim = rgba(palette.TEXT_DIM);
  const sec = rgba(palette.TEXT_SEC);
  const fill = rgba(colorHex);

  drawEmptyCanvas(canvas);

  rows.slice(0, canvas.height).forEach(([label, value], y) => {
    const labelWidth = 9;
    const valueWidth = 5;
    const barX = labelWidth;
    const barWidth = Math.max(4, canvas.width - barX - valueWidth - 1);
    const safeValue = clamp(value, 0, 1);
    const filled = Math.round(barWidth * safeValue);

    fb.drawText(truncate(label.padEnd(labelWidth - 1, " "), labelWidth - 1), 0, y, sec, bg);

    for (let x = 0; x < barWidth; x++) {
      fb.setCell(
        barX + x,
        y,
        x < filled ? "█" : "·",
        x < filled ? fill : dim,
        bg,
      );
    }

    fb.drawText(safeValue.toFixed(2).padStart(valueWidth, " "), barX + barWidth + 1, y, sec, bg);
  });
}

function updateCanvases(): void {
  const live = currentSearch;
  drawBars(
    evalBars,
    [
      ["hits", live ? clamp(live.result_count / 10, 0, 1) : 0],
      ["gap", live ? clamp(live.summary.score_gap_top1_top2, 0, 1) : 0],
      ["vec", live ? clamp(live.summary.avg_vector_score, 0, 1) : 0],
      ["lex", live ? clamp(live.summary.avg_lexical_score, 0, 1) : 0],
    ],
    palette.SUCCESS,
  );

  const riskCount =
    currentSearch?.summary.risk_counts.reduce(
      (sum, risk) => sum + risk.count,
      0,
    ) ?? 0;
  const riskLevel = currentSearch
    ? clamp(riskCount / Math.max(1, currentSearch.result_count), 0, 1)
    : 0;

  drawBars(
    riskBars,
    [
      ["risk", riskLevel],
      ["gap", currentSearch?.summary.score_gap_top1_top2 ?? 0],
      ["miss", selectedHit()?.missing_terms.length ? 1 : 0],
    ],
    palette.ACCENT_RED,
  );
}

function buildOption(hit: DebugHit): SelectOption {
  const heading = hit.heading_chain.at(-1) || hit.host || hit.display_url;
  const rank = String(hit.rank).padStart(2, " ");
  const finalScore = score(hit.score_breakdown.final_score);
  const vec = score(hit.score_breakdown.vector_score);
  const lex = score(hit.score_breakdown.lexical_score);
  const flags: string[] = [];
  if (hit.score_breakdown.exact_heading_phrase || hit.score_breakdown.exact_body_phrase)
    flags.push("◆");
  if (hit.score_breakdown.authority_bonus > 0) flags.push("★");
  if (hit.diagnostics.length > 0) flags.push("⚠");
  const flagStr = flags.length ? ` ${flags.join("")}` : "";

  return {
    name: `${rank}.  ${finalScore}  ${truncate(heading, 38)}${flagStr}`,
    description: `      vec ${vec}   lex ${lex}   ${truncate(hit.display_url, 44)}`,
    value: hit,
  };
}

function formatSummary(search: DebugSearchResponse | null): string {
  if (!search) {
    return [
      "No search yet.",
      "",
      "Type a query above and press Enter",
      "to populate this panel.",
    ].join("\n");
  }

  const s = search.summary;
  const pad = (k: string) => k.padEnd(11, " ");

  return [
    `${pad("Query")} ${truncate(search.query, 40)}`,
    `${pad("Latency")} ${search.elapsed_ms} ms`,
    `${pad("Hits")} ${search.result_count}`,
    `${pad("Terms")} ${s.query_terms.join(", ") || "—"}`,
    "",
    `${pad("Top gap")} ${score(s.score_gap_top1_top2)}`,
    `${pad("Avg vec")} ${score(s.avg_vector_score)}`,
    `${pad("Avg lex")} ${score(s.avg_lexical_score)}`,
    `${pad("Dense dom")} ${s.dense_dominant_hits}`,
    `${pad("Lex dom")} ${s.lexical_dominant_hits}`,
    `${pad("Phrase")} ${s.exact_phrase_hits}`,
  ].join("\n");
}

function formatDetail(hit: DebugHit | null): string {
  if (!hit) {
    return [
      "Select a result on the left to inspect its score breakdown.",
      "",
      "Use ↑ / ↓ to move through the ranking.",
    ].join("\n");
  }

  const b = hit.score_breakdown;
  const div = "─".repeat(48);

  const diagnostics =
    hit.diagnostics.length === 0
      ? ["  (none)"]
      : hit.diagnostics.map((item) => `  • ${item.label} — ${item.reason}`);

  const scoreRow = (label: string, raw: number, contrib?: number): string => {
    const rawStr = score(raw).padStart(6, " ");
    const contribStr =
      contrib === undefined ? "" : `   →  ${score(contrib).padStart(6, " ")}`;
    return `  ${label.padEnd(11, " ")} ${rawStr}${contribStr}`;
  };

  return [
    `  Rank  #${hit.rank}     Final score  ${score(b.final_score)}`,
    `  Chunk    ${hit.chunk_id}`,
    `  Host     ${hit.host || "—"}`,
    `  URL      ${hit.source_url}`,
    `  Heading  ${hit.heading_chain.join(" › ") || "—"}`,
    "",
    `  TERM COVERAGE`,
    `  ${div}`,
    `    Matched   ${hit.matched_terms.join(", ") || "—"}`,
    `    Missing   ${hit.missing_terms.join(", ") || "—"}`,
    "",
    `  SCORE BREAKDOWN              raw         contribution`,
    `  ${div}`,
    scoreRow("vector", b.vector_score, b.vector_contribution),
    scoreRow("lexical", b.lexical_score, b.lexical_contribution),
    scoreRow("title", b.title_overlap, b.title_contribution),
    scoreRow("heading", b.heading_overlap, b.heading_contribution),
    scoreRow("body", b.body_overlap, b.body_contribution),
    "",
    scoreRow("phrase+", b.phrase_bonus),
    scoreRow("authority", b.authority_bonus),
    scoreRow("penalty×", b.penalty_multiplier),
    "",
    `  DIAGNOSTICS`,
    `  ${div}`,
    ...diagnostics,
    "",
    `  PREVIEW`,
    `  ${div}`,
    `  ${(hit.preview || hit.text).split("\n").join("\n  ")}`,
  ].join("\n");
}

function formatEval(report: DebugEvalResponse | null): string {
  const liveSection = (() => {
    if (!currentSearch) {
      return [
        "LIVE QUERY",
        "  Run a search to populate live metrics.",
      ].join("\n");
    }

    const s = currentSearch.summary;
    const top = currentSearch.results[0]?.score_breakdown.final_score ?? 0;
    const lexicalShare =
      s.lexical_dominant_hits / Math.max(1, currentSearch.result_count);
    const denseShare =
      s.dense_dominant_hits / Math.max(1, currentSearch.result_count);

    return [
      "LIVE QUERY",
      `  Query    ${truncate(currentSearch.query, 52)}`,
      `  Hits     ${String(currentSearch.result_count).padEnd(4)}  Latency  ${currentSearch.elapsed_ms} ms`,
      `  Top      ${score(top).padEnd(6)}  Gap      ${score(s.score_gap_top1_top2)}`,
      `  Avg      vec ${score(s.avg_vector_score)}   lex ${score(s.avg_lexical_score)}`,
      `  Share    dense ${denseShare.toFixed(2)}    lex ${lexicalShare.toFixed(2)}`,
      `  Phrase   ${String(s.exact_phrase_hits).padEnd(4)}  Authority ${s.authority_boosted_hits}`,
    ].join("\n");
  })();

  const fmt = (value: number | null): string =>
    value === null ? "  —  " : value.toFixed(3);

  const batchSection = (() => {
    const mrr = report ? report.mrr.toFixed(4) : "  —   ";
    const ndcg1 = report ? fmt(metric(report.ndcg_at, 1)) : "  —  ";
    const ndcg3 = report ? fmt(metric(report.ndcg_at, 3)) : "  —  ";
    const ndcg5 = report ? fmt(metric(report.ndcg_at, 5)) : "  —  ";
    const ndcg10 = report ? fmt(metric(report.ndcg_at, 10)) : "  —  ";
    const rec1 = report ? fmt(metric(report.recall_at, 1)) : "  —  ";
    const rec3 = report ? fmt(metric(report.recall_at, 3)) : "  —  ";
    const rec5 = report ? fmt(metric(report.recall_at, 5)) : "  —  ";
    const rec10 = report ? fmt(metric(report.recall_at, 10)) : "  —  ";

    const header = report
      ? `  ${report.num_queries} queries in ${report.elapsed_ms} ms`
      : "  Not run yet — press Ctrl+E";

    return [
      "BATCH EVAL  (Ctrl+E)",
      header,
      `  MRR       ${mrr}`,
      `  NDCG      @1 ${ndcg1}   @3 ${ndcg3}   @5 ${ndcg5}   @10 ${ndcg10}`,
      `  Recall    @1 ${rec1}   @3 ${rec3}   @5 ${rec5}   @10 ${rec10}`,
    ].join("\n");
  })();

  return [liveSection, "", batchSection].join("\n");
}

function formatRisk(search: DebugSearchResponse | null): string {
  if (!search) {
    return ["No search yet.", "", "Run a query to surface risk signals."].join(
      "\n",
    );
  }

  const risks = search.summary.risk_counts;
  if (risks.length === 0) return "✓  No risk heuristics triggered.";

  return risks
    .slice(0, 6)
    .map((risk) => {
      const detail =
        RISK_HELP[risk.code] ||
        RISK_HELP[risk.label.toLowerCase().replaceAll(" ", "_")] ||
        "See ? help for explanation.";
      return `⚠  ${risk.label}  (${risk.count})\n     ${detail}`;
    })
    .join("\n\n");
}

function applySearch(search: DebugSearchResponse): void {
  currentSearch = search;
  resultsCard.title = ` Ranking · ${search.result_count} hits `;

  if (search.results.length > 0) {
    resultsList.options = search.results.map(buildOption);
    resultsList.setSelectedIndex(0);
    detailsText.content = formatDetail(selectedHit());
  } else {
    resultsList.options = [
      {
        name: "  No hits",
        description: "  Try a benchmark query (see Chunk Detail panel)",
        value: null,
      },
    ];
    detailsText.content = [
      "  No chunks matched this query.",
      "",
      "  The backend responded successfully but the current",
      "  corpus / index did not return any candidates.",
      "",
      "  Try a benchmark-style query:",
      "    • wal_level postgresql configuration parameter",
      "    • full_page_writes postgres setting",
      "    • shared_buffers configuration postgres",
      "    • work_mem postgres parameter",
    ].join("\n");
  }

  summaryText.content = formatSummary(search);
  riskText.content = formatRisk(search);
  detailsScroll.scrollTop = 0;
  updateCanvases();

  setStatus(
    `Search complete: ${search.result_count} hits in ${search.elapsed_ms} ms.`,
    search.result_count > 0 ? "success" : "warning",
  );
}

function applyEval(report: DebugEvalResponse | null): void {
  currentEval = report;
  evalText.content = formatEval(report);
  updateCanvases();

  if (report) {
    setStatus(
      `Batch eval complete: ${report.num_queries} queries in ${report.elapsed_ms} ms.`,
      "success",
    );
  }
}

async function fetchJson<T>(
  path: string,
  params: Record<string, string>,
  signal?: AbortSignal,
): Promise<T> {
  const url = new URL(path, apiBase);

  for (const [key, value] of Object.entries(params)) {
    if (value.trim()) url.searchParams.set(key, value);
  }

  const timeout = AbortSignal.timeout(120_000);
  const combined = signal ? AbortSignal.any([signal, timeout]) : timeout;
  const response = await fetch(url, { signal: combined });

  if (!response.ok) {
    const body = await response.text();
    throw new Error(`${response.status} ${response.statusText}: ${body}`);
  }

  return (await response.json()) as T;
}

async function runSearch(query: string): Promise<void> {
  const trimmed = query.trim();
  if (!trimmed) {
    setStatus("Type a query first.", "warning");
    return;
  }

  if (queryHistory.length === 0 || queryHistory[queryHistory.length - 1] !== trimmed) {
    queryHistory.push(trimmed);
    if (queryHistory.length > 50) queryHistory.shift();
  }
  historyIndex = -1;

  searchAbort?.abort();
  searchAbort = new AbortController();
  const signal = searchAbort.signal;
  const seq = ++searchSeq;

  setStatus(`Searching for "${truncate(trimmed, 60)}"…`);
  summaryText.content = "Searching…";
  detailsText.content = "  Waiting for /debug/api/search …";
  resultsList.options = [];
  resultsCard.title = " Ranking ";

  try {
    const response = await fetchJson<DebugSearchResponse>(
      "/debug/api/search",
      { q: trimmed, k: "10" },
      signal,
    );

    if (seq !== searchSeq) return;
    queryInput.value = trimmed;
    applySearch(response);
    evalText.content = formatEval(currentEval);
    focus(0);
  } catch (error) {
    if (signal.aborted || seq !== searchSeq) return;

    const message = error instanceof Error ? error.message : String(error);
    currentSearch = null;
    summaryText.content = "Search failed.";
    riskText.content = "Search failed.";
    detailsText.content = [
      "  Search failed.",
      "",
      `  ${message}`,
      "",
      "  Make sure the Rust server is running:",
      "    cargo run --release --bin search_engine",
    ].join("\n");
    updateCanvases();
    setStatus("Search failed. Is the server running?", "warning");
  }
}

async function runEval(): Promise<void> {
  evalAbort?.abort();
  evalAbort = new AbortController();
  const signal = evalAbort.signal;
  const seq = ++evalSeq;

  setStatus("Running batch eval…");
  evalText.content = [
    "BATCH EVAL  (running…)",
    `  qrels    ${qrelsPath}`,
    `  queries  ${queriesPath}`,
    `  k        ${kValues}`,
  ].join("\n");

  try {
    const report = await fetchJson<DebugEvalResponse>(
      "/debug/api/eval",
      { qrels: qrelsPath, queries: queriesPath, k: kValues },
      signal,
    );

    if (seq !== evalSeq) return;
    applyEval(report);
  } catch (error) {
    if (signal.aborted || seq !== evalSeq) return;

    const message = error instanceof Error ? error.message : String(error);
    applyEval(null);
    evalText.content = [
      "BATCH EVAL  (failed)",
      `  ${message}`,
      "",
      "  Make sure the server is running and that the",
      "  qrels / query paths exist relative to it.",
    ].join("\n");
    setStatus("Eval failed.", "warning");
  }
}

function resizeCanvases(): void {
  const columns = process.stdout.columns || 120;
  const cardWidth = Math.max(24, Math.floor((columns - 8) / 3));

  if (viewMode === "detail") {
    resultsCard.width = Math.max(34, Math.min(44, Math.floor(columns * 0.26)));
  } else {
    resultsCard.width = Math.max(34, Math.min(52, Math.floor(columns * 0.32)));
  }

  evalBars.width = Math.max(22, cardWidth - 8);
  evalBars.height = 4;

  riskBars.width = Math.max(20, cardWidth - 8);
  riskBars.height = 3;

  updateCanvases();
}

function applyTheme(): void {
  palette = themeMode === "dark" ? DARK : LIGHT;

  app.backgroundColor = palette.BG;
  mainRow.backgroundColor = palette.BG;
  bottomRow.backgroundColor = palette.BG;

  for (const card of allCards) {
    card.backgroundColor =
      card === statusBar ? palette.SURFACE_RAISED : palette.SURFACE;
    card.borderColor = palette.BORDER;
    card.focusedBorderColor = palette.BORDER_VIS;
  }

  for (const text of allPlainText) {
    text.fg = palette.TEXT_PRI;
  }

  headerText.fg = palette.TEXT_SEC;
  queryHint.fg = palette.TEXT_DIM;
  helpText.fg = palette.TEXT_PRI;
  keysText.fg = palette.TEXT_DIM;

  queryInput.backgroundColor = palette.SURFACE_RAISED;
  queryInput.focusedBackgroundColor = palette.SURFACE_RAISED;
  queryInput.textColor = palette.TEXT_PRI;
  queryInput.focusedTextColor = palette.TEXT_DISP;
  queryInput.placeholderColor = palette.TEXT_DIM;
  queryInput.cursorColor = palette.TEXT_DISP;

  resultsList.backgroundColor = palette.SURFACE;
  resultsList.focusedBackgroundColor = palette.SURFACE;
  resultsList.textColor = palette.TEXT_PRI;
  resultsList.focusedTextColor = palette.TEXT_PRI;
  resultsList.selectedBackgroundColor = palette.SURFACE_RAISED;
  resultsList.selectedTextColor = palette.TEXT_DISP;
  resultsList.descriptionColor = palette.TEXT_DIM;
  resultsList.selectedDescriptionColor = palette.TEXT_SEC;

  detailsScroll.backgroundColor = palette.SURFACE;
  detailsScroll.contentOptions = {
    flexDirection: "column",
    padding: 0,
    backgroundColor: palette.SURFACE,
  };

  renderStatus();
  applyViewMode(viewMode);
  focus(focusedIndex);
  resizeCanvases();
}

function toggleTheme(): void {
  themeMode = themeMode === "dark" ? "light" : "dark";
  applyTheme();
  setStatus(`Theme changed to ${themeMode}.`, "success");
}

function applyViewMode(mode: ViewMode): void {
  viewMode = mode;

  headerCard.visible = true;
  queryCard.visible = true;
  statusBar.visible = true;

  resultsCard.visible = true;
  detailsCard.visible = true;
  summaryCard.visible = true;
  evalCard.visible = true;
  riskCard.visible = true;
  helpCard.visible = false;

  mainRow.visible = true;
  bottomRow.visible = true;

  mainRow.height = "auto";
  mainRow.flexGrow = 1;
  bottomRow.height = 14;
  bottomRow.flexShrink = 0;
  detailsCard.flexGrow = 1;
  evalCard.flexGrow = 1;
  riskCard.flexGrow = 1;

  if (mode === "detail") {
    helpCard.visible = false;
    mainRow.visible = true;
    bottomRow.visible = false;
    detailsCard.flexGrow = 3;
    resultsCard.width = Math.max(
      34,
      Math.min(44, Math.floor((process.stdout.columns || 120) * 0.26)),
    );
  } else if (mode === "metrics") {
    helpCard.visible = false;
    mainRow.visible = false;
    bottomRow.visible = true;
    bottomRow.height = Math.max(18, (process.stdout.rows || 40) - 14);
    summaryCard.flexGrow = 1;
    evalCard.flexGrow = 2;
    riskCard.flexGrow = 2;
  } else if (mode === "help") {
    mainRow.visible = false;
    bottomRow.visible = false;
    helpCard.visible = true;
    helpCard.height = Math.max(14, (process.stdout.rows || 40) - 12);
  }

  renderStatus();
  resizeCanvases();
}

resultsList.on(
  SelectRenderableEvents.SELECTION_CHANGED,
  (_index: number, _option: SelectOption) => {
    detailsText.content = formatDetail(selectedHit());
    detailsScroll.scrollTop = 0;
    updateCanvases();
  },
);

queryInput.on(InputRenderableEvents.ENTER, (value: string) => {
  void runSearch(value);
});

renderer.keyInput.on("keypress", (key: KeyEvent) => {
  if (key.ctrl && key.name === "t") {
    toggleTheme();
    return;
  }

  if (key.ctrl && key.name === "e") {
    void runEval();
    return;
  }

  if (key.ctrl && key.name === "r") {
    void runSearch(queryInput.value);
    return;
  }

  if (key.name === "up") {
    resultsList.moveUp();
    detailsText.content = formatDetail(selectedHit());
    detailsScroll.scrollTop = 0;
    updateCanvases();
    return;
  }

  if (key.name === "down") {
    resultsList.moveDown();
    detailsText.content = formatDetail(selectedHit());
    detailsScroll.scrollTop = 0;
    updateCanvases();
    return;
  }

  if (key.name === "?") {
    applyViewMode(viewMode === "help" ? "normal" : "help");
    setStatus(viewMode === "help" ? "Help opened." : "Normal view.", "neutral");
    return;
  }

  if (key.name === "n") {
    applyViewMode("normal");
    setStatus("Normal view.", "neutral");
    return;
  }

  if (key.name === "d") {
    applyViewMode("detail");
    setStatus("Detail focus view.", "neutral");
    return;
  }

  if (key.name === "m") {
    applyViewMode("metrics");
    setStatus("Metrics focus view.", "neutral");
    return;
  }

  if (key.name === "tab") {
    focusNext(key.shift ? -1 : 1);
    return;
  }

  if (key.name === "/" && !queryInput.focused) {
    focus(0);
    return;
  }

  if (queryInput.focused && key.ctrl && key.name === "p") {
    if (historyIndex < queryHistory.length - 1) {
      if (historyIndex === -1) {
        historyDraft = queryInput.value;
      }
      historyIndex++;
      queryInput.value = queryHistory[queryHistory.length - 1 - historyIndex];
    }
    return;
  }

  if (queryInput.focused && key.ctrl && key.name === "n") {
    if (historyIndex > 0) {
      historyIndex--;
      queryInput.value = queryHistory[queryHistory.length - 1 - historyIndex];
    } else if (historyIndex === 0) {
      historyIndex = -1;
      queryInput.value = historyDraft;
    }
    return;
  }

  if (key.name === "escape") {
    if (viewMode === "help") {
      applyViewMode("normal");
      setStatus("Help closed.", "neutral");
      return;
    }
    renderer.destroy();
    return;
  }

  if (queryInput.focused) {
    return;
  }

  if (key.name === "t") {
    toggleTheme();
    return;
  }

  if (key.name === "q") {
    renderer.destroy();
    return;
  }

  if (key.name === "c") {
    const hit = selectedHit();
    const url = hit?.source_url || hit?.display_url || "";
    if (!url) {
      setStatus("Nothing to copy: no selected result URL.", "warning");
      return;
    }

    let proc;
    const platform = process.platform;
    if (platform === "darwin") {
      proc = spawn("pbcopy");
    } else if (platform === "win32") {
      proc = spawn("clip");
    } else if (process.env.WAYLAND_DISPLAY) {
      proc = spawn("wl-copy");
    } else {
      proc = spawn("xclip", ["-selection", "clipboard"]);
    }

    proc.on("error", () => {
      setStatus("Copy failed: clipboard utility is unavailable.", "warning");
    });
    proc.stdin.write(url);
    proc.stdin.end();
    setStatus("Copied selected result URL to clipboard.", "success");
  }
});

process.stdout.on("resize", () => {
  applyViewMode(viewMode);
});
renderer.on("resize", () => {
  applyViewMode(viewMode);
});

resizeCanvases();
applyTheme();
applyEval(null);
applyViewMode("normal");
focus(0);
renderer.start();
setStatus(
  "Ready. Type a query and press Enter, or press ? for the help panel.",
);
