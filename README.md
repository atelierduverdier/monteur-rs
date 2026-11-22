# Monteur VerdierCAM

Le monteur vidéo donné avec [VerdierCAM](https://verdiercam.fr) : il filme VerdierCAM pendant qu'on s'en sert,
prépare tout seul le montage d'après les gestes notés par VerdierCAM (zooms sur les clics, nom des réglages, temps
morts coupés, sous-titres de la voix), puis rend la vidéo et sa vignette, prêtes pour YouTube. Linux et Windows.
Écrit en Rust, interface Dear ImGui à la charte de VerdierCAM.

Mode d'emploi : [docs/mode-emploi.md](docs/mode-emploi.md).

## Le donner

```bash
outils/construire-windows.sh
```

```bash
outils/construire-linux.sh
```

Le premier fait `dist/Monteur-VerdierCAM-<version>-windows-x64.zip` (compilation croisée, ici, sous Linux :
[docs/windows.md](docs/windows.md)) ; le second `dist/Monteur-VerdierCAM-<version>-linux-x86_64.AppImage`, dans un
conteneur Ubuntu 22.04 ([docs/linux.md](docs/linux.md)). Chacun emporte FFmpeg, whisper-cli, le mode d'emploi et
les licences.

## Lancer pour le développer

```bash
cargo build --release
target/release/monteur2
```

FFmpeg 9 et ses en-têtes viennent du paquet `ffmpeg` du système. Le groupe `dear-imgui-*` / `dear-app` est épinglé
dans `Cargo.toml` : il change de version d'un bloc ; `vendor/dear-app` y ajoute l'identité de la fenêtre (Wayland)
et son icône (Windows). Ce qui change d'un système à l'autre passe par `noyau::systeme`
([docs/portabilite.md](docs/portabilite.md)). `outils/voir-fenetre.sh` et `outils/voir-fenetre-windows.sh`
montrent la fenêtre dans un KWin virtuel, pilotée par un déroulé d'essai.

## Dossiers

- `crates/charte` : couleurs de VerdierCAM posées sur le style d'ImGui.
- `crates/noyau` : fichiers, préparation du montage, sous-titres, ce qui change d'un système à l'autre ; contrôlé contre le Monteur 1 ([docs/etape-1.md](docs/etape-1.md)).
- `crates/composeur` : l'image montée d'un instant, la vignette et le carton de fin ; contrôlé contre le Monteur 1 ([docs/etape-2.md](docs/etape-2.md)).
- `crates/media` : la lecture (image, son, horloge du son, sauts) et les petits sons (bips, écoute, micro).
- `crates/rendu` : la vidéo finale ([docs/etape-4.md](docs/etape-4.md)).
- `crates/appli` : la fenêtre (`monteur2`) : filmer ([docs/filmer.md](docs/filmer.md)), whisper et YouTube ([docs/etape-5.md](docs/etape-5.md)), l'aide.
- `essais/` : les essais de l'étape 0 (fenêtre, vidéo, son), chacun avec ses mesures.
- `ressources/` : l'icône, le logo, les polices Inter et JetBrains Mono (licence OFL).

## Histoire

Il remplace le Monteur PySide6 de l'atelier (le « Monteur 1 »), dont il relit et réécrit les fichiers au même
format ; les deux partagent un verrou tant que le premier reste en service. Étapes et mesures : `docs/etape-*.md`.
Maquette : https://claude.ai/artifact/C1NuBZgz9sp1WUtKfbTQNo

## Licence

GPL version 3 ou suivante ([LICENSE](LICENSE)). Le nom et le logo VerdierCAM n'en font pas partie
([MARQUE.md](MARQUE.md)).
