# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and the project uses
[Semantic Versioning](https://semver.org/).

## [0.1.0] - 2026-10-22

First release.

### Added

- Match tracking from the official Rocket League **Stats API** (local TCP socket): result,
  score, overtime, your stats (score, goals, assists, saves, shots, demos, touches), every
  player's stats, goal log and statfeed events. Matches left early are recorded as such.
- **MMR from the game log** (`Launch.log`): the MMR read when you join a queue fills in the
  final MMR of your previous match in that playlist; rank tier included. Typed values are
  never overwritten. Estimated MMR (≈) while the exact value is not known yet.
- **Ranked matches recovered from the log** when the tracker was not running (score, arena,
  overtime, points, MMR before/after and result).
- **Rank and division** cards with progress and the MMR (and estimated wins) to the next
  division and rank, for 1v1, 2v2 and 3v3.
- Web dashboard: sessions, streaks, win rate, averages, MMR chart, match table with details,
  CSV export. **Phone page** (read-only from the local network) and **OBS overlay**.
- **Tray icon** on Linux (StatusNotifierItem) and Windows: status, open dashboard, phone
  access, enable the game's Stats API, start at login, language, quit.
- English and Italian, following the system language or chosen in the settings.
- `setup` command to enable the Stats API (`PacketSendRate`), also under Proton/Wine.
- `simulate` command: a fake game to try the app without Rocket League.
- Migration of the database of the first (Python) prototype.

[0.1.0]: https://github.com/deddroid/local-rocket-stats/releases/tag/v0.1.0
