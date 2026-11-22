# La version Linux à donner (22/11/2026, lot 5)

Construit sur CachyOS, le Monteur exige la glibc 2.43 et le FFmpeg 9 d'Arch (`libavcodec.so.63`) : il ne
démarrerait ni sur Ubuntu 24.04 ni sur Mint. Il se construit donc dans un conteneur, comme VerdierCAM, et sort en un
seul fichier :

```bash
outils/construire-linux.sh
```

→ `dist/Monteur-VerdierCAM-<version>-linux-x86_64.AppImage` (89 Mo) et son `.sha256`.

## La chaîne

- **L'image Docker `monteur-construction-linux`** (`outils/linux/Dockerfile`) part de
  `verdiercam-construction-linux` (Ubuntu 22.04, glibc 2.35, gcc 12, cmake, patchelf, appimagetool) et n'y ajoute
  que clang (bindgen lit les en-têtes de FFmpeg) et Rust 1.100.0. Elle se construit seule la première fois.
- **FFmpeg partagé de BtbN**, le pendant du Windows : `ffmpeg-n9.0.2-61-gd59078794d-linux64-gpl-shared-9.0.tar.xz`
  (publication `autobuild-2026-11-22-12-36`, empreinte vérifiée), déballé dans
  `~/Applications/chaine-linux-monteur/ffmpeg-extrait/`, `ffmpeg` pointe dessus. Il ne demande que la glibc 2.28.
- **whisper.cpp 1.9.5** : les mêmes sources que pour Windows (`~/Applications/chaine-windows-monteur/sources`).
- Le registre des crates du poste est monté dans le conteneur : rien ne se retélécharge (`--offline`).

## Ce que fait l'assemblage (`outils/linux/assembler.sh`, dans le conteneur)

1. whisper-cli, en bibliothèques partagées, chaque variante du processeur chargée au lancement, sans OpenMP.
2. Le Monteur, contre le FFmpeg qui voyage avec lui.
3. L'AppDir : `usr/bin` (monteur2, ffmpeg, ffprobe, whisper-cli, et les bibliothèques de whisper et ggml à côté
   de lui : ggml y cherche ses variantes), `usr/lib` (les 7 bibliothèques de FFmpeg), rpath posés par patchelf ;
   licences et mode d'emploi dans `usr/share/monteur-verdiercam/` ; un `AppRun` qui met `usr/bin` dans le PATH.
4. Refus si une bibliothèque manque, hors de celles de tout bureau ; la glibc la plus récente demandée est dite
   (2.34 aujourd'hui).
5. appimagetool.

Ce qui vient du système, comme pour VerdierCAM : la glibc, libstdc++, ALSA (`libasound.so.2`), Vulkan, Wayland, X11.
Pour filmer : gpu-screen-recorder (paquet de la distribution ou Flatpak), qui passe par le portail du bureau.

## Essayé le 22/11/2026

- Dans le KWin virtuel du poste (`MONTEUR2_PROGRAMME=dist/….AppImage outils/voir-fenetre.sh …`) : la fenêtre
  s'ouvre, « Ce qu'il faut » trouve ffmpeg et whisper-cli dans l'AppImage.
- Dans des conteneurs Debian 12 et Ubuntu 24.04 nus : ffmpeg 9.0.2 et whisper-cli démarrent ; il ne manque au
  Monteur que `libasound.so.2`, présent sur tout bureau qui a du son.

**Pas essayé** : sur un vrai poste Ubuntu ou Mint, avec son écran et sa carte graphique.
