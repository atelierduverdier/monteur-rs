#!/usr/bin/env bash
# LA VERSION LINUX À DONNER (22/11/2026). Construit ici, le Monteur exige la glibc de CachyOS et le FFmpeg 9 d'Arch :
# il ne démarrerait ni sur Ubuntu 24.04 ni sur Mint. Il se construit donc dans un conteneur Ubuntu 22.04
# (outils/linux/Dockerfile, bâti sur celui de VerdierCAM) et sort en un seul fichier :
#
#   outils/construire-linux.sh     → dist/Monteur-VerdierCAM-<version>-linux-x86_64.AppImage, FFmpeg et whisper dedans
#
# La chaîne vit hors du dépôt : ~/Applications/chaine-linux-monteur/ffmpeg (FFmpeg partagé de BtbN, glibc 2.28) et
# les sources de whisper.cpp dans ~/Applications/chaine-windows-monteur/sources (les mêmes que pour Windows).
set -euo pipefail

DEPOT="$(cd "$(dirname "$0")/.." && pwd)"
IMAGE=monteur-construction-linux
CHAINE="${MONTEUR_CHAINE_LINUX:-$HOME/Applications/chaine-linux-monteur}"
SOURCES="${MONTEUR_CHAINE_WINDOWS:-$HOME/Applications/chaine-windows-monteur}/sources"

[ -f "$CHAINE/ffmpeg/lib/libavcodec.so" ] || { echo "Pas de FFmpeg Linux dans $CHAINE/ffmpeg (docs/linux.md)." >&2; exit 1; }
ls -d "$SOURCES"/whisper.cpp-* > /dev/null 2>&1 || { echo "Pas de sources de whisper.cpp dans $SOURCES (docs/windows.md)." >&2; exit 1; }
if ! docker image inspect "$IMAGE" > /dev/null 2>&1; then
    echo "L'image $IMAGE n'existe pas encore : construction (elle part de verdiercam-construction-linux)…"
    docker build -t "$IMAGE" "$DEPOT/outils/linux"
fi

# Le mode d'emploi en HTML, fait ici (python-markdown), emporté par l'assemblage.
python3 "$DEPOT/outils/mode-emploi-html.py" "$DEPOT/docs/mode-emploi.md" "$DEPOT/target/mode-emploi.html" 2>/dev/null || true

docker run --rm --user "$(id -u):$(id -g)" -e HOME=/tmp \
    -v "$DEPOT:/depot" -v "$CHAINE:/chaine:ro" -v "$SOURCES:/sources:ro" \
    -v "$(readlink -f "$CHAINE/ffmpeg"):$(readlink -f "$CHAINE/ffmpeg"):ro" \
    -v "$HOME/.cargo/registry:/tmp/cargo/registry" \
    "$IMAGE" bash /depot/outils/linux/assembler.sh
