#!/usr/bin/env bash
# LA VERSION WINDOWS DU MONTEUR, CONSTRUITE ICI SOUS LINUX (22/11/2026, lot 2 de la portabilité).
#
#   outils/construire-windows.sh      construit, puis assemble dist/Monteur2-<version>-windows-x64/ et son .zip
#
# La chaîne vit hors du dépôt, dans $MONTEUR_CHAINE_WINDOWS (défaut : ~/Applications/chaine-windows-monteur) :
#   rustup/  Rust officiel avec la cible x86_64-pc-windows-gnullvm (celui d'Arch n'a pas de cible Windows, et ses
#            bibliothèques ne se mêlent pas aux officielles) ;
#   ffmpeg/  FFmpeg partagé de BtbN : en-têtes, bibliothèques d'import, 7 DLL, ffmpeg.exe et ffprobe.exe.
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
cargo build --release --locked --target "$CIBLE" -p appli --target-dir "$DEPOT/target/windows"
EXE="$DEPOT/target/windows/$CIBLE/release/monteur2.exe"

# 2. Le dossier : le Monteur, ffmpeg.exe et ffprobe.exe (le Monteur les trouve à côté de lui, noyau::systeme), et les DLL
# qu'ils citent, de proche en proche. Une DLL qui n'est ni dans FFmpeg ni dans llvm-mingw est une DLL du système
# (KERNEL32, l'UCRT…) : elle ne voyage pas.
VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' "$DEPOT/crates/appli/Cargo.toml" | head -1)"
NOM="Monteur2-$VERSION-windows-x64"
SORTIE="$DEPOT/dist/$NOM"
rm -rf "$SORTIE" "$DEPOT/dist/$NOM.zip"
mkdir -p "$SORTIE/licences"
cp "$EXE" "$FFMPEG/bin/ffmpeg.exe" "$FFMPEG/bin/ffprobe.exe" "$SORTIE/"

cherche() {
    local d
    for d in "$FFMPEG/bin" "$LLVM/x86_64-w64-mingw32/bin"; do
        [ -f "$d/$1" ] && { echo "$d/$1"; return 0; }
    done
    return 1
}
declare -A vues=()
a_voir=("$SORTIE/monteur2.exe" "$SORTIE/ffmpeg.exe" "$SORTIE/ffprobe.exe")
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

# 4. Les licences de ce qui voyage : FFmpeg (GPL, avec x264), les polices (OFL, dans le programme).
cp "$FFMPEG/LICENSE.txt" "$SORTIE/licences/FFmpeg.txt"
cp "$DEPOT/ressources/polices/OFL-Inter.txt" "$DEPOT/ressources/polices/OFL-JetBrainsMono.txt" "$SORTIE/licences/"
{
    echo "Monteur 2 $VERSION pour Windows : double-cliquer sur monteur2.exe."
    echo
    echo "ffmpeg.exe et ffprobe.exe servent au Monteur : ils restent dans le même dossier que lui."
    echo "FFmpeg vient de https://github.com/BtbN/FFmpeg-Builds ($(basename "$(readlink -f "$FFMPEG")"))."
} > "$SORTIE/LISEZMOI.txt"

# 5. Le zip et son empreinte.
(cd "$DEPOT/dist" && zip -qr "$NOM.zip" "$NOM" && sha256sum "$NOM.zip" > "$NOM.zip.sha256")
echo "$(find "$SORTIE" -maxdepth 1 -iname '*.dll' | wc -l) DLL, $(du -sh "$SORTIE" | cut -f1) — dist/$NOM.zip ($(du -h "$DEPOT/dist/$NOM.zip" | cut -f1))"
