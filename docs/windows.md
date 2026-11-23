# La version Windows (22/11/2026, lot 2 de la portabilité)

Le Monteur se construit pour Windows ici, sous Linux, comme VerdierCAM. Ce qui sort :
`dist/Monteur2-<version>-windows-x64/` et son `.zip` (90 Mo ; 230 Mo déballé).

| Fichier | Rôle |
|---|---|
| `monteur2.exe` (20 Mo) | le Monteur ; application graphique, sans console ; libc++ et libunwind liées dedans |
| `ffmpeg.exe`, `ffprobe.exe` | appelés par le Monteur, trouvés à côté de lui (`noyau::systeme::commande`) |
| 7 DLL de FFmpeg 9.0.2 | avcodec (122 Mo), avfilter (38), avformat (20), avdevice, avutil, swresample, swscale ; elles ne citent que Windows |
| `whisper-cli.exe`, `libwhisper.dll`, 15 DLL de ggml | les sous-titres ; ggml choisit au lancement la variante du processeur |
| `libc++.dll`, `libunwind.dll` | la bibliothèque C++ de llvm-mingw, pour whisper |
| `licences/`, `LISEZMOI.txt` | FFmpeg (GPL, avec x264), whisper.cpp (MIT), llvm-mingw, polices (OFL) |

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

4. **whisper-cli**, depuis les sources de whisper.cpp 1.9.5 (`codeload.github.com/ggml-org/whisper.cpp`, tag
   v1.9.5, déballées dans `$M/sources/`), llvm-mingw dans le PATH :

   ```bash
   cmake -S $M/sources/whisper.cpp-1.9.5 -B $M/whisper-construction -G Ninja -DCMAKE_BUILD_TYPE=Release \
     -DCMAKE_SYSTEM_NAME=Windows -DCMAKE_SYSTEM_PROCESSOR=x86_64 -DCMAKE_C_COMPILER=x86_64-w64-mingw32-clang \
     -DCMAKE_CXX_COMPILER=x86_64-w64-mingw32-clang++ -DCMAKE_RC_COMPILER=x86_64-w64-mingw32-windres \
     -DCMAKE_CXX_FLAGS="-include cstdlib -include algorithm" -DCMAKE_INSTALL_PREFIX=$M/whisper \
     -DBUILD_SHARED_LIBS=ON -DGGML_NATIVE=OFF -DGGML_BACKEND_DL=ON -DGGML_CPU_ALL_VARIANTS=ON -DGGML_OPENMP=OFF \
     -DWHISPER_BUILD_TESTS=OFF -DWHISPER_BUILD_SERVER=OFF -DWHISPER_SDL2=OFF -DWHISPER_CURL=OFF
   cmake --build $M/whisper-construction && cmake --install $M/whisper-construction
   ```

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
où l'on veut, double-cliquer « Monteur VerdierCAM.exe », puis : ouvrir une prise OBS, Préparer, poser un zoom, écrire
la vignette, Voix › écouter avant/après, Rendre, mettre une prise à la corbeille, Ouvrir le dossier.

**Pas sous Windows** : le nanoKONTROL2. Les sous-titres et YouTube : lot 4, dans [portabilite.md](portabilite.md).

## La signature (23/11/2026)

Sans signature, SmartScreen arrête le Monteur au premier lancement (« Windows a protégé votre ordinateur — Éditeur :
inconnu »), les antivirus le retiennent, et Smart App Control de Windows 11 refuse chaque DLL qui n'est ni signée ni
connue de Microsoft (« Image incorrecte », 0xc0e90002). VerdierCAM a payé les trois ; sa doc Windows
(`verdiercam-imgui/docs/windows.md`, « L'avertissement de SmartScreen », « Smart App Control ») en garde le détail.

`outils/construire-windows.sh` signe comme VerdierCAM, avec ses variables, pour qu'un seul certificat signe les deux :

```bash
VCAM_SIGNATURE_PFX=<certificat.pfx> VCAM_SIGNATURE_MOT_DE_PASSE_FICHIER=<fichier du mot de passe> outils/construire-windows.sh
```

Chaque `.exe` et chaque DLL du dossier (30 fichiers : le Monteur, ffmpeg, ffprobe, whisper-cli, FFmpeg, ggml,
libc++…), par `osslsigncode` (paquet AUR `osslsigncode`, absent du poste aujourd'hui), en SHA-256, au nom de « Monteur
VerdierCAM », horodaté chez Sectigo. Chaque fichier signé est relu (`osslsigncode verify`) : un seul qui ne se vérifie
pas, et le paquet est refusé. Sans certificat, le paquet sort non signé et le dit ; `LISEZMOI.txt` dit toujours quoi
cliquer devant SmartScreen, car un programme signé mais neuf peut encore être averti tant que sa signature n'a pas de
réputation. `VCAM_SIGNATURE_HORODATAGE=` (vide) signe sans horodatage, pour un essai hors réseau avec un certificat
jetable.

**Essayé le 23/11/2026**, sur le vrai paquet, avec un certificat fait pour l'occasion (« CN=Atelier du Verdier
Essai », RSA 3072, clé détruite aussitôt). L'outil était `osslsigncode` 2.2 d'Ubuntu 22.04, déballé à part ; la
`libcurl-gnutls` qu'il demande manque à Arch, une bibliothèque factice de neuf fonctions vides l'a remplacée, l'essai
étant sans horodatage. Résultats :

- les 30 fichiers sont signés, et chacun se vérifie une fois sorti du zip ;
- un octet changé dans le `.exe` signé, la vérification échoue ;
- quand la vérification échoue (certificat non reconnu), le script refuse le paquet dès la première DLL, sans zip ;
- quand une signature est demandée sans `osslsigncode`, le script refuse aussi ;
- le `.exe` signé ne diffère du construit que par sa somme de contrôle, l'entrée « Security » et la signature
  ajoutée au bout, l'icône intacte ;
- sous Wine, `ffprobe.exe` et `whisper-cli.exe` signés se lancent avec leurs DLL signées.

Le paquet de `dist/` a été reconstruit non signé ensuite. **Pas essayé** : l'horodatage chez Sectigo, et un Windows
devant un certificat reconnu.

**Le jour de l'achat, à adapter ici comme dans VerdierCAM.** Depuis juin 2023, aucune autorité ne remet plus le
certificat en `.pfx` : la clé vit sur un jeton USB ou dans le nuage de l'autorité. La branche `.pfx` ne sert donc
qu'avec un certificat d'essai ; un vrai certificat passe par le module PKCS#11 du jeton ou de l'autorité
(`osslsigncode sign -pkcs11module …`).

**Ce que le Monteur peut prendre et pas VerdierCAM** : le certificat « Open Source Code Signing » de Certum. Il ne
convient pas à VerdierCAM, qui est vendu, mais le Monteur est libre et gratuit. Il est délivré au nom du développeur
(« Open Source Developer, … »), vaut un an, et garde sa clé dans le nuage SimplySign, que l'on ouvre par une
application sur le téléphone. Le prix et les pièces demandées sont à revérifier chez Certum avant d'acheter. Ce
certificat ne signerait que le Monteur : pour les deux, il faut un certificat OV.

## Le Microsoft Store (23/11/2026)

La voie sans certificat à acheter, la même que pour VerdierCAM. L'inscription d'un particulier à l'Espace partenaires
est gratuite, et un paquet **MSIX** envoyé au Store y est **signé par Microsoft** après sa certification. SmartScreen et
Smart App Control le laissent alors passer.

```bash
outils/construire-windows.sh
```

```bash
python3 outils/construire-msix.py
```

→ `dist/Monteur-VerdierCAM-<version>-windows-x64.msix` (96 Mo, 43 fichiers, en 9 s), à côté du zip, qu'il ne
remplace pas. Le format est celui de VerdierCAM, repris tel quel (`outils/construire-msix.py`, écrit en Python
d'après le SDK MSIX de Microsoft). Le paquet est relu comme Windows le lira : CRC, empreinte de chaque bloc, chaque
bloc qui se décomprime seul, un type pour chaque fichier, chaque programme et chaque image que cite le manifeste. Un
paquet qui ne se relit pas est effacé.

- **L'identité** : le Monteur est un produit à part dans l'Espace partenaires, du même compte que VerdierCAM.
  `MONTEUR_MSIX_NOM` (son nom de paquet), `VCAM_MSIX_EDITEUR` et `VCAM_MSIX_EDITEUR_NOM` (les mêmes que pour
  VerdierCAM). Sans elles, c'est un paquet d'essai, d'éditeur « CN=Atelier du Verdier Essai », que le Store refuserait.
- **Le contenu** : tout le dossier du zip à la racine du paquet, sauf `LISEZMOI.txt`, qui ne parle que du zip.
  Quatre vignettes sont tirées de l'icône.
- **Les écritures ne sont pas détournées** (`desktop6:FileSystemWriteVirtualization` et `RegistryWriteVirtualization`
  à `disabled`, capacité `unvirtualizedResources`). Un paquet ordinaire range dans une copie à lui tout ce qu'il écrit
  dans AppData et la base de registre, et les programmes qu'il lance aussi. Or le Monteur lance VerdierCAM, qui écrit
  ses réglages, son essai (dans la base de registre) et le journal des gestes. Détournés, les réglages changés pendant
  une prise seraient restés dans le paquet du Monteur. Le Monteur du Store écrit donc où écrit celui du zip : il
  partage avec lui ses réglages et le modèle des sous-titres, et les laisse après la désinstallation, comme le zip.
- **La capture et le micro** : `graphicsCaptureProgrammatic` et `graphicsCaptureWithoutBorder` (uap11), et le micro
  (`DeviceCapability microphone`), sans lequel le Monteur n'apparaîtrait pas dans Paramètres › Confidentialité ›
  Microphone.
- **Windows 10 2004 au moins** (19041), en français seulement.
- **Valider le manifeste** : `VCAM_MSIX_SCHEMAS=~/Applications/chaine-windows-monteur/schemas-msix`. Ce sont les
  54 schémas de `github.com/microsoft/msix-packaging` (`resources/AppxPackaging/Manifest/Schema`), empreintes git
  vérifiées.
- **Signer un paquet à installer à la main** : `VCAM_MSIX_PFX` et `VCAM_MSIX_MOT_DE_PASSE_FICHIER`, avec
  osslsigncode 2.6 au moins. Non signé, Windows refuse de l'installer (0x80073D2B). `VCAM_SIGNATURE_HORODATAGE=`
  (vide) signe sans horodatage. Le paquet envoyé au Store part, lui, non signé.

**Essayé le 23/11/2026** :
- le manifeste passe les schémas de Microsoft ;
- trois sabotages sont refusés : une capacité inventée, le micro avant les capacités, une valeur hors liste ;
- `unzip -t` ne trouve aucune erreur ;
- une identité incomplète est refusée ;
- l'identité du Store est prise dans le manifeste ;
- saboté (blocs comprimés sans `Z_FULL_FLUSH`), le paquet est refusé, rien ne reste et le paquet d'avant est intact ;
- une signature demandée sans osslsigncode est refusée ;
- signé par osslsigncode 2.14 (le `.deb` d'Ubuntu, déballé à part puis effacé), le paquet se vérifie ; un octet
  changé, la vérification échoue.

**Pas essayé : sur un vrai Windows.** Le paquet signé d'essai attend dans `dist/essai-msix/`, avec son certificat
(« CN=Atelier du Verdier Essai », clé détruite, sans horodatage : la signature ne vaut que jusqu'au 23/12/2026). Refait
le 23/11/2026 avec la demande de capture sans cadre (ci-dessous), avec un certificat neuf.
Comme VerdierCAM le 29/10, sur le PC de l'atelier, dans la session (pas par SSH, où `Add-AppxPackage` échoue) :

1. dans un Terminal administrateur :
   `Import-Certificate -FilePath "Atelier du Verdier Essai.cer" -CertStoreLocation Cert:\LocalMachine\TrustedPeople` ;
2. `Add-AppxPackage -Path Monteur-VerdierCAM-0.1.0-windows-x64.msix` ; le Monteur arrive au menu Démarrer ;
3. à regarder :
   - Aide › Ce qu'il faut trouve ffmpeg et whisper-cli ;
   - Filmer : Windows demande-t-il le micro ? Demande-t-il, une seule fois, de filmer sans cadre ? Une fois l'accord
     donné, plus de cadre jaune autour de la fenêtre filmée ?
   - Filmer VerdierCAM (VerdierCAM choisi dans Ce qu'il faut) : changer un réglage de VerdierCAM pendant la prise, puis
     rouvrir VerdierCAM hors du Monteur, le réglage doit y être ; Préparer doit trouver les gestes ;
   - Télécharger le modèle ;
4. puis `Get-AppxPackage AtelierDuVerdier.MonteurVerdierCAM | Remove-AppxPackage`, et
   `Remove-Item Cert:\LocalMachine\TrustedPeople\7D68A038E8EA75D5EECF20E0BA6A66D45D1AD73D`.

**Ce qu'on sait déjà devoir regarder** :
- **Le cadre jaune** : un programme empaqueté n'a la capture sans cadre qu'après avoir demandé l'accord de
  l'utilisateur (`GraphicsCaptureAccess.RequestAccessAsync`, accès « Borderless »). Sinon Windows garde le cadre, sans
  erreur. `windows-capture` ne fait pas cette demande ; **le Monteur la fait depuis le 23/11/2026**
  (`capture_windows.rs`, `demander_sans_cadre`).
  - Quand : dès qu'on clique Filmer ou Filmer VerdierCAM.
  - Comment : dans un fil à part, pour que la fenêtre ne se fige pas pendant que Windows pose sa question. Windows la
    pose une fois, puis retient la réponse.
  - Le démarrage de la prise attend la réponse une demi-seconde au plus, puis filme. La toute première prise peut donc
    garder le cadre si la réponse arrive après la fin du décompte.
  - Hors paquet (le zip), ou sur un Windows 10 qui n'a pas cette demande (avant le build 20348), elle échoue ou revient
    accordée, et rien ne change. Peut-être pose-t-elle la question au zip aussi : à voir sur le PC.
  - Sous Wine (sans cette capture) : la boîte « Quelle fenêtre filmer ? » s'ouvre comme avant, et le déroulé d'essai va
    jusqu'au bout.
- **Le Monteur et le VerdierCAM du Store** : le Monteur ne trouve pas encore un VerdierCAM installé par le Store
  (il faudrait son alias `verdiercam.exe`). Et un VerdierCAM empaqueté écrirait le journal des gestes dans sa copie à
  lui d'AppData, où le Monteur ne le lit pas. À régler dans le manifeste de VerdierCAM le jour où les deux sont au
  Store : `unvirtualizedResources`, ou `$(KnownFolder:RoamingAppData)\monteur` exclu de la copie (Windows 11).

**Le jour de l'envoi** :
1. Dans l'Espace partenaires, créer le produit « Monteur VerdierCAM » (Nouveau produit › Application MSIX ou PWA), et
   relever son identité.
2. `MONTEUR_MSIX_NOM=… VCAM_MSIX_EDITEUR=… VCAM_MSIX_EDITEUR_NOM=… VCAM_MSIX_SCHEMAS=… python3 outils/construire-msix.py`.
3. Justifier les deux capacités restreintes :
   - `runFullTrust` : un programme de bureau qui lance ffmpeg et VerdierCAM ;
   - `unvirtualizedResources` : VerdierCAM, lancé par le Monteur, garde ses réglages et le journal des gestes là où
     il les écrit toujours.
4. Remplir la fiche :
   - gratuit ;
   - logiciel libre, avec le lien du code ;
   - une politique de confidentialité, sans doute demandée pour le micro (le Monteur n'envoie rien : la voix et les
     sous-titres restent sur l'ordinateur), à vérifier dans les règles du Store au moment de l'envoi.
5. La certification prend quelques jours, à chaque version.

## Filmer sous Windows (lot 3, 22/11/2026)

Sans gpu-screen-recorder ni OBS : `capture_windows.rs` filme une fenêtre par Windows.Graphics.Capture (crate
`windows-capture`), `enregistreur.rs` fait la prise. Ce que ça donne :

- **La fenêtre** se choisit dans la boîte « Quelle fenêtre filmer ? » (titre et programme). Filmer la garde
  (`fenetre-capture`, à côté des fiches) ; Fichier › Filmer une autre fenêtre l'oublie ; Enregistrement simple la
  redemande à chaque fois. **Filmer VerdierCAM** lance VerdierCAM et filme sa fenêtre (`VerdierCAM.exe`) dès qu'elle
  s'ouvre, sans rien demander ; 40 s sans fenêtre, il abandonne.
- **L'image** : celle de la fenêtre même quand une autre passe devant, avec le pointeur, sans le cadre jaune
  (Windows 11 ; Windows 10 le dessine). Recadrée sur la zone cliente, sans barre de titre : c'est là que VerdierCAM
  note ses gestes. 30 i/s fixes : la dernière image est reprise tant que la fenêtre ne bouge pas, et le nombre
  d'images suit l'horloge. Une fenêtre redimensionnée se pose en haut à gauche du cadre de la première image.
- **L'encodage** : ffmpeg, par son entrée standard ; NVENC, AMF ou Quick Sync si la carte graphique sait faire (une
  image d'essai le dit), x264 « veryfast » sinon.
- **Le son** : le bureau (WASAPI en « loopback », par cpal) et le micro, chacun dans un WAV avec l'instant de son
  premier échantillon. À l'arrêt, ffmpeg les cale sur la première image et les mêle, le micro des deux côtés.
- **Le décompte** se joue avant la première image : la fenêtre est déjà choisie. **Windows+F12** arrête
  (`RegisterHotKey`). Fermer le Monteur en filmant termine la prise.

Essayé ici : 6 contrôles d'`enregistreur.rs` sous Linux, dont une vraie prise de 2 s par une source factice
(60 images, nom d'OBS, rien qui traîne), un mélange réel (une piste stéréo, 2 s) et le calage (saboté : il
rougit). La boîte du choix s'affiche sous Wine. **La capture elle-même ne s'essaie que sur un vrai Windows**
(Wine n'a pas Windows.Graphics.Capture) : filmer VerdierCAM 30 s, vérifier que les zooms de Préparer tombent sur
les clics, que la voix est calée sur les lèvres, et que l'image est nette.
