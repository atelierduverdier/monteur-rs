#!/usr/bin/env bash
# LA VERSION WINDOWS DU MONTEUR, CONSTRUITE ICI SOUS LINUX (22/11/2026, lot 2 de la portabilité).
#
#   outils/construire-windows.sh      construit, puis assemble dist/Monteur-VerdierCAM-<version>-windows-x64/ et son .zip
#
# La chaîne vit hors du dépôt, dans $MONTEUR_CHAINE_WINDOWS (défaut : ~/Applications/chaine-windows-monteur) :
#   rustup/  Rust officiel avec la cible x86_64-pc-windows-gnullvm (celui d'Arch n'a pas de cible Windows, et ses
#            bibliothèques ne se mêlent pas aux officielles) ;
#   ffmpeg/  FFmpeg partagé de BtbN : en-têtes, bibliothèques d'import, 7 DLL, ffmpeg.exe et ffprobe.exe ;
#   whisper/ whisper-cli.exe et ses DLL, compilés depuis les sources de whisper.cpp (docs/windows.md) : facultatif,
#            sans lui le paquet n'a pas de sous-titres.
# Le compilateur C/C++ (pour Dear ImGui) et l'éditeur de liens sont ceux de VerdierCAM : llvm-mingw, dans
# $VCAM_CHAINE_WINDOWS/llvm-mingw (défaut : ~/Applications/chaine-windows). docs/windows.md dit comment tout poser.
# Ce qui sort : target/windows/ (la construction) et dist/, tous deux ignorés par git.
set -euo pipefail

DEPOT="$(cd "$(dirname "$0")/.." && pwd)"
CHAINE="${MONTEUR_CHAINE_WINDOWS:-$HOME/Applications/chaine-windows-monteur}"
LLVM="${VCAM_CHAINE_WINDOWS:-$HOME/Applications/chaine-windows}/llvm-mingw"
RUST=1.100.0
CIBLE=x86_64-pc-windows-gnullvm
OUTILS_RUST="$CHAINE/rustup/toolchains/$RUST-x86_64-unknown-linux-gnu"
FFMPEG="$CHAINE/ffmpeg"
OBJDUMP="$LLVM/bin/llvm-objdump"

manque() { echo "$1" >&2; echo "Voir docs/windows.md, « Mettre la chaîne en place »." >&2; exit 1; }
[ -x "$LLVM/bin/x86_64-w64-mingw32-clang++" ] || manque "Pas de compilateur llvm-mingw dans $LLVM."
[ -d "$OUTILS_RUST/lib/rustlib/$CIBLE" ] || manque "Pas de Rust $RUST avec la cible $CIBLE dans $CHAINE/rustup."
[ -f "$FFMPEG/lib/libavcodec.dll.a" ] && [ -f "$FFMPEG/bin/ffmpeg.exe" ] || manque "Pas de FFmpeg Windows dans $FFMPEG."

# 1. La construction. Le Rust officiel passe devant celui d'Arch, seulement ici ; le registre des crates (~/.cargo)
# reste partagé. Bindgen lit les en-têtes de FFmpeg comme le compilateur de Windows les lirait.
export PATH="$OUTILS_RUST/bin:$LLVM/bin:$PATH"
export FFMPEG_DIR="$FFMPEG"
export CC_x86_64_pc_windows_gnullvm=x86_64-w64-mingw32-clang
export CXX_x86_64_pc_windows_gnullvm=x86_64-w64-mingw32-clang++
export AR_x86_64_pc_windows_gnullvm=llvm-ar
export CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_LINKER=x86_64-w64-mingw32-clang
export BINDGEN_EXTRA_CLANG_ARGS_x86_64_pc_windows_gnullvm="--target=x86_64-w64-mingw32 --sysroot=$LLVM/x86_64-w64-mingw32 -isystem $LLVM/generic-w64-mingw32/include"
# L'icône et la fiche de l'exécutable (ressources/monteur.rc), compilées par windres et liées au seul programme.
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$DEPOT/crates/appli/Cargo.toml" | head -1)"
IFS=. read -r V1 V2 V3 <<< "$VERSION"
mkdir -p "$DEPOT/target/windows"
RC="$DEPOT/target/windows/monteur.rc"
sed -e "s/@V1@/$V1/g; s/@V2@/$V2/g; s/@V3@/$V3/g; s/@VERSION@/$VERSION/g" "$DEPOT/ressources/monteur.rc" > "$RC"
x86_64-w64-mingw32-windres -O coff -I "$DEPOT/ressources" -o "$DEPOT/target/windows/monteur-ressources.o" "$RC"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNULLVM_RUSTFLAGS="-C link-arg=$DEPOT/target/windows/monteur-ressources.o"
cargo build --release --locked --target "$CIBLE" -p appli --target-dir "$DEPOT/target/windows"
EXE="$DEPOT/target/windows/$CIBLE/release/monteur2.exe"

# 2. Le dossier : le Monteur, ffmpeg.exe et ffprobe.exe (le Monteur les trouve à côté de lui, noyau::systeme), et les DLL
# qu'ils citent, de proche en proche. Une DLL qui n'est ni dans FFmpeg ni dans llvm-mingw est une DLL du système
# (KERNEL32, l'UCRT…) : elle ne voyage pas.
NOM="Monteur-VerdierCAM-$VERSION-windows-x64"
SORTIE="$DEPOT/dist/$NOM"
rm -rf "$SORTIE" "$DEPOT/dist/$NOM.zip"
mkdir -p "$SORTIE/licences"
cp "$EXE" "$SORTIE/Monteur VerdierCAM.exe"
cp "$FFMPEG/bin/ffmpeg.exe" "$FFMPEG/bin/ffprobe.exe" "$SORTIE/"
# whisper-cli et ses variantes du processeur : ggml les charge au lancement, aucune table d'importation ne les cite.
WHISPER="$CHAINE/whisper/bin"
if [ -f "$WHISPER/whisper-cli.exe" ]; then
    cp "$WHISPER/whisper-cli.exe" "$WHISPER/libwhisper.dll" "$WHISPER"/ggml*.dll "$SORTIE/"
else
    echo "Pas de whisper-cli.exe dans $WHISPER : le paquet n'aura pas de sous-titres." >&2
fi

cherche() {
    local d
    for d in "$FFMPEG/bin" "$WHISPER" "$LLVM/x86_64-w64-mingw32/bin"; do
        [ -f "$d/$1" ] && { echo "$d/$1"; return 0; }
    done
    return 1
}
declare -A vues=()
mapfile -t a_voir < <(find "$SORTIE" -maxdepth 1 \( -iname '*.exe' -o -iname '*.dll' \))
while [ ${#a_voir[@]} -gt 0 ]; do
    courant="${a_voir[0]}"; a_voir=("${a_voir[@]:1}")
    while read -r dll; do
        cle="${dll,,}"
        [ -n "${vues[$cle]:-}" ] && continue
        vues[$cle]=1
        if chemin="$(cherche "$dll")"; then
            cp "$chemin" "$SORTIE/"
            a_voir+=("$SORTIE/$dll")
        fi
    done < <("$OBJDUMP" -p "$courant" | sed -n 's/^\s*DLL Name: //p')
done

# 3. Chaque symbole importé est exporté par la DLL qui voyage avec lui (la leçon de VerdierCAM, 02/10/2026).
python3 "$DEPOT/outils/verifier-dll-windows.py" "$OBJDUMP" "$SORTIE" \
    || { echo "Paquet refusé : des symboles importés n'existent pas dans les DLL du paquet." >&2; exit 1; }

# 4. Les licences : le Monteur (GPL, et la marque VerdierCAM à part), FFmpeg (GPL, avec x264), whisper.cpp (MIT),
# llvm-mingw (libc++), les polices (OFL).
cp "$DEPOT/LICENSE" "$SORTIE/licences/Monteur-VerdierCAM-GPL-3.0.txt"
cp "$DEPOT/MARQUE.md" "$SORTIE/licences/Marque-VerdierCAM.txt"
cp "$FFMPEG/LICENSE.txt" "$SORTIE/licences/FFmpeg.txt"
[ -f "$SORTIE/whisper-cli.exe" ] && cp "$(ls -d "$CHAINE"/sources/whisper.cpp-*/LICENSE | tail -1)" "$SORTIE/licences/whisper.cpp.txt"
[ -f "$SORTIE/libc++.dll" ] && cp "$LLVM/LICENSE.TXT" "$SORTIE/licences/llvm-mingw.txt"
cp "$DEPOT/ressources/polices/OFL-Inter.txt" "$DEPOT/ressources/polices/OFL-JetBrainsMono.txt" "$SORTIE/licences/"
{
    echo "Monteur VerdierCAM $VERSION pour Windows : double-cliquer sur « Monteur VerdierCAM.exe »."
    echo
    echo "Tout ce qui est dans ce dossier sert au Monteur : ffmpeg.exe et ffprobe.exe pour l'image et le son,"
    echo "whisper-cli.exe pour les sous-titres. Le dossier se déplace en entier, n'importe où."
    echo
    echo "Logiciel libre, sous licence GPL version 3 ou suivante (licences/). Code source :"
    echo "https://github.com/atelierduverdier/monteur-rs"
    echo "FFmpeg vient de https://github.com/BtbN/FFmpeg-Builds ($(basename "$(readlink -f "$FFMPEG")")),"
    echo "whisper-cli de whisper.cpp, compilé depuis ses sources."
} > "$SORTIE/LISEZMOI.txt"

# Le mode d'emploi, en une page : Aide › Mode d'emploi l'ouvre, sans réseau.
python3 "$DEPOT/outils/mode-emploi-html.py" "$DEPOT/docs/mode-emploi.md" "$SORTIE/Mode d'emploi.html"

# 5. Le zip et son empreinte.
(cd "$DEPOT/dist" && zip -qr "$NOM.zip" "$NOM" && sha256sum "$NOM.zip" > "$NOM.zip.sha256")
echo "$(find "$SORTIE" -maxdepth 1 -iname '*.dll' | wc -l) DLL, $(du -sh "$SORTIE" | cut -f1) — dist/$NOM.zip ($(du -h "$DEPOT/dist/$NOM.zip" | cut -f1))"
