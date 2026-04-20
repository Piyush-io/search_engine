process.env.OPENTUI_FORCE_EXPLICIT_WIDTH ??= "false"

import {
  BoxRenderable,
  InputRenderable,
  InputRenderableEvents,
  ScrollBoxRenderable,
  SelectRenderable,
  SelectRenderableEvents,
  TextRenderable,
  createCliRenderer,
  type KeyEvent,
  type SelectOption,
} from "@opentui/core"

type DebugSearchResponse = {
  query: string
  elapsed_ms: number
  result_count: number
  summary: DebugSummary
  results: DebugHit[]
}

type DebugSummary = {
  query_terms: string[]
  short_query: boolean
  score_gap_top1_top2: number
  host_diversity: number
  avg_vector_score: number
  avg_lexical_score: number
  avg_heading_overlap: number
  avg_body_overlap: number
  dense_dominant_hits: number
  lexical_dominant_hits: number
  authority_boosted_hits: number
  exact_phrase_hits: number
  risk_counts: DebugRiskCount[]
}

type DebugRiskCount = {
  code: string
  label: string
  count: number
}

type DebugHit = {
  rank: number
  chunk_id: string
  source_url: string
  display_url: string
  host: string
  heading_chain: string[]
  text: string
  preview: string
  matched_terms: string[]
  missing_terms: string[]
  diagnostics: DebugDiagnostic[]
  score_breakdown: DebugScoreBreakdown
}

type DebugDiagnostic = {
  code: string
  label: string
  reason: string
}

type DebugScoreBreakdown = {
  query_mode: string
  vector_score: number
  lexical_score: number
  title_overlap: number
  heading_overlap: number
  body_overlap: number
  vector_contribution: number
  lexical_contribution: number
  title_contribution: number
  heading_contribution: number
  body_contribution: number
  base_total: number
  phrase_bonus: number
  penalty_multiplier: number
  pre_authority_score: number
  authority_bonus: number
  final_score: number
  dense_minus_lexical: number
  exact_heading_phrase: boolean
  exact_body_phrase: boolean
  reconstruction_gap: number
}

type DebugEvalResponse = {
  elapsed_ms: number
  qrels_path: string
  queries_path: string
  top_k: number
  num_queries: number
  mrr: number
  ndcg_at: MetricPoint[]
  recall_at: MetricPoint[]
  worst_queries: DebugQueryEvalRow[]
}

type MetricPoint = {
  k: number
  value: number
}

type DebugQueryEvalRow = {
  query_id: string
  query: string
  reciprocal_rank: number
  first_relevant_rank: number | null
  ndcg_at_top_k: number
  recall_at_top_k: number
  returned_relevant: string[]
  missed_relevant: string[]
}

const palette = {
  canvas: "#0B0B0B",
  panel: "#111111",
  panelAlt: "#161628",
  border: "#2B396D",
  borderFocus: "#4a5a9d",
  selection: "#2B396D",
  text: "#E4E4E4",
  muted: "#9090a0",
  accent: "#E4E4E4",
  danger: "#ff8a5b",
  success: "#7bd389",
}

const apiBase = process.env.RETRIEVAL_XRAY_API_URL ?? "http://127.0.0.1:3000"
const qrelsPath = process.env.RETRIEVAL_XRAY_QRELS ?? ""
const queriesPath = process.env.RETRIEVAL_XRAY_QUERIES ?? ""
const kValues = process.env.RETRIEVAL_XRAY_K ?? "1,3,5,10"
const configPath = process.env.SEARCH_ENGINE_CONFIG_PATH ?? "config.toml"
const initialQuery =
  process.argv.slice(2).join(" ") ||
  process.env.RETRIEVAL_XRAY_INITIAL_QUERY ||
  "rust lifetime elision rules"

const renderer = await createCliRenderer({
  exitOnCtrlC: true,
  targetFps: 30,
  backgroundColor: palette.canvas,
})

const app = new BoxRenderable(renderer, {
  id: "app",
  flexGrow: 1,
  flexDirection: "column",
  padding: 1,
  gap: 1,
  backgroundColor: palette.canvas,
})

renderer.root.add(app)

const headerBox = new BoxRenderable(renderer, {
  id: "header",
  height: 3,
  flexShrink: 0,
  flexDirection: "row",
  alignItems: "center",
  justifyContent: "space-between",
  border: true,
  borderStyle: "rounded",
  borderColor: palette.border,
  backgroundColor: palette.panel,
  paddingX: 1,
})
const headerTitle = new TextRenderable(renderer, {
  id: "header-title",
  content: "Retrieval X-Ray | neural retrieval debugger",
  fg: palette.text,
})
const headerStatus = new TextRenderable(renderer, {
  id: "header-status",
  content: "booting...",
  fg: palette.muted,
})
headerBox.add(headerTitle)
headerBox.add(headerStatus)

const queryBox = new BoxRenderable(renderer, {
  id: "query-box",
  height: 3,
  flexShrink: 0,
  border: true,
  borderStyle: "rounded",
  borderColor: palette.border,
  focusedBorderColor: palette.borderFocus,
  title: "Query | Enter to search",
  backgroundColor: palette.panel,
  paddingX: 1,
  alignItems: "center",
})
const queryInput = new InputRenderable(renderer, {
  id: "query-input",
  flexGrow: 1,
  placeholder: "Search retrieval behavior...",
  value: initialQuery,
  backgroundColor: palette.panel,
  focusedBackgroundColor: palette.panel,
  textColor: palette.text,
  focusedTextColor: palette.text,
  placeholderColor: palette.muted,
  cursorColor: palette.accent,
})
queryBox.add(queryInput)

const bodyRow = new BoxRenderable(renderer, {
  id: "body-row",
  flexGrow: 1,
  flexDirection: "row",
  gap: 1,
})

const resultsPane = new BoxRenderable(renderer, {
  id: "results-pane",
  width: 40,
  flexShrink: 0,
  border: true,
  borderStyle: "rounded",
  borderColor: palette.border,
  focusedBorderColor: palette.borderFocus,
  title: "Ranking",
  backgroundColor: palette.panel,
})
const resultsList = new SelectRenderable(renderer, {
  id: "results-list",
  flexGrow: 1,
  options: [],
  backgroundColor: palette.panel,
  focusedBackgroundColor: palette.panelAlt,
  textColor: palette.text,
  focusedTextColor: palette.text,
  selectedBackgroundColor: palette.selection,
  selectedTextColor: palette.text,
  descriptionColor: palette.muted,
  selectedDescriptionColor: palette.text,
  showDescription: true,
  showScrollIndicator: true,
  wrapSelection: true,
})
resultsPane.add(resultsList)

const detailPane = new BoxRenderable(renderer, {
  id: "detail-pane",
  flexGrow: 1,
  border: true,
  borderStyle: "rounded",
  borderColor: palette.border,
  focusedBorderColor: palette.borderFocus,
  title: "Chunk detail",
  backgroundColor: palette.panel,
})
const detailScroll = new ScrollBoxRenderable(renderer, {
  id: "detail-scroll",
  flexGrow: 1,
  scrollY: true,
  scrollX: false,
  contentOptions: {
    flexDirection: "column",
    padding: 1,
    backgroundColor: palette.panel,
  },
})
const detailText = new TextRenderable(renderer, {
  id: "detail-text",
  content: "Run a query to inspect how dense, lexical, and structural signals combine.",
  wrapMode: "word",
  fg: palette.text,
})
detailScroll.add(detailText)
detailPane.add(detailScroll)

const sideColumn = new BoxRenderable(renderer, {
  id: "side-column",
  width: 38,
  flexShrink: 0,
  flexDirection: "column",
  gap: 1,
})

const summaryPane = new BoxRenderable(renderer, {
  id: "summary-pane",
  height: 18,
  flexShrink: 0,
  border: true,
  borderStyle: "rounded",
  borderColor: palette.border,
  title: "Query summary",
  backgroundColor: palette.panel,
})
const summaryScroll = new ScrollBoxRenderable(renderer, {
  id: "summary-scroll",
  flexGrow: 1,
  scrollY: true,
  scrollX: false,
  contentOptions: {
    flexDirection: "column",
    padding: 1,
    backgroundColor: palette.panel,
  },
})
const summaryText = new TextRenderable(renderer, {
  id: "summary-text",
  content: "Waiting for the first retrieval run.",
  wrapMode: "word",
  fg: palette.text,
})
summaryScroll.add(summaryText)
summaryPane.add(summaryScroll)

const evalPane = new BoxRenderable(renderer, {
  id: "eval-pane",
  flexGrow: 1,
  border: true,
  borderStyle: "rounded",
  borderColor: palette.border,
  title: "Evaluation",
  backgroundColor: palette.panel,
})
const evalScroll = new ScrollBoxRenderable(renderer, {
  id: "eval-scroll",
  flexGrow: 1,
  scrollY: true,
  scrollX: false,
  contentOptions: {
    flexDirection: "column",
    padding: 1,
    backgroundColor: palette.panel,
  },
})
const evalText = new TextRenderable(renderer, {
  id: "eval-text",
  content: "Evaluation is optional. Set RETRIEVAL_XRAY_QRELS and RETRIEVAL_XRAY_QUERIES, then press Ctrl+E.",
  wrapMode: "word",
  fg: palette.text,
})
evalScroll.add(evalText)
evalPane.add(evalScroll)

sideColumn.add(summaryPane)
sideColumn.add(evalPane)

bodyRow.add(resultsPane)
bodyRow.add(detailPane)
bodyRow.add(sideColumn)

const footerBox = new BoxRenderable(renderer, {
  id: "footer",
  height: 3,
  flexShrink: 0,
  border: true,
  borderStyle: "rounded",
  borderColor: palette.border,
  backgroundColor: palette.panel,
  paddingX: 1,
  alignItems: "center",
})
const footerText = new TextRenderable(renderer, {
  id: "footer-text",
  content:
    "Tab focus | Enter search | arrows/jk navigate | Ctrl+L focus query | Ctrl+R rerun | Ctrl+E eval | Esc quit",
  wrapMode: "word",
  fg: palette.muted,
})
footerBox.add(footerText)

app.add(headerBox)
app.add(queryBox)
app.add(bodyRow)
app.add(footerBox)

const focusRing = [queryInput, resultsList, detailScroll, evalScroll]
let focusIndex = 0
let currentSearch: DebugSearchResponse | null = null
let currentEval: DebugEvalResponse | null = null
let searchSequence = 0
let evalSequence = 0
let searchAbort: AbortController | null = null
let evalAbort: AbortController | null = null

function setStatus(message: string): void {
  headerStatus.content = `${message} | ${configPath}`
}

function resizePanels(width: number): void {
  resultsPane.width = Math.max(34, Math.min(46, Math.floor(width * 0.29)))
  sideColumn.width = Math.max(34, Math.min(40, Math.floor(width * 0.27)))
}

function truncate(text: string, max: number): string {
  if (text.length <= max) {
    return text
  }
  return `${text.slice(0, Math.max(0, max - 3))}...`
}

function formatScore(value: number): string {
  return value.toFixed(3)
}

function formatListHeading(hit: DebugHit): string {
  const heading = hit.heading_chain.at(-1) || hit.host || hit.display_url
  return truncate(heading, 34)
}

function buildResultOptions(results: DebugHit[]): SelectOption[] {
  return results.map((hit) => ({
    name: `#${hit.rank} ${formatScore(hit.score_breakdown.final_score)} ${formatListHeading(hit)}`,
    description: `${truncate(hit.display_url, 28)} | v ${formatScore(hit.score_breakdown.vector_score)} | l ${formatScore(hit.score_breakdown.lexical_score)}`,
    value: hit,
  }))
}

function formatSummary(search: DebugSearchResponse | null, selected: DebugHit | null): string {
  if (!search) {
    return "Waiting for the first retrieval run."
  }

  const summary = search.summary
  const riskLines =
    summary.risk_counts.length === 0
      ? ["- no obvious risk heuristics triggered"]
      : summary.risk_counts.slice(0, 5).map((risk) => `- ${risk.label}: ${risk.count}`)

  const selectedLines = selected
    ? [
        "",
        "Selected hit",
        `- rank: #${selected.rank}`,
        `- host: ${selected.host || "n/a"}`,
        `- matched terms: ${selected.matched_terms.join(", ") || "none"}`,
        `- missing terms: ${selected.missing_terms.join(", ") || "none"}`,
        `- diagnostics: ${selected.diagnostics.map((item) => item.label).join(", ") || "none"}`,
      ]
    : []

  return [
    `Query: ${search.query || "(empty)"}`,
    `Mode: ${summary.short_query ? "short" : "long"} (${summary.query_terms.length} terms)` ,
    `Latency: ${search.elapsed_ms} ms`,
    `Hits: ${search.result_count}`,
    `Host diversity: ${summary.host_diversity}`,
    `Top score gap: ${formatScore(summary.score_gap_top1_top2)}`,
    `Avg vector / lexical: ${formatScore(summary.avg_vector_score)} / ${formatScore(summary.avg_lexical_score)}`,
    `Avg heading / body overlap: ${formatScore(summary.avg_heading_overlap)} / ${formatScore(summary.avg_body_overlap)}`,
    `Dense dominant hits: ${summary.dense_dominant_hits}`,
    `Lexical dominant hits: ${summary.lexical_dominant_hits}`,
    `Authority boosted hits: ${summary.authority_boosted_hits}`,
    `Exact phrase hits: ${summary.exact_phrase_hits}`,
    "",
    "Risk counts",
    ...riskLines,
    ...selectedLines,
  ].join("\n")
}

function formatDetail(hit: DebugHit | null): string {
  if (!hit) {
    return "Select a ranked chunk to inspect score decomposition, coverage, and failure signals."
  }

  const diagnostics =
    hit.diagnostics.length === 0
      ? ["- no diagnostic heuristics triggered"]
      : hit.diagnostics.map((item) => `- ${item.label}: ${item.reason}`)
  const heading = hit.heading_chain.length > 0 ? hit.heading_chain.join(" > ") : "(no heading chain)"

  return [
    `Rank #${hit.rank}`,
    `Chunk: ${hit.chunk_id}`,
    `Host: ${hit.host || "n/a"}`,
    `URL: ${hit.source_url}`,
    `Heading: ${heading}`,
    `Matched terms: ${hit.matched_terms.join(", ") || "none"}`,
    `Missing terms: ${hit.missing_terms.join(", ") || "none"}`,
    "",
    "Score decomposition",
    `- final score: ${formatScore(hit.score_breakdown.final_score)}`,
    `- vector: ${formatScore(hit.score_breakdown.vector_score)} x contribution ${formatScore(hit.score_breakdown.vector_contribution)}`,
    `- lexical: ${formatScore(hit.score_breakdown.lexical_score)} x contribution ${formatScore(hit.score_breakdown.lexical_contribution)}`,
    `- title overlap: ${formatScore(hit.score_breakdown.title_overlap)} x contribution ${formatScore(hit.score_breakdown.title_contribution)}`,
    `- heading overlap: ${formatScore(hit.score_breakdown.heading_overlap)} x contribution ${formatScore(hit.score_breakdown.heading_contribution)}`,
    `- body overlap: ${formatScore(hit.score_breakdown.body_overlap)} x contribution ${formatScore(hit.score_breakdown.body_contribution)}`,
    `- base total: ${formatScore(hit.score_breakdown.base_total)}`,
    `- phrase bonus: ${formatScore(hit.score_breakdown.phrase_bonus)}`,
    `- penalty multiplier: ${formatScore(hit.score_breakdown.penalty_multiplier)}`,
    `- pre-authority score: ${formatScore(hit.score_breakdown.pre_authority_score)}`,
    `- authority bonus: ${formatScore(hit.score_breakdown.authority_bonus)}`,
    `- dense minus lexical: ${formatScore(hit.score_breakdown.dense_minus_lexical)}`,
    `- reconstruction gap: ${formatScore(hit.score_breakdown.reconstruction_gap)}`,
    `- exact heading phrase: ${hit.score_breakdown.exact_heading_phrase ? "yes" : "no"}`,
    `- exact body phrase: ${hit.score_breakdown.exact_body_phrase ? "yes" : "no"}`,
    "",
    "Diagnostics",
    ...diagnostics,
    "",
    "Chunk text",
    hit.text,
  ].join("\n")
}

function formatMetricSeries(label: string, points: MetricPoint[]): string[] {
  if (points.length === 0) {
    return [`${label}: n/a`]
  }

  return [
    `${label}: ${points.map((point) => `@${point.k} ${point.value.toFixed(3)}`).join(" | ")}`,
  ]
}

function formatEval(evalReport: DebugEvalResponse | null): string {
  if (!qrelsPath || !queriesPath) {
    return [
      "Evaluation is not configured.",
      "",
      "Set both of these environment variables and press Ctrl+E:",
      "- RETRIEVAL_XRAY_QRELS=/path/to/qrels.tsv",
      "- RETRIEVAL_XRAY_QUERIES=/path/to/queries.tsv",
    ].join("\n")
  }

  if (!evalReport) {
    return [
      `Qrels: ${qrelsPath}`,
      `Queries: ${queriesPath}`,
      "",
      "Press Ctrl+E to load aggregate retrieval metrics.",
    ].join("\n")
  }

  const worst =
    evalReport.worst_queries.length === 0
      ? ["- no judged query rows available"]
      : evalReport.worst_queries.map(
          (row) =>
            `- ${row.query_id} | RR ${row.reciprocal_rank.toFixed(3)} | Recall ${row.recall_at_top_k.toFixed(3)} | ${truncate(row.query, 34)}`,
        )

  return [
    `Qrels: ${evalReport.qrels_path}`,
    `Queries: ${evalReport.queries_path}`,
    `Elapsed: ${evalReport.elapsed_ms} ms`,
    `Judged queries: ${evalReport.num_queries}`,
    `MRR: ${evalReport.mrr.toFixed(3)}`,
    ...formatMetricSeries("NDCG", evalReport.ndcg_at),
    ...formatMetricSeries("Recall", evalReport.recall_at),
    "",
    "Worst queries",
    ...worst,
  ].join("\n")
}

function applySearch(search: DebugSearchResponse): void {
  currentSearch = search
  resultsPane.title = `Ranking | ${search.result_count} hits`
  resultsList.options = buildResultOptions(search.results)

  const selected = search.results[0] ?? null
  if (search.results.length > 0) {
    resultsList.setSelectedIndex(0)
  }
  detailText.content = formatDetail(selected)
  detailPane.title = selected ? `Chunk detail | #${selected.rank}` : "Chunk detail"
  summaryText.content = formatSummary(search, selected)
  detailScroll.scrollTop = 0
}

function applySelection(option: SelectOption | null): void {
  const hit = (option?.value ?? null) as DebugHit | null
  detailText.content = formatDetail(hit)
  detailPane.title = hit ? `Chunk detail | #${hit.rank}` : "Chunk detail"
  summaryText.content = formatSummary(currentSearch, hit)
  detailScroll.scrollTop = 0
}

async function fetchJson<T>(path: string, params: Record<string, string>, signal?: AbortSignal): Promise<T> {
  const url = new URL(path, apiBase)
  for (const [key, value] of Object.entries(params)) {
    if (value.trim().length > 0) {
      url.searchParams.set(key, value)
    }
  }

  const timeout = AbortSignal.timeout(120_000)
  const combined = signal ? AbortSignal.any([signal, timeout]) : timeout

  const response = await fetch(url, { signal: combined })

  if (!response.ok) {
    const body = await response.text()
    throw new Error(`${response.status} ${response.statusText}: ${body}`)
  }

  return (await response.json()) as T
}

async function runSearch(query: string): Promise<void> {
  const trimmed = query.trim()
  if (!trimmed) {
    setStatus("enter a query")
    return
  }

  searchAbort?.abort()
  searchAbort = new AbortController()
  const { signal } = searchAbort

  const seq = ++searchSequence
  setStatus(`searching "${trimmed}"`)
  summaryText.content = "Running retrieval..."
  headerStatus.fg = palette.muted

  try {
    const response = await fetchJson<DebugSearchResponse>("/debug/api/search", {
      q: trimmed,
      k: "10",
    }, signal)
    if (seq !== searchSequence) {
      return
    }
    queryInput.value = trimmed
    applySearch(response)
    headerStatus.fg = palette.success
    setStatus(`ready | ${response.elapsed_ms} ms | ${response.result_count} hits`)
  } catch (error) {
    if (signal.aborted || seq !== searchSequence) {
      return
    }
    const message = error instanceof Error ? error.message : String(error)
    currentSearch = null
    resultsList.options = []
    resultsPane.title = "Ranking | 0 hits"
    detailText.content = `Search failed.\n\n${message}`
    detailPane.title = "Chunk detail"
    summaryText.content = ""
    headerStatus.fg = palette.danger
    setStatus("search failed")
  }
}

async function loadEvaluation(): Promise<void> {
  if (!qrelsPath || !queriesPath) {
    currentEval = null
    evalText.content = formatEval(null)
    return
  }

  const seq = ++evalSequence
  evalText.content = `Loading evaluation from\n${qrelsPath}\n${queriesPath}`

  try {
    const report = await fetchJson<DebugEvalResponse>("/debug/api/eval", {
      qrels: qrelsPath,
      queries: queriesPath,
      k: kValues,
    })
    if (seq !== evalSequence) {
      return
    }
    currentEval = report
    evalText.content = formatEval(report)
    evalScroll.scrollTop = 0
  } catch (error) {
    if (seq !== evalSequence) {
      return
    }
    const message = error instanceof Error ? error.message : String(error)
    currentEval = null
    evalText.content = `Evaluation failed.\n\n${message}`
  }
}

function cycleFocus(forward: boolean): void {
  focusRing[focusIndex]?.blur()
  focusIndex = (focusIndex + (forward ? 1 : -1) + focusRing.length) % focusRing.length
  focusRing[focusIndex]?.focus()
}

resultsList.on(SelectRenderableEvents.SELECTION_CHANGED, (_index: number, option: SelectOption) => {
  applySelection(option)
})

resultsList.on(SelectRenderableEvents.ITEM_SELECTED, () => {
  focusRing[focusIndex]?.blur()
  focusIndex = focusRing.indexOf(detailScroll)
  detailScroll.focus()
})

queryInput.on(InputRenderableEvents.ENTER, (value: string) => {
  void runSearch(value)
})

renderer.keyInput.on("keypress", (key: KeyEvent) => {
  if (key.name === "tab") {
    cycleFocus(!key.shift)
    return
  }

  if (key.name === "escape") {
    renderer.destroy()
    return
  }

  if ((key.ctrl && key.name === "l") || key.name === "/") {
    focusRing[focusIndex]?.blur()
    focusIndex = 0
    queryInput.focus()
    return
  }

  if (key.ctrl && key.name === "r") {
    void runSearch(queryInput.value)
    return
  }

  if (key.ctrl && key.name === "e") {
    void loadEvaluation()
  }
})

renderer.on("resize", (width: number) => {
  resizePanels(width)
})

resizePanels(process.stdout.columns || 160)
queryInput.focus()
renderer.start()
evalText.content = formatEval(currentEval)
setStatus("ready")

await loadEvaluation()
await runSearch(initialQuery)
