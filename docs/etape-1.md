# Étape 1 — le noyau (04/11/2026, finie)

`crates/noyau`, sans interface ni écran : ce que `noyau.py` et `montage.py` du Monteur 1 font sans
Qt, porté à l'identique.

- **Fichiers** : `montage.json`, `.monteur`, `fiches.json`, `… description YouTube.txt`, journal des
  gestes de VerdierCAM. Même format que le Monteur 1 : les deux Monteurs travaillent sur les mêmes
  prises. Un champ inconnu (d'un Monteur plus récent) est gardé et réécrit ; un élément illisible
  est sauté, comme avant ; écriture atomique.
- **Préparer le montage** : calage du journal sur l'image, zooms, étiquettes et clics tirés des
  gestes, arrêt de ce qui montre un endroit quand l'écran change d'un coup, étiquettes calées sur
  la voix, temps morts (coupés ou accélérés ×4), sous-titres de whisper-cli recollés, `.srt` aux
  temps du montage. Les commandes `ffmpeg` / `whisper-cli` sont bâties ici, lancées par l'appli.
- **Les vidéos** : liste d'un dossier (la plus récente d'abord, le .mp4 préféré au .mkv du même
  nom, les montées dont l'original est parti), état (brute, vignette, montée, envoyée, ignorée),
  fichiers liés et rangement (rien n'écrasé, la fiche suit), ffprobe, cadre de la vignette,
  « 3 s de vignette puis la vidéo », envoi par `youtube-envoi` toujours en privé
  (`MONTEUR_YOUTUBE_ENVOI` désigne un faux dans les contrôles).

Pas porté exprès : les commandes ImageMagick de la vignette. La vignette se peindra avec le reste
de l'image (étape 2, `composeur`), contre le même test au pixel que la vignette MACHINE.

## Contrôle contre le Monteur 1

`outils/reference_python.py` fait calculer au Monteur 1 lui-même, sur de vrais fichiers
(`crates/noyau/tests/donnees/`), ce que le noyau doit retrouver ; `cargo test -p noyau` compare à
1e-9 près (18 contrôles). Il refait notamment la préparation complète de deux prises :

| Prise | Durée | Calage | Résultat retrouvé à l'identique |
|---|---|---|---|
| 2026-11-02 10-06-06 | 1 min 35 | −0,35 s | 10 zooms, 6 étiquettes, 32 clics |
| 2026-11-04 07-59-21 | 32 min 38 | −0,60 s | 125 zooms, 162 étiquettes, 356 clics, 74 coupes |

Le mouvement de l'image et les silences y sont mesurés par ffmpeg sur la vraie vidéo (lue
seulement, empreinte vérifiée) et gardés dans `reference.json` : le contrôle tourne sans elle.

Chaque règle a été sabotée une fois pour voir un contrôle tomber : rampe du zoom, seuil du
changement d'écran, regroupement des gestes, taille d'une boîte, étiquette lue, garde et
accélération des temps morts, glisser (distance et temps), longueur de ligne du `.srt` ; puis
préférence du .mp4, ordre des états, refus d'écraser au rangement, arrondi et hauteur du cadre,
envoi en privé, identifiant YouTube, fichiers liés. Le rangement se rejoue sur un dossier jetable
bâti comme celui du Monteur 1.

Les arrondis suivent ceux de Python (`round` au plus proche, égalités vers le pair ; longueurs en
caractères, pas en octets) : le Monteur 1 relit ce que celui-ci écrit.
