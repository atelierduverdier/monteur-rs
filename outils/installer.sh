#!/bin/bash
# Pose le lanceur du Monteur 2 dans ~/.local, sans sudo. Le portail de KDE en a besoin pour accorder
# le raccourci global. ./outils/installer.sh --retirer le défait.
set -eu
racine=$(cd "$(dirname "$0")/.." && pwd)
nom=fr.atelierduverdier.Monteur2.desktop
cible="$HOME/.local/share/applications/$nom"
if [ "${1:-}" = "--retirer" ]; then
    rm -f "$cible"
else
    sed "s|@EXEC@|\"$racine/target/release/monteur2\"|" "$racine/outils/$nom" > "$cible"
fi
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
