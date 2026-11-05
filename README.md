# Monteur 2

Le Monteur des vidéos VerdierCAM réécrit en Rust, avec une interface Dear ImGui (panneaux
ancrables) à la charte de VerdierCAM. Le Monteur PySide6 (`~/Projets/logiciels/monteur`) reste en
service jusqu'à ce que celui-ci fasse une vidéo du jour entière.

- Plan : `~/Projets/logiciels/monteur/docs/plan-monteur-rust.md`
- Maquette : https://claude.ai/artifact/C1NuBZgz9sp1WUtKfbTQNo
- Étape 0, essais de faisabilité et leurs mesures : [docs/etape-0.md](docs/etape-0.md)

## Lancer

```bash
cargo build --release
target/release/monteur2
```

Voir [docs/etape-3.md](docs/etape-3.md) pour ce que la fenêtre sait faire.

## Construire

```bash
cargo build --release
```

Le groupe `dear-imgui-*` / `dear-app` est épinglé dans `Cargo.toml` : il change de version d'un
bloc. FFmpeg 9 et ses en-têtes viennent du paquet `ffmpeg` du système.

## Dossiers

- `crates/charte` : couleurs de VerdierCAM posées sur le style d'ImGui.
- `crates/noyau` : fichiers, préparation du montage, sous-titres ; contrôlé contre le Monteur 1 ([docs/etape-1.md](docs/etape-1.md)).
- `crates/composeur` : l'image montée d'un instant et la vignette ; contrôlé contre le Monteur 1 ([docs/etape-2.md](docs/etape-2.md)).
- `crates/media` : la lecture (image, son, horloge du son, sauts).
- `crates/rendu` : la vidéo finale ([docs/etape-4.md](docs/etape-4.md)).
- `crates/appli` : la fenêtre (`monteur2`), avec whisper et l'envoi YouTube ([docs/etape-5.md](docs/etape-5.md)).
- `essais/` : les essais de l'étape 0 (fenêtre, vidéo, son), chacun avec ses mesures.
- `ressources/polices` : Inter et JetBrains Mono (licence OFL).
