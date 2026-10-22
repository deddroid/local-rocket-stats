import { api, esc, playlistName, arenaName, enrich, summarize, fmtDelta, pct, rankCardsHtml } from "/common.js";
import { t, setLang, locale, applyI18n } from "/i18n.js";

const $ = (s) => document.querySelector(s);
const state = {
  meta: null,
  matches: [],
  ranks: {},
  version: -1,
  open: new Set(),          // expanded rows
  skipped: new Set(),       // matches whose MMR banner was skipped
  filters: { period: "session", playlist: "all", chart: null },
};

try {
  Object.assign(state.filters, JSON.parse(localStorage.getItem("rs-filters") || "{}"));
} catch {}

async function loadMeta() {
  state.meta = await api("/api/meta");
  if (state.meta.lang !== state.lang) {
    state.lang = state.meta.lang;
    setLang(state.lang);
    applyI18n();
    $("#mmr-banner").removeAttribute("data-id");
  }
}

async function loadMatches() {
  const [matches, ranks] = await Promise.all([api("/api/matches"), api("/api/ranks")]);
  state.matches = enrich(matches, state.meta);
  state.ranks = ranks;
  render();
}

// --- filters ---------------------------------------------------------------
function filtered() {
  const { period, playlist } = state.filters;
  let list = state.matches;
  if (playlist !== "all") list = list.filter((m) => String(m.playlist_id) === playlist);
  if (period === "session") {
    const last = state.matches.at(-1);
    list = last ? list.filter((m) => m.session === last.session) : [];
  } else if (period === "today") {
    const d = new Date(); d.setHours(0, 0, 0, 0);
    list = list.filter((m) => m.start >= d);
  } else if (period !== "all") {
    const from = Date.now() - Number(period) * 86_400_000;
    list = list.filter((m) => m.start >= from);
  }
  return list;
}

function saveFilters() {
  try { localStorage.setItem("rs-filters", JSON.stringify(state.filters)); } catch {}
}

// --- render ---------------------------------------------------------------
function render() {
  renderFilterOptions();
  const list = filtered();
  renderRanks();
  renderTiles(list);
  renderChart();
  renderTable(list);
  renderBanner();
}

function renderFilterOptions() {
  const played = [...new Set(state.matches.map((m) => m.playlist_id))].filter((x) => x != null);
  const sel = $("#f-playlist");
  sel.innerHTML = `<option value="all">${t("all")}</option>` +
    played.map((id) => `<option value="${id}">${esc(playlistName(state.meta, id))}</option>`).join("");
  if (![...sel.options].some((o) => o.value === state.filters.playlist)) state.filters.playlist = "all";
  sel.value = state.filters.playlist;
  $("#f-period").value = state.filters.period;

  const ranked = played.filter((id) => state.meta.ranked.includes(id));
  const counts = (id) => state.matches.filter((m) => m.playlist_id === id).length;
  ranked.sort((a, b) => counts(b) - counts(a));
  const csel = $("#chart-playlist");
  csel.innerHTML = ranked.map((id) => `<option value="${id}">${esc(playlistName(state.meta, id))}</option>`).join("");
  if (!ranked.map(String).includes(String(state.filters.chart))) state.filters.chart = ranked[0] ?? null;
  csel.value = state.filters.chart ?? "";
  csel.hidden = ranked.length === 0;

  const last = state.matches.at(-1);
  $("#period-label").textContent = state.filters.period === "session" && last
    ? t("session_of", { date: last.start.toLocaleDateString(locale(), { weekday: "long", day: "numeric", month: "long" }) })
    : "";
}

const fmtAvg = (v) => (v == null ? "–" : v.toFixed(2));

function tile(label, value, sub = "", cls = "") {
  return `<div class="tile"><div class="label">${label}</div><div class="value ${cls}">${value}</div><div class="sub">${sub}</div></div>`;
}

function renderRanks() {
  const ids = state.filters.playlist === "all" ? state.meta.ranked : [Number(state.filters.playlist)];
  const html = rankCardsHtml(state.matches, state.ranks, state.meta, ids);
  $("#ranks").innerHTML = html;
  $("#ranks").hidden = !html;
}

function renderTiles(list) {
  const s = summarize(list);
  const streak = s.streak > 0 ? t("streak_w", { n: s.streak }) : s.streak < 0 ? t("streak_l", { n: -s.streak }) : "–";
  const mmrTiles = Object.entries(s.mmrDelta).map(([pl, d]) =>
    tile(t("t_mmr", { pl: esc(pl.replace("Ranked ", "")) }), (s.mmrApproxBy[pl] ? "≈" : "") + fmtDelta(d), t(s.mmrApproxBy[pl] ? "t_period_est" : "t_period"), d > 0 ? "pos" : d < 0 ? "neg" : ""));
  $("#tiles").innerHTML = [
    tile(t("t_matches"), s.n, t("t_matches_sub", { w: s.w, l: s.l }) + (s.a ? t("t_abandoned", { n: s.a }) : "")),
    tile(t("t_winrate"), pct(s.winrate), "", s.winrate == null ? "" : s.winrate >= 0.5 ? "pos" : "neg"),
    tile(t("t_streak"), streak, "", s.streak > 0 ? "pos" : s.streak < 0 ? "neg" : ""),
    ...mmrTiles,
    tile(t("t_goals"), fmtAvg(s.per("goals")), t("total", { n: s.goals })),
    tile(t("t_assists"), fmtAvg(s.per("assists")), t("total", { n: s.assists })),
    tile(t("t_saves"), fmtAvg(s.per("saves")), t("epic", { n: s.epicSaves })),
    tile(t("t_shooting"), pct(s.shootingPct), t("shots_of", { g: s.goals, s: s.shots })),
    tile(t("t_score"), s.per("score") == null ? "–" : Math.round(s.per("score")), t("mvp", { n: s.mvp })),
  ].join("");
}

function renderChart() {
  const el = $("#chart");
  const id = state.filters.chart;
  const pts = state.matches.filter((m) => String(m.playlist_id) === String(id) && m.mmr != null).slice(-60);
  if (pts.length < 2) {
    el.innerHTML = `<div class="empty-msg">${id == null
      ? t("chart_none") : t("chart_few")}</div>`;
    return;
  }
  const W = Math.max(320, el.clientWidth || 1000), H = 240, L = 44, R = 12, T = 12, B = 22;
  const vals = pts.map((m) => m.mmr);
  let lo = Math.min(...vals), hi = Math.max(...vals);
  const pad = Math.max(10, (hi - lo) * 0.1);
  lo -= pad; hi += pad;
  const x = (i) => L + (i * (W - L - R)) / (pts.length - 1);
  const y = (v) => T + ((hi - v) * (H - T - B)) / (hi - lo);
  const ticks = 4;
  let grid = "";
  for (let i = 0; i <= ticks; i++) {
    const v = lo + ((hi - lo) * i) / ticks;
    grid += `<line class="grid" x1="${L}" x2="${W - R}" y1="${y(v)}" y2="${y(v)}"/>` +
      `<text x="${L - 6}" y="${y(v) + 4}" text-anchor="end">${Math.round(v)}</text>`;
  }
  const line = pts.map((m, i) => `${x(i)},${y(m.mmr)}`).join(" ");
  const dots = pts.map((m, i) =>
    `<circle class="dot-${m.result}" cx="${x(i)}" cy="${y(m.mmr)}" r="4"><title>${esc(
      `${m.start.toLocaleString(locale())} · ${t(m.result === "W" ? "win" : "loss")} ${m.myScore ?? "?"}-${m.oppScore ?? "?"} · MMR ${Math.round(m.mmr)} ${fmtDelta(m.delta)}`
    )}</title></circle>`).join("");
  const first = pts[0].start.toLocaleDateString(locale()), last = pts.at(-1).start.toLocaleDateString(locale());
  el.innerHTML = `<svg viewBox="0 0 ${W} ${H}" role="img" aria-label="${t("mmr_trend")}">
    ${grid}<polyline class="line" points="${line}"/>${dots}
    <text x="${L}" y="${H - 4}">${first}</text><text x="${W - R}" y="${H - 4}" text-anchor="end">${last}</text></svg>`;
}

function resultCell(m) {
  const label = t(`res_${m.result ?? "?"}`);
  const score = m.myScore != null ? `${m.myScore}–${m.oppScore}${m.overtime ? " OT" : ""}` : "";
  return `<span class="res res-${esc(m.result)}">${label}</span>${score}`;
}

function renderTable(list) {
  const rows = [];
  const desc = [...list].reverse();
  desc.forEach((m, i) => {
    const brk = i > 0 && desc[i - 1].session !== m.session ? " session-break" : "";
    const d = m.delta;
    rows.push(`<tr class="match${brk}" data-id="${m.id}">
      <td>${m.start.toLocaleString(locale(), { day: "2-digit", month: "2-digit", hour: "2-digit", minute: "2-digit" })}</td>
      <td>${esc(m.playlist)}<div class="muted">${esc(arenaName(state.meta, m.arena))}</div></td>
      <td>${resultCell(m)}</td>
      <td class="num">${m.score ?? ""}</td>
      <td class="num">${[m.goals, m.assists, m.saves, m.shots].map((v) => v ?? "–").join(" / ")}</td>
      <td class="num">${m.ranked || m.mmr != null
        ? `<input class="mmr${m.mmr_auto ? " auto" : ""}" type="number" step="1" value="${m.mmr != null ? Math.round(m.mmr) : ""}" placeholder="${m.mmrEstimate != null ? `≈${Math.round(m.mmrEstimate)}` : ""}" data-id="${m.id}" aria-label="MMR" title="${m.mmr_auto ? t("mmr_auto") : m.mmr == null && m.mmr_before != null ? t("mmr_est") : ""}">`
        : ""}</td>
      <td class="num ${d > 0 ? "pos" : d < 0 ? "neg" : ""}">${d != null ? fmtDelta(d)
        : m.deltaEstimate != null ? `<span class="muted" title="${t("estimate")}">≈${fmtDelta(m.deltaEstimate)}</span>` : ""}</td>
      <td><button class="icon-btn del" data-id="${m.id}" title="${t("delete_match")}">✕</button></td>
    </tr>`);
    if (state.open.has(m.id)) rows.push(`<tr class="details"><td colspan="8">${details(m)}</td></tr>`);
  });
  $("#matches tbody").innerHTML = rows.join("");
  $("#empty").hidden = state.matches.length > 0;
  $("#match-count").textContent = t("count", { n: list.length, total: state.matches.length });
}

function details(m) {
  const players = (m.players ?? []).map((p) => `<tr class="${p.id && p.id === m.me_id ? "me-row" : ""}">
      <td class="team-${p.team}">${esc(p.name)}${p.left ? ` <span class="muted">${t("left")}</span>` : ""}</td>
      <td class="num">${p.score}</td><td class="num">${p.goals}</td><td class="num">${p.assists}</td>
      <td class="num">${p.saves}</td><td class="num">${p.shots}</td><td class="num">${p.demos}</td></tr>`).join("");
  const goals = (m.goals_log ?? []).map((g) => {
    const when = g.time_left != null ? `${g.overtime ? "+" : ""}${Math.floor(g.time_left / 60)}:${String(g.time_left % 60).padStart(2, "0")}` : "";
    return `<tr><td>${when}</td><td class="team-${g.team}">${esc(g.scorer)}${g.assister ? ` <span class="muted">(${esc(g.assister)})</span>` : ""}</td><td class="num">${g.speed_kph} km/h</td></tr>`;
  }).join("");
  const feed = Object.entries(m.statfeed ?? {}).map(([k, v]) => `${esc(k)} ×${v}`).join(" · ");
  return `<div class="details-grid">
    <div><table><thead><tr><th>${t("player")}</th>${["d_points", "d_g", "d_a", "d_s", "d_sh", "d_demo"].map((k) => `<th class="num">${t(k)}</th>`).join("")}</tr></thead><tbody>${players}</tbody></table></div>
    <div><table><thead><tr><th>${t("time")}</th><th>${t("goal")}</th><th class="num">${t("speed")}</th></tr></thead><tbody>${goals || `<tr><td colspan="3" class="muted">${t("no_goals")}</td></tr>`}</tbody></table>
    ${feed ? `<p class="muted">${feed}</p>` : ""}
    ${m.me_name || m.source === "log" ? "" : `<p class="muted">${t("who_unknown")}</p>`}</div>
  </div>`;
}

// Banner to type the MMR of the last ranked match.
function renderBanner() {
  const m = state.matches.at(-1);
  const form = $("#mmr-banner");
  const show = m && m.ranked && m.mmr == null && (m.result === "W" || m.result === "L" || m.result === "A") &&
    Date.now() - m.end < 3 * 3600_000 && !state.skipped.has(m.id);
  if (!show) { form.hidden = true; delete form.dataset.id; return; }
  if (form.dataset.id === String(m.id)) return;  // already shown: leave the input alone
  form.dataset.id = m.id;
  const label = t({ W: "win", L: "loss", A: "abandoned" }[m.result]);
  $("#mmr-banner-title").textContent = t("banner_title", { label, pl: m.playlist }) + (m.myScore != null ? ` (${m.myScore}–${m.oppScore})` : "");
  $("#mmr-banner-hint").textContent = m.mmr_before != null
    ? t("banner_before", { mmr: Math.round(m.mmr_before), pl: m.playlist })
    : t("banner_none");
  const input = $("#mmr-banner-input");
  const guess = m.mmrEstimate ?? m.prevMmr;
  input.value = guess != null ? Math.round(guess) : "";
  form.hidden = false;
}

function renderLive(live) {
  const conn = $("#conn");
  conn.textContent = t(live.connected ? (live.match ? "conn_match" : "conn_game") : "conn_off");
  conn.classList.toggle("on", live.connected);
  const el = $("#live");
  const m = live.match;
  if (!m || !m.players.length) { el.hidden = true; return; }
  const mine = m.my_team ?? 0, opp = 1 - mine;
  const secs = m.time_left ?? 0;
  const clock = `${m.overtime ? "+" : ""}${Math.floor(secs / 60)}:${String(secs % 60).padStart(2, "0")}`;
  const me = m.me;
  el.innerHTML = `<div class="scoreline">
      <span class="muted">${esc(playlistName(state.meta, m.playlist_id))} · ${esc(arenaName(state.meta, m.arena))}</span>
      <span class="score"><span class="team-${mine}">${m.score[mine]}</span> – <span class="team-${opp}">${m.score[opp]}</span></span>
      <span class="clock">${m.ended ? t("finished") : clock}</span>
    </div>
    ${me ? `<div class="live-stats">
      ${["points", "goals", "assists", "saves", "shots", "demos", "touches"].map((k) => `<span>${t(`l_${k}`)} <b>${me[k === "points" ? "score" : k]}</b></span>`).join("")}</div>` : ""}`;
  el.hidden = false;
}

// --- actions ---------------------------------------------------------------
async function saveMmr(id, value) {
  await api(`/api/matches/${id}`, { method: "POST", body: JSON.stringify({ mmr: value === "" ? null : Number(value) }) });
  await loadMatches();
}

function bind() {
  $("#f-period").addEventListener("change", (e) => { state.filters.period = e.target.value; saveFilters(); render(); });
  $("#f-playlist").addEventListener("change", (e) => { state.filters.playlist = e.target.value; saveFilters(); render(); });
  $("#chart-playlist").addEventListener("change", (e) => { state.filters.chart = e.target.value; saveFilters(); renderChart(); });

  $("#mmr-banner").addEventListener("submit", async (e) => {
    e.preventDefault();
    const form = e.currentTarget;
    await saveMmr(form.dataset.id, $("#mmr-banner-input").value);
  });
  $("#mmr-banner-skip").addEventListener("click", () => {
    state.skipped.add(Number($("#mmr-banner").dataset.id));
    renderBanner();
  });

  const tbody = $("#matches tbody");
  tbody.addEventListener("click", async (e) => {
    const del = e.target.closest(".del");
    if (del) {
      if (confirm(t("confirm_delete"))) {
        await api(`/api/matches/${del.dataset.id}`, { method: "DELETE" });
        await loadMatches();
      }
      return;
    }
    if (e.target.closest("input")) return;
    const row = e.target.closest("tr.match");
    if (!row) return;
    const id = Number(row.dataset.id);
    state.open.has(id) ? state.open.delete(id) : state.open.add(id);
    renderTable(filtered());
  });
  tbody.addEventListener("change", (e) => {
    if (e.target.matches("input.mmr")) saveMmr(e.target.dataset.id, e.target.value);
  });
  tbody.addEventListener("keydown", (e) => {
    if (e.key === "Enter" && e.target.matches("input.mmr")) e.target.blur();
  });

  let resizeTimer;
  window.addEventListener("resize", () => { clearTimeout(resizeTimer); resizeTimer = setTimeout(renderChart, 150); });

  $("#open-settings").addEventListener("click", openSettings);
  $("#settings").addEventListener("close", saveSettings);
}

function openSettings() {
  $("#s-lang").value = state.meta.lang_auto ? "" : state.meta.lang;
  $("#s-me").value = state.meta.me_ids.join("\n");
  $("#s-gap").value = state.meta.session_gap_min;
  // Suggest the most frequent ids: one of them is almost certainly you.
  const freq = new Map();
  for (const m of state.matches) for (const p of m.players ?? []) {
    if (!p.id || /^unknown/i.test(p.id)) continue;
    const e = freq.get(p.id) ?? { name: p.name, n: 0 };
    e.n++; freq.set(p.id, e);
  }
  const top = [...freq.entries()].sort((a, b) => b[1].n - a[1].n).slice(0, 4).filter(([, e]) => e.n > 1);
  $("#s-candidates").innerHTML = top.map(([id, e]) =>
    `<button type="button" class="btn ghost" data-id="${esc(id)}">${esc(e.name)} · ${t("games_n", { n: e.n })}</button>`).join("");
  $("#s-candidates").onclick = (ev) => {
    const b = ev.target.closest("button");
    if (!b) return;
    const ta = $("#s-me");
    if (!ta.value.split("\n").includes(b.dataset.id)) ta.value = (ta.value.trim() + "\n" + b.dataset.id).trim();
  };
  $("#settings").showModal();
}

async function saveSettings() {
  if ($("#settings").returnValue !== "save") return;
  await api("/api/settings", { method: "POST", body: JSON.stringify({
    me_ids: $("#s-me").value.split("\n").map((s) => s.trim()).filter(Boolean),
    session_gap_min: Number($("#s-gap").value) || 30,
    lang: $("#s-lang").value || null,
  }) });
  await loadMeta();
  await loadMatches();
}

// --- start ----------------------------------------------------------------
async function poll() {
  try {
    const live = await api("/api/live");
    renderLive(live);
    if (live.version !== state.version) {
      state.version = live.version;
      await loadMeta();
      await loadMatches();
    }
  } catch {
    $("#conn").textContent = t("conn_app");
    $("#conn").classList.remove("on");
  }
  setTimeout(poll, 1000);
}

await loadMeta();
bind();
await loadMatches();
poll();
setInterval(renderBanner, 60_000);
