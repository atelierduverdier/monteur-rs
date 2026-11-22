# Mode d'emploi du Monteur VerdierCAM

Le Monteur VerdierCAM filme VerdierCAM pendant que tu t'en sers, puis prépare tout seul le montage d'après tes
gestes : il zoome sur ce que tu cliques, écrit le nom des réglages que tu touches, coupe les temps morts et
sous-titre ta voix. Tu relis, tu corriges, tu écris la vignette et tu rends la vidéo, prête pour YouTube.

Il est gratuit et libre (licence GPL). Il marche sous Windows 10 et 11, et sous Linux.

## Installer

**Windows** : déballe le zip où tu veux, puis double-clique sur « Monteur VerdierCAM.exe ». Le dossier se déplace
en entier : tout ce qu'il contient sert au Monteur (ffmpeg pour l'image et le son, whisper pour les sous-titres).
Au premier lancement, Windows peut afficher « Windows a protégé votre ordinateur » : « Informations
complémentaires », puis « Exécuter quand même ».

**Linux** : rends le fichier exécutable, puis lance-le :

```bash
chmod +x Monteur-VerdierCAM-*-linux-x86_64.AppImage
./Monteur-VerdierCAM-*-linux-x86_64.AppImage
```

Il marche sur toute distribution de 2022 ou après (Ubuntu 22.04, Debian 12, Mint 21…). Pour filmer, il lui faut
**gpu-screen-recorder** (paquet de ta distribution, ou Flatpak) : il passe par le dialogue de ton bureau pour
choisir la fenêtre, le même qu'OBS.

## Au premier lancement

La boîte **Ce qu'il faut** dit ce que le Monteur a trouvé : de quoi lire et rendre les vidéos, filmer, sous-titrer,
et où est VerdierCAM. Ce qui manque y a son bouton :

- **Modèle des sous-titres** : « Télécharger… » apporte le modèle de Whisper (575 Mo, une seule fois). Les
  sous-titres se font ensuite sur ton ordinateur, rien n'est envoyé nulle part.
- **VerdierCAM** : s'il n'est pas trouvé tout seul, « Choisir… » et montre-lui le programme.

On la rouvre par Aide › Ce qu'il faut.

## Filmer

Trois boutons dans le ruban :

- **Filmer VerdierCAM** (le meilleur) : le Monteur lance VerdierCAM et filme sa fenêtre. VerdierCAM note alors
  chacun de tes gestes (clics, réglages, boîtes ouvertes) : c'est d'eux que naît le montage.
- **Filmer** : une fenêtre de ton choix ; le Monteur garde ce choix pour la fois suivante (Fichier › Filmer une
  autre fenêtre pour en changer).
- **Enregistrement simple** : une vidéo vite faite, sans décompte ni préparation, toujours dans Copies d'écran.

Trois bips annoncent la première image : ils ne sont pas filmés. Pendant la prise, le vu-mètre montre ton micro
(« Micro muet ? » s'il ne reçoit rien). Pour arrêter : le bouton **Arrêter**, ou **Windows+F12** (Meta+F12 sous
Linux) depuis n'importe quelle fenêtre. Le son de l'ordinateur et ta voix sont enregistrés, ta voix des deux côtés
même si ton micro n'en donne qu'un.

La prise arrive dans la liste, s'ouvre, et **Préparer le montage** se lance tout seul.

## Les prises et les projets

À gauche, la liste des prises du dossier : miniature, date, durée, et leur état (montée, vignette, envoyée). Un
clic ouvre une prise ; clic droit : la mettre à la corbeille (jamais de suppression définitive).

Les prises arrivent dans **Vidéos › Copies d'écran**. Un **projet** (Fichier › Nouveau projet) est un dossier dans
**Vidéos › Projets** : la prise y part avec tout ce qui l'accompagne (montage, vignette, rendus, vidéos insérées).
Le sélecteur en haut de la liste passe d'un dossier à l'autre ; « Ouvrir le dossier » le montre.

## Préparer le montage

Montage › Préparer le montage (ou le gros bouton orange) lit la prise et le journal des gestes de VerdierCAM :

- un **zoom** sur chaque endroit où tu travailles, une **étiquette** au nom de chaque réglage touché ;
- les **temps morts** (silence et image immobile) coupés ;
- les **sous-titres** de ta voix (Montage › Langue des sous-titres pour une autre langue que le français).

Tout est à relire : rien n'est figé, chaque élément se change dans la ligne de temps.

## Monter

- **La visionneuse** montre l'image montée de l'instant, zooms et étiquettes compris. Un zoom choisi : son cadre
  orange se glisse, se retaille par un coin ; tracer à côté en fait un autre.
- **La ligne de temps** : chapitres, vidéos, zooms, étiquettes, clics, sous-titres, voix, coupes. Un clic dans la
  règle place la tête ; un élément se glisse, s'allonge par un bord ; double-clic dans une piste vide : un élément de
  2 s. Molette : défiler ; Ctrl+molette : resserrer.
- **L'inspecteur** règle l'élément choisi (début, fin, grossissement, texte…), sinon le montage entier : montrer les
  clics, les étiquettes, les sous-titres (incrustés dans l'image, ou à part en .srt), le fondu aux coupes, le carton
  « Fait avec VerdierCAM » à la fin.
- **Le panneau Texte** montre tout ce que tu dis : un clic va à la phrase, « Couper » la retire du montage.
- **Les vidéos insérées** (Intercaler, Par-dessus) : une autre vidéo qui coupe la prise, ou qui passe par-dessus
  pendant que ta voix continue.

Les touches :

| Touche | Geste |
|---|---|
| Espace | lire, mettre en pause |
| K | arrêter (revient d'où la lecture est partie) |
| J, L | 5 s en arrière, en avant |
| ←, → | une image (Maj : une seconde) |
| Début, Fin | début, fin de la prise |
| C, puis C | une coupe, de la première à la seconde fois (ou clic droit, ou Entrée) |
| Z | un zoom à la tête |
| S | couper en deux l'élément choisi |
| M | un chapitre |
| Suppr, Échap | supprimer, désélectionner |
| Ctrl+Z, Ctrl+Maj+Z | annuler, rétablir |
| Ctrl+S | enregistrer le projet (le montage s'enregistre aussi tout seul) |

## Nettoyer la voix

Le bouton **Voix** : coupe du grave, bruit de fond, sifflantes, compresseur, puis niveau égal (comme YouTube
l'attend). « Écouter avant » et « Écouter après » font entendre dix secondes autour de la tête. Les réglages valent
pour toutes les prises.

## La vignette

L'onglet **Vignette**, à côté de la visionneuse : clique dans la ligne de temps pour choisir l'image, glisse-la et
resserre-la à la molette, écris le titre, le sous-titre et trois lignes. Ce qui mord sur l'image s'affiche en rouge.
Dans **Chaîne**, mets le nom et le logo (PNG) de ta chaîne à la place de ceux de VerdierCAM : ils valent pour toutes
tes vignettes. « Écrire la vignette » la pose à côté de la vidéo (PNG et JPG) ; elle passera 3 s en tête au rendu.

## Rendre

Le bouton **Rendre** : choisis un préréglage (YouTube, YouTube 1440p, Court vertical, Brouillon, Extrait GIF) et
mets-le dans la file ; tu peux continuer à monter pendant ce temps. Le rendu YouTube donne la vidéo montée, puis,
si la vignette est écrite, la **vidéo avec vignette** : 3 s de vignette en tête, et le carton « Fait avec
VerdierCAM » 2 s à la fin (décoche-le dans l'inspecteur si tu n'en veux pas).

## YouTube

L'onglet **YouTube** : titre, description, mots-clés. « Proposer d'après la vignette » les écrit pour toi ;
« Insérer les chapitres » ajoute les chapitres aux bons temps. Ce que YouTube refuserait s'affiche dessous.

Puis : « Copier le titre », « Copier la description », « Copier les mots-clés », « Ouvrir YouTube Studio », et
glisse-y la vidéo avec vignette (« Montrer la vidéo » ouvre son dossier). Dans Studio, mets la vignette (le fichier
JPG à côté de la vidéo) dans « Miniature ».

## Où sont les fichiers

- Les prises : Vidéos › Copies d'écran ; les projets : Vidéos › Projets.
- À côté de chaque vidéo : son montage (`… montage.json`), sa vignette, ses rendus, sa description.
- Les réglages du Monteur : `AppData\Roaming\monteur` sous Windows, `~/.local/share/monteur` sous Linux.

## Licence

Le Monteur VerdierCAM est un logiciel libre, sous licence GPL version 3 ou suivante : son code est sur
https://github.com/atelierduverdier/monteur-rs. Le nom et le logo VerdierCAM restent la marque de l'Atelier du
Verdier. Il s'appuie sur FFmpeg, whisper.cpp, Dear ImGui, wgpu et cpal ; leurs licences sont livrées avec lui.
