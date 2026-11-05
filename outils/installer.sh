#!/bin/bash
# Pose le lanceur du Monteur 2 dans ~/.local, sans sudo. Le portail de KDE en a besoin pour accorder
# le raccourci global. ./outils/installer.sh --retirer le défait.
set -eu
racine=$(cd "$(dirname "$0")/.." && pwd)
nom=fr.atelierduverdier.Monteur2.desktop
cible="$HOME/.local/share/applications/$nom"
icone="$HOME/.local/share/icons/hicolor/scalable/apps/fr.atelierduverdier.Monteur2.svg"
if [ "${1:-}" = "--retirer" ]; then
    rm -f "$cible" "$icone"
else
    sed "s|@EXEC@|\"$racine/target/release/monteur2\"|" "$racine/outils/$nom" > "$cible"
    mkdir -p "$(dirname "$icone")"
    cp "$racine/ressources/fr.atelierduverdier.Monteur2.svg" "$icone"
fi
update-desktop-database "$HOME/.local/share/applications" 2>/dev/null || true
gtk-update-icon-cache -q -t "$HOME/.local/share/icons/hicolor" 2>/dev/null || true
kbuildsycoca6 >/dev/null 2>&1 || true
