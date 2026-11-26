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
  sous-titres se font ensuite sur ton ordinateur, rien n'est envoyé nulle part. L'avancement s'affiche en bas de la
  fenêtre et dans cette boîte. Si tu fermes le Monteur pendant le téléchargement, il te le demande d'abord ; arrêté, le
  téléchargement reprendra où il en était la fois suivante.
- **VerdierCAM** : s'il n'est pas trouvé tout seul, « Choisir… » et montre-lui le programme.

On la rouvre par Aide › Ce qu'il faut.

Un seul Monteur s'ouvre à la fois. Le relancer alors qu'il l'est déjà, sur un autre bureau virtuel ou réduit, fait
venir sa fenêtre ; si le bureau ne le permet pas, une notification (une boîte sous Windows) dit qu'il est déjà ouvert.

## Filmer

Deux boutons dans le ruban :

- **Filmer VerdierCAM** (le meilleur) : le Monteur lance VerdierCAM et filme sa fenêtre. VerdierCAM note alors
  chacun de tes gestes (clics, réglages, boîtes ouvertes) : c'est d'eux que naissent les zooms et les étiquettes.
  Sa fenêtre est filmée sans rien demander : sous Linux, le dialogue de KDE ne s'ouvre que la toute première fois,
  pour la choisir. S'il est déjà ouvert, il n'est pas relancé et ton travail reste là : sa fenêtre est filmée telle quelle. Ses
  gestes ne sont alors notés que si c'est le Monteur qui l'avait lancé ; sinon le message te le dit, et pour les
  avoir il faut le fermer et repasser par ce bouton.
- **Filmer** : une fenêtre de ton choix, ou **tout l'écran** pour en montrer plusieurs. Le choix s'ouvre à chaque
  fois : sous Linux, le dialogue de KDE (l'écran, une zone ou une fenêtre) ; sous Windows, la boîte « Que filmer ? »,
  les écrans en tête, celui ou celle filmé la dernière fois déjà choisi : Entrée pour le reprendre. Les gestes de
  VerdierCAM ne sont notés que s'il a été lancé par le bouton d'à côté.

Et dans le menu Fichier, **Enregistrement simple** : une vidéo vite faite, sans décompte ni préparation, toujours
dans Copies d'écran.

Trois bips annoncent la première image : ils ne sont pas filmés. Pendant la prise, le vu-mètre montre ton micro
(« Micro muet ? » s'il ne reçoit rien). Pour arrêter : le bouton **Arrêter**, ou **Windows+F12** (Meta+F12 sous
Linux) depuis n'importe quelle fenêtre. Le son de l'ordinateur et ta voix sont enregistrés, ta voix des deux côtés
même si ton micro n'en donne qu'un.

La prise arrive dans la liste, s'ouvre, et **Préparer le montage** se lance tout seul.

## La webcam

Pour te montrer dans la vidéo : coche **Fichier › Filmer aussi la webcam** (s'il y a plusieurs caméras, **Quelle
webcam** choisit la bonne). Elle est alors filmée avec chaque prise (pas avec l'enregistrement simple) : « ● webcam »
s'allume à côté du vu-mètre. Elle s'enregistre à part, à côté de la prise (`… webcam.mkv`), et s'incruste au
montage, calée toute seule :

- **La vignette**, en bas à droite au départ. Dans la visionneuse, à l'arrêt : glisse-la pour la déplacer, tire son
  coin bas droit pour la retailler. L'inspecteur (rien de choisi dans la ligne de temps) la montre ou la cache, règle
  sa taille, la pose dans un coin, et retouche son calage.
- **En grand**, pour parler aux gens en cachant ton travail : **W** à la tête de lecture la fait grandir jusqu'à
  toute l'image pour 5 s ; **W** encore la ramène en vignette. Les passages se voient dans la piste **Webcam** de la
  ligne de temps : ils s'y glissent, s'allongent, et un glisser dans la piste en crée un. Tes sous-titres restent
  par-dessus.
- **En grand dès le tournage** : pendant la prise, **Meta+F11** (Windows+F11 sous Windows), le bouton S de la voie 5
  du nanoKONTROL2 ou un clic sur « ● webcam » la passent en grand, puis la ramènent. « ● webcam en grand » s'allume
  en orange, et la prise arrive avec ces passages déjà posés, à retoucher comme les autres. KDE fait confirmer
  Meta+F11 la première fois, comme Meta+F12. Une notification sans son (« Webcam en grand », « Webcam en vignette »)
  confirme chaque appui, puisque le Monteur est souvent caché pendant qu'on filme ; elle ne se voit pas dans une
  prise d'une seule fenêtre. Meta+F11 pressée hors d'une prise avec webcam le dit aussi.

Ce que tu vois est ce qui sort : le rendu incruste la webcam comme la visionneuse.

Sous Windows, une caméra branchée peut rester invisible si Windows en interdit l'accès : le micro de la caméra
marche, l'image non. Le Monteur le dit, dans **Aide › Ce qu'il faut** (ligne « Webcam ») et quand la webcam n'a rien
filmé, avec l'interrupteur à activer dans **Paramètres › Confidentialité et sécurité › Caméra**. Relance ensuite le
Monteur.

## Les prises et les projets

À gauche, la liste des prises du dossier : miniature, date, durée, et leur état (montée, vignette, envoyée). Un
clic ouvre une prise ; clic droit : la mettre à la corbeille (jamais de suppression définitive).

Pour en jeter **plusieurs d'un coup**, choisis-les comme dans un gestionnaire de fichiers : **Ctrl+clic** en ajoute
une (ou la retire), **Maj+clic** prend toutes celles entre la dernière cliquée et celle-ci (Ctrl+Maj+clic les
ajoute au choix). Ces clics n'ouvrent pas de prise. « N prises choisies » s'affiche au-dessus de la liste, avec
**Corbeille…** et **Aucune** (tout désélectionner) ; un clic droit sur l'une d'elles, ou Fichier › Mettre les N
prises choisies à la corbeille, fait la même chose. Une seule confirmation dit combien partent, lesquelles, avec
leurs fichiers et leur poids. Si la prise ouverte en fait partie, elle est d'abord enregistrée, puis fermée.
Un clic simple revient à une seule prise.

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
  zooms (décochée, l'image reste entière ; les zooms restent sur leur piste, prêts à revenir), les clics, les étiquettes, les sous-titres (incrustés dans l'image, ou à part en .srt), le fondu aux coupes, le carton
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
| W | la webcam en grand à la tête, ou de retour en vignette |
| Suppr, Échap | supprimer, désélectionner |
| Ctrl+Z, Ctrl+Maj+Z | annuler, rétablir |
| Ctrl+S | enregistrer le projet : la première fois, tu choisis son dossier et son nom (le titre de la vidéo est proposé), ensuite il s'écrit au même endroit ; le montage s'enregistre aussi tout seul |
| Ctrl+Maj+S | enregistrer le projet sous un autre nom, ou ailleurs |

## Nettoyer la voix

Le bouton **Voix** : bruit de fond, coupe du grave, sifflantes, compresseur, puis niveau égal (comme YouTube
l'attend). « Écouter avant » et « Écouter après » font entendre dix secondes autour de la tête. Les réglages valent
pour toutes les prises.

Le bruit de fond (ventilation, ronflement de l'atelier, souffle) est retiré par **DeepFilterNet**, un petit réseau
qui reconnaît la voix : entre les phrases, le bruit disparaît. Il est dans le Monteur, rien à télécharger, et
travaille sur le processeur : un peu plus d'une minute pour 40 min de prise, au début du rendu (la file
d'attente dit « débruitage de la voix… »). La **force** dit jusqu'où le bruit baisse : à 50 %, 33 dB ; à 100 %,
sans limite, comme l'essai du 26/11. À fond, la voix peut sonner « sous l'eau » : baisse la force jusqu'à ce
qu'elle retrouve son timbre, à l'oreille, avec « Écouter après ».

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
- À côté de chaque vidéo : son montage (`… montage.json`), sa webcam (`… webcam.mkv`), sa vignette, ses rendus, sa
  description.
- Les réglages du Monteur : `AppData\Roaming\monteur` sous Windows, `~/.local/share/monteur` sous Linux.

## Licence

Le Monteur VerdierCAM est un logiciel libre, sous licence GPL version 3 ou suivante : son code est sur
https://github.com/atelierduverdier/monteur-verdiercam. Le nom et le logo VerdierCAM restent la marque de l'Atelier du
Verdier. Il s'appuie sur FFmpeg, whisper.cpp, Dear ImGui, wgpu et cpal ; leurs licences sont livrées avec lui.
