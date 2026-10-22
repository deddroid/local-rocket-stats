// Calculations shared by the dashboard, the phone page and the overlay.
import { THRESHOLDS } from "/ranks.js";
import { t } from "/i18n.js";

export async function api(path, opts = {}) {
  // Screenshot mode: data embedded in the page by the server (see `?preload=1`).
  const pre = window.__PRELOAD__;
  if (pre && !opts.method && path in pre) {
    const v = pre[path];
    delete pre[path];
    return v;
  }
  const res = await fetch(path, {
    ...opts,
    headers: opts.body ? { "Content-Type": "application/json" } : undefined,
  });
  if (!res.ok) throw new Error(`${res.status} ${path}`);
  return res.json();
}

export const esc = (s) =>
  String(s ?? "").replace(/[&<>"']/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;", "'": "&#39;" })[c]);

export function playlistName(meta, id) {
  if (id == null) return "?";
  return meta.playlists[String(id)] ?? `Playlist ${id}`;
}

export function arenaName(meta, asset) {
  if (!asset) return "";
  const k = asset.toLowerCase();
  if (meta.arenas[k]) return meta.arenas[k];
  return k.replace(/_p$/, "").replace(/_/g, " ").replace(/\b\w/g, (c) => c.toUpperCase());
}

// Adds derived fields to the matches (sorted by date, oldest first).
export function enrich(matches, meta) {
  const ranked = new Set(meta.ranked);
  const lastByPlaylist = new Map();
  const gapMs = (meta.session_gap_min ?? 30) * 60_000;
  let session = 0;
  let prevEnd = null;
  for (const m of matches) {
    m.start = new Date(m.started_at);
    m.end = m.ended_at ? new Date(m.ended_at) : m.start;
    if (prevEnd && m.start - prevEnd > gapMs) session++;
    m.session = session;
    prevEnd = m.end;
    m.ranked = ranked.has(m.playlist_id);
    m.playlist = playlistName(meta, m.playlist_id);
    const swap = m.my_team === 1;
    m.myScore = swap ? m.team1_score : m.team0_score;
    m.oppScore = swap ? m.team0_score : m.team1_score;
    // MMR change: against the MMR read in the queue before the match, otherwise against the
    // final MMR of the previous match in the same playlist.
    const prev = lastByPlaylist.get(m.playlist_id);
    m.prevMmr = m.mmr_before ?? prev?.mmr ?? null;
    m.delta = m.mmr != null && m.prevMmr != null ? m.mmr - m.prevMmr : null;
    lastByPlaylist.set(m.playlist_id, m);
  }
  // Estimated final MMR while the exact one is unknown (e.g. last match of the evening: the game
  // logs it only at the next queue): MMR before ± the average change per win/loss.
  const avg = (pl, sign) => {
    const d = matches.filter((m) => m.playlist_id === pl && m.delta != null && Math.sign(m.delta) === sign).map((m) => Math.abs(m.delta));
    return d.length ? d.reduce((a, b) => a + b, 0) / d.length : 9;
  };
  for (const m of matches) {
    m.mmrEstimate = null;
    m.deltaEstimate = null;
    if (m.mmr == null && m.mmr_before != null && ["W", "L", "A"].includes(m.result)) {
      const sign = m.result === "W" ? 1 : -1;
      m.deltaEstimate = sign * Math.round(avg(m.playlist_id, sign));
      m.mmrEstimate = m.mmr_before + m.deltaEstimate;
    }
  }
  return matches;
}

export function summarize(list) {
  const s = { n: list.length, w: 0, l: 0, a: 0, goals: 0, assists: 0, saves: 0, shots: 0,
              score: 0, demos: 0, mvp: 0, epicSaves: 0, mmrDelta: {}, mmrApproxBy: {}, streak: 0, statN: 0, scoreN: 0 };
  for (const m of list) {
    if (m.result === "W") s.w++;
    else if (m.result === "L") s.l++;
    else if (m.result === "A") s.a++;
    // Matches rebuilt from the log have no goals/saves/...: averages only use those that do.
    if (m.goals != null) s.statN++;
    if (m.score != null) s.scoreN++;
    for (const k of ["goals", "assists", "saves", "shots", "score", "demos"]) s[k] += m[k] ?? 0;
    s.mvp += m.statfeed?.MVP ?? 0;
    s.epicSaves += m.statfeed?.EpicSave ?? 0;
    const d = m.delta ?? m.deltaEstimate;
    if (d != null) s.mmrDelta[m.playlist] = (s.mmrDelta[m.playlist] ?? 0) + d;
    if (m.delta == null && m.deltaEstimate != null) {
      s.mmrApprox = true;
      s.mmrApproxBy[m.playlist] = true;
    }
  }
  const decided = s.w + s.l + s.a;
  s.winrate = decided ? s.w / decided : null;
  s.per = (k) => {
    const n = k === "score" ? s.scoreN : s.statN;
    return n ? s[k] / n : null;
  };
  s.shootingPct = s.shots ? s.goals / s.shots : null;
  // Current streak (positive = wins in a row, negative = losses).
  for (let i = list.length - 1; i >= 0; i--) {
    const r = list[i].result;
    if (r === "?") continue;
    const win = r === "W";
    if (s.streak === 0) s.streak = win ? 1 : -1;
    else if (win === s.streak > 0) s.streak += win ? 1 : -1;
    else break;
  }
  return s;
}

const DIVISIONS = ["I", "II", "III"];
const RANKS = ["Bronze", "Silver", "Gold", "Platinum", "Diamond", "Champion", "Grand Champion"];

export function tierName(tier) {
  if (tier == null) return null;
  if (tier <= 0) return "Unranked";
  if (tier >= 22) return "Supersonic Legend";
  return `${RANKS[Math.floor((tier - 1) / 3)]} ${DIVISIONS[(tier - 1) % 3]}`;
}

const DIV_NAMES = ["Div I", "Div II", "Div III", "Div IV"];

// Rank, division and distance to the next division/rank for an MMR.
// tierHint is the tier read from the game log: when present it wins (the thresholds are estimates).
export function rankInfo(playlistId, mmr, tierHint = null) {
  const rows = THRESHOLDS[playlistId];
  if (!rows || mmr == null) return null;
  const at = (i) => ({ tier: rows[i][0], div: rows[i][1], min: rows[i][2] });
  let i = 0;
  for (let k = 0; k < rows.length; k++) if (rows[k][2] <= mmr) i = k;
  if (tierHint != null && tierHint > 0 && rows[i][0] !== tierHint) {
    const idx = rows.map((r, k) => (r[0] === tierHint ? k : -1)).filter((k) => k >= 0);
    if (idx.length) {
      const below = idx.filter((k) => rows[k][2] <= mmr);
      i = below.length ? below.at(-1) : idx[0];
    }
  }
  const cur = at(i);
  const next = i + 1 < rows.length ? at(i + 1) : null;
  const nr = rows.findIndex((r) => r[0] > cur.tier);
  const nextRank = nr >= 0 ? at(nr) : null;
  const span = next ? next.min - cur.min : 0;
  return {
    ...cur,
    name: tierName(cur.tier),
    divName: cur.tier >= 22 ? "" : DIV_NAMES[cur.div - 1],
    mmr,
    progress: span > 0 ? Math.min(1, Math.max(0, (mmr - cur.min) / span)) : 1,
    next: next && { ...next, name: tierName(next.tier), divName: DIV_NAMES[next.div - 1] ?? "", need: Math.max(0, Math.ceil(next.min - mmr)) },
    nextRank: nextRank && { ...nextRank, name: tierName(nextRank.tier), need: Math.max(0, Math.ceil(nextRank.min - mmr)) },
  };
}

// Average MMR gained per win in a playlist (from your matches), 9 if unknown.
export function avgWinGain(matches, playlistId) {
  const gains = matches.filter((m) => m.playlist_id === playlistId && m.result === "W" && m.delta > 0).map((m) => m.delta);
  return gains.length ? gains.reduce((a, b) => a + b, 0) / gains.length : 9;
}

// Latest known MMR per playlist: the newest of a match's final MMR and the MMR read in the queue.
export function currentMmr(matches, ranks, playlistId) {
  const r = ranks[playlistId];
  const last = matches.filter((m) => m.playlist_id === playlistId && (m.mmr ?? m.mmrEstimate) != null).at(-1);
  // The log's tier belongs to the queued MMR; after a match the rank may have changed.
  if (last && (!r || last.end > new Date(r.at)))
    return { mmr: last.mmr ?? last.mmrEstimate, tier: null, estimated: last.mmr == null };
  return r ? { mmr: r.mmr, tier: r.tier, estimated: false } : null;
}

// Rank cards (rank, division, progress) for the given ranked playlists.
export function rankCardsHtml(matches, ranks, meta, ids) {
  return ids.map((id) => {
    const cur = currentMmr(matches, ranks, id);
    const info = cur && rankInfo(id, cur.mmr, cur.tier);
    if (!info) return "";
    const gain = avgWinGain(matches, id);
    const wins = (need) => {
      const n = Math.ceil(need / gain);
      return need > 0 ? ` <span class="muted">(${n === 1 ? t("wins_one") : t("wins_many", { n })})</span>` : "";
    };
    const lines = [];
    if (info.next) lines.push(t("rank_need", { n: info.next.need, name: esc(`${info.next.name} ${info.next.divName}`.trim()) }) + wins(info.next.need));
    if (info.nextRank && info.nextRank.tier !== info.next?.tier)
      lines.push(t("rank_need", { n: info.nextRank.need, name: esc(info.nextRank.name) }) + wins(info.nextRank.need));
    const nextMin = info.next ? info.next.min : info.min;
    return `<div class="rank-card tier-${Math.ceil(info.tier / 3)}">
      <div class="rank-head">
        <span class="muted">${esc(playlistName(meta, id).replace("Ranked ", ""))}</span>
        <span class="rank-mmr" title="${cur.estimated ? t("rank_est") : ""}">${cur.estimated ? "≈" : ""}${Math.round(info.mmr)} <span class="muted">MMR</span></span>
      </div>
      <div class="rank-name">${esc(info.name)} <span>${esc(info.divName)}</span></div>
      <div class="rank-bar" role="progressbar" aria-valuemin="${info.min}" aria-valuemax="${nextMin}" aria-valuenow="${Math.round(info.mmr)}">
        <div style="width:${(info.progress * 100).toFixed(1)}%"></div>
      </div>
      <div class="rank-scale muted"><span>${info.min}</span><span>${info.next ? info.next.min : ""}</span></div>
      ${lines.map((l) => `<div class="rank-need">${l}</div>`).join("")}
    </div>`;
  }).join("");
}

export const fmtDelta = (d) => (d == null ? "" : `${d > 0 ? "+" : ""}${Math.round(d)}`);
export const pct = (x) => (x == null ? "–" : `${Math.round(x * 100)}%`);
