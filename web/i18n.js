// English and Italian texts for the web UI. The language comes from /api/meta (settings,
// shared with the tray menu); `data-i18n*` attributes translate the static HTML.

const STRINGS = {
  en: {
    conn_game: "Game connected", conn_match: "In match", conn_off: "Game not connected", conn_app: "App not reachable",
    export_csv: "Export CSV", settings: "Settings", save: "Save", skip: "Skip", cancel: "Cancel",
    period: "Period", p_session: "Last session", p_today: "Today", p_7: "7 days", p_30: "30 days", p_all: "All",
    playlist: "Playlist", all: "All", session_of: "session of {date}",
    mmr_trend: "MMR trend", matches: "Matches", date: "Date", result: "Result", points: "Points",
    gasp_title: "Goals / Assists / Saves / Shots", gasp: "G/A/S/Sh",
    empty: "No matches yet. Start Rocket League with the Stats API enabled (tray menu, or <code>local-rocket-stats setup</code>) and play: matches will show up here.",
    accounts: "Your accounts (PrimaryId, one per line)", session_gap: "Break that separates sessions (minutes)",
    language: "Language", lang_auto: "System",
    t_matches: "Matches", t_matches_sub: "{w}W · {l}L", t_abandoned: " · {n} left early",
    t_winrate: "Win rate", t_streak: "Current streak", t_mmr: "MMR {pl}", t_period: "in the period", t_period_est: "in the period (estimate)",
    t_goals: "Goals / match", t_assists: "Assists / match", t_saves: "Saves / match", t_shooting: "Shooting %",
    t_score: "Average score", total: "{n} total", epic: "{n} epic", shots_of: "{g}/{s} shots", mvp: "{n} MVP",
    streak_w: "{n}W", streak_l: "{n}L",
    chart_none: "Play a ranked match to see the trend.", chart_few: "At least 2 matches with MMR are needed to draw the chart.",
    win: "Win", loss: "Loss", abandoned: "Left early",
    res_W: "W", res_L: "L", res_A: "LE", "res_?": "?",
    mmr_auto: "Read automatically from the game log",
    mmr_est: "Estimate: the exact value arrives at your next queue in this playlist, or type it in",
    estimate: "Estimate", delete_match: "Delete match", confirm_delete: "Delete this match from the history?",
    count: "{n} in the period · {total} total",
    player: "Player", time: "Time", goal: "Goal", speed: "Speed", no_goals: "No goals", left: "(left)",
    d_points: "Pts", d_g: "G", d_a: "A", d_s: "S", d_sh: "Sh", d_demo: "Demo",
    who_unknown: "Couldn't tell which player is you: set your account in Settings.",
    banner_title: "{label} in {pl}", banner_before: "MMR before the match: {mmr}. The new one is read automatically when you queue again in {pl}; if you're done for now, type it here.",
    banner_none: "Type the MMR shown after the match to track the change.",
    finished: "Finished", l_points: "Points", l_goals: "Goals", l_assists: "Assists", l_saves: "Saves", l_shots: "Shots", l_demos: "Demos", l_touches: "Touches",
    games_n: "{n} matches",
    rank_need: "<b>{n}</b> MMR to {name}", rank_est: "Estimate: the exact MMR arrives at your next queue",
    wins_one: "≈1 win", wins_many: "≈{n} wins",
    streak_win_one: "win in a row", streak_win_many: "wins in a row", streak_loss_one: "loss in a row", streak_loss_many: "losses in a row",
    no_session: "no matches in this session", wins: "wins", losses: "losses", mmr_session: "session MMR", pc_unreachable: "PC not reachable",
    o_w: "{n}W", o_l: "{n}L", o_streak_w: "{n}W in a row", o_streak_l: "{n}L in a row",
    // Required by the Epic Games Fan Content Policy (official wording).
    disclaimer: "Portions of the materials used are trademarks and/or copyrighted works of Epic Games, Inc. All rights reserved by Epic. This material is not official and is not endorsed by Epic.",
    footer_local: "Local Rocket Stats reads only data on this computer: nothing is sent online.",
  },
  it: {
    conn_game: "Gioco connesso", conn_match: "In partita", conn_off: "Gioco non connesso", conn_app: "App non raggiungibile",
    export_csv: "Esporta CSV", settings: "Impostazioni", save: "Salva", skip: "Salta", cancel: "Annulla",
    period: "Periodo", p_session: "Ultima sessione", p_today: "Oggi", p_7: "7 giorni", p_30: "30 giorni", p_all: "Tutto",
    playlist: "Playlist", all: "Tutte", session_of: "sessione del {date}",
    mmr_trend: "Andamento MMR", matches: "Partite", date: "Data", result: "Esito", points: "Punti",
    gasp_title: "Gol / Assist / Parate / Tiri", gasp: "G/A/P/T",
    empty: "Nessuna partita registrata. Avvia Rocket League con la Stats API abilitata (menu della tray, oppure <code>local-rocket-stats setup</code>) e gioca: le partite compariranno qui.",
    accounts: "I tuoi account (PrimaryId, uno per riga)", session_gap: "Pausa che separa le sessioni (minuti)",
    language: "Lingua", lang_auto: "Sistema",
    t_matches: "Partite", t_matches_sub: "{w}V · {l}S", t_abandoned: " · {n} abbandonate",
    t_winrate: "Vittorie", t_streak: "Serie attuale", t_mmr: "MMR {pl}", t_period: "nel periodo", t_period_est: "nel periodo (stima)",
    t_goals: "Gol / partita", t_assists: "Assist / partita", t_saves: "Parate / partita", t_shooting: "Precisione tiro",
    t_score: "Punteggio medio", total: "{n} totali", epic: "{n} epiche", shots_of: "{g}/{s} tiri", mvp: "{n} MVP",
    streak_w: "{n}V", streak_l: "{n}S",
    chart_none: "Gioca una partita classificata per vedere l'andamento.", chart_few: "Servono almeno 2 partite con MMR per disegnare il grafico.",
    win: "Vittoria", loss: "Sconfitta", abandoned: "Partita abbandonata",
    res_W: "V", res_L: "S", res_A: "AB", "res_?": "?",
    mmr_auto: "Rilevato automaticamente dal log del gioco",
    mmr_est: "Stima: il valore esatto arriva alla prossima coda in questa playlist, oppure scrivilo tu",
    estimate: "Stima", delete_match: "Elimina partita", confirm_delete: "Eliminare questa partita dallo storico?",
    count: "{n} nel periodo · {total} totali",
    player: "Giocatore", time: "Tempo", goal: "Gol", speed: "Velocità", no_goals: "Nessun gol", left: "(uscito)",
    d_points: "Punti", d_g: "G", d_a: "A", d_s: "P", d_sh: "T", d_demo: "Demo",
    who_unknown: "Non sono riuscito a capire quale giocatore sei: imposta il tuo account nelle Impostazioni.",
    banner_title: "{label} in {pl}", banner_before: "MMR prima della partita: {mmr}. Quello nuovo viene rilevato da solo quando rientri in coda in {pl}; se hai finito, inseriscilo qui.",
    banner_none: "Inserisci l'MMR mostrato a fine partita per tracciare il delta.",
    finished: "Finita", l_points: "Punti", l_goals: "Gol", l_assists: "Assist", l_saves: "Parate", l_shots: "Tiri", l_demos: "Demo", l_touches: "Tocchi",
    games_n: "{n} partite",
    rank_need: "<b>{n}</b> MMR a {name}", rank_est: "Stima: l'MMR esatto arriva alla prossima coda",
    wins_one: "≈1 vittoria", wins_many: "≈{n} vittorie",
    streak_win_one: "vittoria di fila", streak_win_many: "vittorie di fila", streak_loss_one: "sconfitta di fila", streak_loss_many: "sconfitte di fila",
    no_session: "nessuna partita nella sessione", wins: "vittorie", losses: "sconfitte", mmr_session: "MMR sessione", pc_unreachable: "PC non raggiungibile",
    o_w: "{n}V", o_l: "{n}S", o_streak_w: "{n}V di fila", o_streak_l: "{n}S di fila",
    disclaimer: "Alcuni dei materiali utilizzati sono marchi commerciali e/od opere protette dal diritto d'autore di Epic Games, Inc. Tutti i diritti di Epic sono riservati. Questi materiali non sono ufficiali e non sono approvati da Epic.",
    footer_local: "Local Rocket Stats legge solo dati presenti su questo computer: nulla viene inviato online.",
  },
};

let current = "en";

export function setLang(lang) {
  current = STRINGS[lang] ? lang : "en";
  document.documentElement.lang = current;
}

export const lang = () => current;
export const locale = () => (current === "it" ? "it-IT" : "en-GB");

/// Translated text; `{name}` placeholders are filled from `vars`.
export function t(key, vars = {}) {
  const s = STRINGS[current][key] ?? STRINGS.en[key] ?? key;
  return s.replace(/\{(\w+)\}/g, (_, k) => vars[k] ?? "");
}

/// Translates the static HTML: data-i18n (text or HTML), data-i18n-title, data-i18n-placeholder.
export function applyI18n(root = document) {
  for (const el of root.querySelectorAll("[data-i18n]")) el.innerHTML = t(el.dataset.i18n);
  for (const el of root.querySelectorAll("[data-i18n-title]")) el.title = t(el.dataset.i18nTitle);
  for (const el of root.querySelectorAll("[data-i18n-placeholder]")) el.placeholder = t(el.dataset.i18nPlaceholder);
}
