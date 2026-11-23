#!/bin/bash
# Voir la version Windows du Monteur (dist/…/Monteur VerdierCAM.exe) sous Wine, comme outils/voir-fenetre.sh voit celle de
# Linux : un KWin virtuel à part, sa propre session D-Bus, le son du vrai poste. Wine travaille dans un HOME jetable
# (DOSSIER/maison) : ni ~/.wine, ni le menu du bureau, ni les vraies vidéos ne sont touchés.
#
#   systemd-run --user --scope -q -E XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR -p MemoryMax=6G -p MemorySwapMax=0 \
#     dbus-run-session outils/voir-fenetre-windows.sh DOSSIER 'Z:\chemin\des\prises' "ouvrir 0; attendre 8; quitter" a:10
#
# DOSSIER doit être court : le socket de KWin y vit, et son chemin est limité à 108 octets. Les prises se donnent en
# chemin Windows (Z: est la racine de Linux). MONTEUR2_EXE remplace le programme essayé. Wine ne sert qu'à dégrossir :
# le vrai essai se fait sur un Windows (docs/windows.md).
set -u
D=$1; PRISES=$2; SC=$3; shift 3
ICI=$(cd "$(dirname "$0")/.." && pwd)
EXE=${MONTEUR2_EXE:-$(ls -d "$ICI"/dist/Monteur-VerdierCAM-*-windows-x64/"Monteur VerdierCAM.exe" 2>/dev/null | tail -1)}
[ -f "$EXE" ] || { echo "Pas de « Monteur VerdierCAM.exe » : outils/construire-windows.sh d'abord." >&2; exit 1; }
W="$D/maison"
mkdir -p "$D/run" "$W"; chmod 700 "$D/run"
XDG_RUNTIME_DIR="$D/run" kwin_wayland --virtual --no-lockscreen --no-global-shortcuts --socket=t \
    --width 1640 --height 1060 >"$D/kwin.log" 2>&1 &
K=$!
python3 -c "import time;time.sleep(3)"
# Les services que D-Bus lance (le portail et ses dialogues) s'affichent dans ce KWin, pas sur le vrai bureau.
dbus-update-activation-environment WAYLAND_DISPLAY="$D/run/t" DISPLAY= 2>/dev/null
# Sans DISPLAY, Wine prend son pilote Wayland. mscoree et mshtml coupés : pas de demande de Mono ni de Gecko ;
# winemenubuilder coupé : rien dans le menu du bureau.
# Sous Windows le Monteur dessine en DirectX 12 (main.rs) ; Wine, lui, a toujours été essayé en Vulkan : WGPU_BACKEND.
AVEC_WINE=(env -u DISPLAY WAYLAND_DISPLAY="$D/run/t" HOME="$W" XDG_CONFIG_HOME="$W/.config" XDG_DATA_HOME="$W/.local/share"
      WINEPREFIX="$W/.wine" WINEDEBUG=-all WINEDLLOVERRIDES="mscoree=d;mshtml=d;winemenubuilder.exe=d"
      WGPU_BACKEND="${WGPU_BACKEND:-vulkan}")
"${AVEC_WINE[@]}" MONTEUR2_SCENARIO="$SC" wine "$EXE" "$PRISES" >"$D/app.log" 2>&1 &
A=$!
for s in "$@"; do
    python3 -c "import time;time.sleep(${s#*:})"
    XDG_RUNTIME_DIR="$D/run" WAYLAND_DISPLAY=t spectacle -b -n -f -o "$D/${s%:*}.png" >"$D/spectacle.log" 2>&1
done
wait $A
"${AVEC_WINE[@]}" wineserver -k 2>/dev/null
kill $K
