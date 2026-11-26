#!/usr/bin/env bash
# DANS LE CONTENEUR monteur-construction-linux (outils/construire-linux.sh le lance) : whisper-cli, le Monteur, puis
# l'AppImage. Montés : /depot (le dépôt), /chaine (FFmpeg Linux de BtbN), /sources (les sources de whisper.cpp),
# /tmp/cargo/registry et /tmp/cargo/git (les crates du poste, et DeepFilterNet pris à son dépôt : rien à retélécharger).
set -euo pipefail
DEPOT=/depot
CONSTRUCTION="$DEPOT/target/linux"
FFMPEG=/chaine/ffmpeg
WHISPER_SRC="$(ls -d /sources/whisper.cpp-* | tail -1)"
export CARGO_HOME=/tmp/cargo
mkdir -p "$CONSTRUCTION"
cd "$DEPOT"

# 1. whisper-cli : bibliothèques partagées, chaque variante du processeur chargée au lancement (de SSE4.2 à AVX-512),
# sans OpenMP (libgomp n'aurait pas à voyager).
# Sa configuration écrit dans ses sources : on en travaille une copie.
if [ ! -x "$CONSTRUCTION/whisper/bin/whisper-cli" ]; then
    rm -rf "$CONSTRUCTION/whisper-src" && cp -r "$WHISPER_SRC" "$CONSTRUCTION/whisper-src"
    cmake -S "$CONSTRUCTION/whisper-src" -B "$CONSTRUCTION/whisper-construction" -G Ninja -DCMAKE_BUILD_TYPE=Release \
        -DCMAKE_INSTALL_PREFIX="$CONSTRUCTION/whisper" -DCMAKE_INSTALL_RPATH='$ORIGIN' \
        -DBUILD_SHARED_LIBS=ON -DGGML_NATIVE=OFF -DGGML_BACKEND_DL=ON -DGGML_CPU_ALL_VARIANTS=ON -DGGML_OPENMP=OFF \
        -DWHISPER_BUILD_TESTS=OFF -DWHISPER_BUILD_SERVER=OFF -DWHISPER_SDL2=OFF -DWHISPER_CURL=OFF > /dev/null
    cmake --build "$CONSTRUCTION/whisper-construction" > /dev/null
    cmake --install "$CONSTRUCTION/whisper-construction" > /dev/null
fi

# 2. Le Monteur, contre le FFmpeg qui voyage avec lui.
FFMPEG_DIR="$FFMPEG" cargo build --release --locked --offline -p appli --target-dir "$CONSTRUCTION/cargo"

# 3. L'AppDir : le Monteur, ffmpeg, ffprobe et whisper-cli dans usr/bin ; FFmpeg dans usr/lib ; whisper et ggml à côté
# de whisper-cli (ggml cherche ses variantes du processeur dans le dossier du programme).
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$DEPOT/crates/appli/Cargo.toml" | head -1)"
NOM="Monteur-VerdierCAM-$VERSION-linux-x86_64"
APPDIR="$CONSTRUCTION/AppDir"
rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin" "$APPDIR/usr/lib" "$APPDIR/usr/share/monteur-verdiercam/licences"
cp "$CONSTRUCTION/cargo/release/monteur2" "$FFMPEG/bin/ffmpeg" "$FFMPEG/bin/ffprobe" "$APPDIR/usr/bin/"
cp -P "$FFMPEG"/lib/lib{avcodec,avdevice,avfilter,avformat,avutil,swresample,swscale}.so.* "$APPDIR/usr/lib/"
W="$CONSTRUCTION/whisper"
cp "$W/bin/whisper-cli" "$APPDIR/usr/bin/"
cp -P "$W"/lib/libwhisper.so* "$W"/lib/libggml*.so* "$APPDIR/usr/bin/" 2>/dev/null || true
cp -P "$W"/bin/libggml*.so* "$APPDIR/usr/bin/" 2>/dev/null || true
patchelf --set-rpath '$ORIGIN/../lib' "$APPDIR/usr/bin/monteur2" "$APPDIR/usr/bin/ffmpeg" "$APPDIR/usr/bin/ffprobe"
patchelf --set-rpath '$ORIGIN' "$APPDIR/usr/bin/whisper-cli" "$APPDIR"/usr/bin/lib*.so*
for f in "$APPDIR"/usr/lib/*.so.*; do [ -L "$f" ] || patchelf --set-rpath '$ORIGIN' "$f"; done

# Rien ne doit manquer, hors des bibliothèques de tout bureau Linux (glibc, libstdc++, ALSA…).
if ldd "$APPDIR"/usr/bin/monteur2 "$APPDIR"/usr/bin/ffmpeg "$APPDIR"/usr/bin/whisper-cli | grep -q "not found"; then
    ldd "$APPDIR"/usr/bin/* | grep "not found" >&2
    echo "AppImage refusée : des bibliothèques manquent." >&2
    exit 1
fi
glibc="$(for f in "$APPDIR"/usr/bin/* "$APPDIR"/usr/lib/*.so*; do objdump -T "$f" 2>/dev/null; done | grep -o 'GLIBC_[0-9.]*' | sort -Vu | tail -1)"
echo "glibc demandée au plus : $glibc"

# Licences, mode d'emploi, icône, lanceur.
cp "$DEPOT/LICENSE" "$APPDIR/usr/share/monteur-verdiercam/licences/Monteur-VerdierCAM-GPL-3.0.txt"
cp "$DEPOT/MARQUE.md" "$APPDIR/usr/share/monteur-verdiercam/licences/Marque-VerdierCAM.txt"
cp "$FFMPEG/LICENSE.txt" "$APPDIR/usr/share/monteur-verdiercam/licences/FFmpeg.txt"
cp "$WHISPER_SRC/LICENSE" "$APPDIR/usr/share/monteur-verdiercam/licences/whisper.cpp.txt"
cp "$DEPOT/ressources/DeepFilterNet-MIT.txt" "$APPDIR/usr/share/monteur-verdiercam/licences/DeepFilterNet.txt"
cp "$DEPOT/ressources/polices/OFL-Inter.txt" "$DEPOT/ressources/polices/OFL-JetBrainsMono.txt" "$APPDIR/usr/share/monteur-verdiercam/licences/"
[ -f "$DEPOT/target/mode-emploi.html" ] && cp "$DEPOT/target/mode-emploi.html" "$APPDIR/usr/share/monteur-verdiercam/Mode d'emploi.html"
cp "$DEPOT/ressources/fr.atelierduverdier.Monteur2.svg" "$APPDIR/fr.atelierduverdier.Monteur2.svg"
sed -e 's|@EXEC@|monteur2|' "$DEPOT/outils/fr.atelierduverdier.Monteur2.desktop" > "$APPDIR/fr.atelierduverdier.Monteur2.desktop"
cat > "$APPDIR/AppRun" <<'LANCEUR'
#!/bin/sh
# Le Monteur trouve ffmpeg, ffprobe et whisper-cli à côté de lui (noyau::systeme) ; le PATH les donne aussi aux autres.
ICI="$(dirname "$(readlink -f "$0")")"
export PATH="$ICI/usr/bin:$PATH"
exec "$ICI/usr/bin/monteur2" "$@"
LANCEUR
chmod +x "$APPDIR/AppRun"

mkdir -p "$DEPOT/dist"
rm -f "$DEPOT/dist/$NOM.AppImage"
APPIMAGE_EXTRACT_AND_RUN=1 ARCH=x86_64 appimagetool --no-appstream "$APPDIR" "$DEPOT/dist/$NOM.AppImage" > "$CONSTRUCTION/appimagetool.log" 2>&1 \
    || { cat "$CONSTRUCTION/appimagetool.log" >&2; echo "appimagetool a échoué" >&2; exit 1; }
(cd "$DEPOT/dist" && sha256sum "$NOM.AppImage" > "$NOM.AppImage.sha256")
echo "dist/$NOM.AppImage ($(du -h "$DEPOT/dist/$NOM.AppImage" | cut -f1))"
