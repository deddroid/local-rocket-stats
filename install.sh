#!/usr/bin/env bash
# User install in ~/.local (no root): binary, icons and app-menu entry.
#   ./install.sh               build (if needed) and install
#   ./install.sh --uninstall   remove everything (your match data is kept)
set -euo pipefail

app=local-rocket-stats
here="$(cd "$(dirname "$0")" && pwd)"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
bin="$HOME/.local/bin/$app"
desktop="$data/applications/$app.desktop"
icons="$data/icons/hicolor"
sizes=(16 32 48 64 128 256 512)

if [[ "${1:-}" == "--uninstall" ]]; then
    rm -fv "$bin" "$desktop" "$icons/scalable/apps/$app.svg"
    for s in "${sizes[@]}"; do rm -fv "$icons/${s}x${s}/apps/$app.png"; done
    rm -fv "${XDG_CONFIG_HOME:-$HOME/.config}/autostart/$app.desktop"
    echo "Match data kept in $data/$app"
    exit 0
fi

# From a release archive the binary is next to this script; from the sources, build it.
if [[ -x "$here/$app" ]]; then
    src="$here/$app"
    assets="$here/icons"
    desk="$here/$app.desktop"
else
    (cd "$here" && cargo build --release --locked)
    src="$here/target/release/$app"
    assets="$here/assets"
    desk="$here/packaging/linux/$app.desktop"
fi

install -Dm755 "$src" "$bin"
install -Dm644 "$desk" "$desktop"
sed -i "s|^Exec=$app|Exec=$bin|" "$desktop"
install -Dm644 "$assets/icon.svg" "$icons/scalable/apps/$app.svg"
for s in "${sizes[@]}"; do
    install -Dm644 "$assets/icon-$s.png" "$icons/${s}x${s}/apps/$app.png"
done
command -v update-desktop-database >/dev/null && update-desktop-database -q "$data/applications" || true
echo "Installed $bin"
echo "Find \"Local Rocket Stats\" in the app menu, or run: $app"
