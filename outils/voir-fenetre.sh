#!/bin/bash
# Voir le Monteur 2 sans toucher au bureau de Christophe : un KWin virtuel à part, sa propre session
# D-Bus, le son du vrai poste. Tout s'écrit dans un dossier jetable.
#
#   systemd-run --user --scope -q -E XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR -p MemoryMax=4G -p MemorySwapMax=0 \
#     dbus-run-session outils/voir-fenetre.sh DOSSIER PRISES "ouvrir 0; attendre 8; quitter" a:4 b:3
#
# DOSSIER reçoit a.png, b.png ; chaque `nom:délai` capture après ce délai, le précédent compris.
# PRISES : un dossier de prises jetable. Écran 1640 × 1060 ; spectacle lance la capture.
set -u
D=$1; PRISES=$2; SC=$3; shift 3
ICI=$(cd "$(dirname "$0")/.." && pwd)
mkdir -p "$D/run" "$D/donnees/monteur" "$D/projets"; chmod 700 "$D/run"
XDG_RUNTIME_DIR="$D/run" kwin_wayland --virtual --no-lockscreen --no-global-shortcuts --socket=t \
    --width 1640 --height 1060 >"$D/kwin.log" 2>&1 &
K=$!
python3 -c "import time;time.sleep(3)"
# Les services que D-Bus lance (le portail et ses dialogues) s'affichent dans ce KWin : sinon ils
# héritent du vrai écran, et le dialogue de fichiers du Monteur s'ouvrait sur le bureau (22/11/2026).
dbus-update-activation-environment WAYLAND_DISPLAY="$D/run/t" DISPLAY= 2>/dev/null
WAYLAND_DISPLAY="$D/run/t" DISPLAY= XDG_DATA_HOME="$D/donnees" MONTEUR2_PROJETS="$D/projets" MONTEUR2_SCENARIO="$SC" \
    "$ICI/target/release/monteur2" "$PRISES" >"$D/app.log" 2>&1 &
A=$!
for s in "$@"; do
    python3 -c "import time;time.sleep(${s#*:})"
    XDG_RUNTIME_DIR="$D/run" WAYLAND_DISPLAY=t spectacle -b -n -f -o "$D/${s%:*}.png" >"$D/spectacle.log" 2>&1
done
wait $A
kill $K
