# La version Windows (22/11/2026, lot 2 de la portabilité)

Le Monteur se construit pour Windows ici, sous Linux, comme VerdierCAM. Ce qui sort :
`dist/Monteur2-<version>-windows-x64/` et son `.zip` (80 Mo ; 203 Mo déballé).

| Fichier | Rôle |
|---|---|
| `monteur2.exe` (20 Mo) | le Monteur ; application graphique, sans console ; libc++ et libunwind liées dedans |
| `ffmpeg.exe`, `ffprobe.exe` | appelés par le Monteur, trouvés à côté de lui (`noyau::systeme::commande`) |
| 7 DLL de FFmpeg 9.0.2 | avcodec (122 Mo), avfilter (38), avformat (20), avdevice, avutil, swresample, swscale ; elles ne citent que Windows |
| `licences/`, `LISEZMOI.txt` | FFmpeg (GPL, avec x264), polices (OFL) |

## Mettre la chaîne en place

Tout vit dans `~/Applications/chaine-windows-monteur` (`MONTEUR_CHAINE_WINDOWS`), hors du dépôt. Le Rust
d'Arch reste celui par défaut ; `~/.cargo` et le PATH ne bougent pas.

1. **llvm-mingw**, le compilateur de VerdierCAM : déjà dans `~/Applications/chaine-windows/llvm-mingw`
   (`VCAM_CHAINE_WINDOWS`). Rien de la chaîne de VerdierCAM n'est modifié.
2. **Rust officiel 1.100.0 et la cible `x86_64-pc-windows-gnullvm`** (celui d'Arch n'a pas de cible Windows, et
   ses bibliothèques ne se mêlent pas aux officielles) :

   ```bash
   M=~/Applications/chaine-windows-monteur
   # rustup-init depuis https://static.rust-lang.org/rustup/dist/x86_64-unknown-linux-gnu/, empreinte .sha256 vérifiée
   RUSTUP_HOME=$M/rustup CARGO_HOME=$M/cargo ./rustup-init -y --no-modify-path --profile minimal \
       --default-toolchain 1.100.0 --target x86_64-pc-windows-gnullvm
   ```
3. **FFmpeg partagé de BtbN**, la version Windows que ffmpeg.org propose, en 7 DLL autonomes (celui de MSYS2
   en demande environ 200) : `ffmpeg-n9.0.2-61-gd59078794d-win64-gpl-shared-9.0.zip` de la publication
   `autobuild-2026-11-22-12-36` de https://github.com/BtbN/FFmpeg-Builds, empreinte vérifiée contre son
   `checksums.sha256`, déballé dans `$M/ffmpeg-extrait/`, et `$M/ffmpeg` pointe dessus.

## Construire

```bash
outils/construire-windows.sh
```

Construit dans `target/windows/`, assemble le dossier en suivant les DLL citées de proche en proche, vérifie
que chaque symbole importé est exporté (`outils/verifier-dll-windows.py`, repris de VerdierCAM), puis zippe.
Première construction : 33 s.

## Dégrossir sous Wine

```bash
systemd-run --user --scope -q -E XDG_RUNTIME_DIR=$XDG_RUNTIME_DIR -p MemoryMax=6G -p MemorySwapMax=0 \
  dbus-run-session outils/voir-fenetre-windows.sh /tmp/court 'Z:\chemin\des\prises' "ouvrir 0; attendre 8; quitter" a:10
```

Wine 11 dans un HOME jetable, la fenêtre dans un KWin virtuel. Vu le 22/11/2026 avec le paquet : la fenêtre
s'ouvre (60 i/s), la prise se lit par les DLL, la bande d'images et les miniatures s'affichent, Préparer
aboutit, le rendu sort une vidéo H.264 1280 × 720 avec son son, le chemin s'affiche en entier (`Z:\…`).

**FFmpeg 9.0 et 9.1** : avec `-skip_frame nokey`, le 9.0.2 de Windows ne rend rien d'une vidéo qui n'a qu'une
image clé, là où le 9.1 de Linux rend une image. La bande d'images se redemande alors sur toutes les images.

## Essayer sur un vrai Windows

Wine ne remplace pas Windows (pilote graphique, WASAPI, corbeille, dialogue de fichiers). La VM `win-test`
réserve 16 Go, n'a ni lecteur CD ni agent invité : le zip y entre par le réseau ou une clé USB. Une fois déballé
où l'on veut, double-cliquer `monteur2.exe`, puis : ouvrir une prise OBS, Préparer, poser un zoom, écrire la
vignette, Voix › écouter avant/après, Rendre, mettre une prise à la corbeille, Ouvrir le dossier.

**Pas encore sous Windows** : Filmer (gpu-screen-recorder, lot 3), les sous-titres et l'envoi (whisper-cli et
youtube-envoi, lot 4), le nanoKONTROL2 et le raccourci global. Le `.exe` n'a pas encore d'icône et n'est pas
signé : SmartScreen avertira au premier lancement d'un zip téléchargé (VerdierCAM signe les siens).
