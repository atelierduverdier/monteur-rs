# Étape 0 — essais de faisabilité (04/11/2026)

Trois petits programmes dans `essais/`, mesurés sur ce poste (« atomman », Radeon RX 7600M XT,
Wayland KDE, Rust 1.99, FFmpeg 9.0.2), sur la prise OBS `2026-11-02 10-06-06.mp4`
(H.264 1920 × 1200 à 30 i/s, AAC 48 kHz, 94,6 s). La prise n'est que lue (empreinte md5 vérifiée
avant et après).

## Essai 1 — fenêtre Dear ImGui à panneaux ancrables

`ESSAI_DUREE=8 target/release/essai-fenetre`

- `dear-app` 0.22 (winit + wgpu, docking) : compile du premier coup, tourne sous Wayland.
- Disposition de la maquette (prises, visionneuse, inspecteur, ligne de temps à 8 pistes),
  polices Inter et JetBrains Mono, charte VerdierCAM (`crates/charte`), accents justes.
- **60,4 images/s** (calé sur l'écran), pire écart entre deux images 19,1 ms.
- Une image 1920 × 1080 renvoyée à la carte à chaque tour : **0,82 ms** (`write_texture`).

Reste à régler : la texture vidéo doit être en `Rgba8UnormSrgb` (en `Rgba8Unorm`, la mire sort
trop sombre sur la surface sRGB).

## Essai 2 — décodage de la vidéo

`target/release/essai-video <prise> 20` : 601 images, puis 10 sauts à des instants fixes.

| | Processeur (fils par image) | Carte graphique (VA-API) |
|---|---|---|
| images/s, jusqu'au RGBA | **1 819** | 158 |
| décodage | 0,08 ms | 0,21 ms |
| rapatriement en mémoire | — | 1,97 ms |
| conversion en RGBA | 0,46 ms | 4,15 ms (NV12) |
| saut, moyenne / pire | **53 / 89 ms** | 101 / 213 ms |

**Verdict** : une copie d'écran se décode presque pour rien au processeur. La carte graphique ne
gagne rien à la lecture : l'image doit revenir en mémoire, et cela coûte 12 fois plus. On décode
donc au processeur, et le code `unsafe` VA-API n'est pas nécessaire pour lire. (Il a marché :
il servira peut-être à l'encodage, à mesurer à l'étape 4.)

Le pire saut (89 ms) vient des images clés espacées par OBS (jusqu'à 245 images, 8 s) : glisser
la tête de lecture reste fluide à 10 images/s. Pour mieux, réglage d'OBS (une image clé toutes les
2 s) ou proxy.

## Essai 3 — son par `cpal`

`target/release/essai-son <prise> 20` (silence ; `--entendre` pour l'écouter)

- Hôte ALSA (par PipeWire), 48 kHz, flottants, tampons de 512 images (10,7 ms).
- Tout le son de la prise décodé et rééchantillonné en **70 ms** (1 344 fois le temps réel).
- Retard d'un tampon au haut-parleur : **21,3 ms**, constant.
- Fréquence réelle de la sortie : 47 993,5 et 47 987,2 images/s pour 48 000 annoncées
  (**−136 et −266 millionièmes**, deux essais de 20 s ; écart d'un appel à la droite ≤ 7,6 ms).

**Verdict** : sur une prise de 10 min, suivre l'horloge du système décalerait la voix de 80 à
160 ms. L'image suivra donc l'horloge du son (images livrées par `cpal`), avancée de 21 ms.

## Conséquences sur le plan

1. La pile tient : `dear-app` + `wgpu` + `ffmpeg-the-third` + `cpal`.
2. Décodage au processeur ; plus de risque VA-API pour la lecture.
3. L'horloge maîtresse est le son ; elle se mesure, elle ne se suppose pas.
4. Repli `easy-imgui` inutile.
