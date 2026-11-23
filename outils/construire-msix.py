#!/usr/bin/env python3
"""LE PAQUET DU MICROSOFT STORE DU MONTEUR (23/11/2026, Christophe : « Prépare le paquet MSIX pour le Store comme
VerdierCAM »).

Le script de VerdierCAM (verdiercam-imgui/outils/construire-msix.py, 29/10/2026), repris tel quel pour le format. Le
Microsoft Store est gratuit pour un particulier et SIGNE LUI-MÊME le paquet qu'on lui envoie, à condition que ce soit un
MSIX : ni SmartScreen ni Smart App Control n'arrêtent alors le Monteur, sans certificat à acheter (docs/windows.md,
« La signature » et « Le Microsoft Store »).

Ce script fait ce MSIX à partir du dossier que construire-windows.sh vient d'assembler : tout ce dossier à la racine du
paquet (le Monteur trouve ffmpeg, ffprobe et whisper-cli à côté de lui), sauf LISEZMOI.txt, qui ne parle que du zip ;
les vignettes tirées de l'icône (ressources/fr.atelierduverdier.Monteur2.svg, par rsvg-convert) ; le manifeste
(outils/windows/AppxManifest.xml.in). Le paquet lui-même (un zip ZIP64, AppxBlockMap.xml, [Content_Types].xml) est
écrit ici, d'après le SDK MSIX de Microsoft (github.com/microsoft/msix-packaging, src/msix/pack) : son outil makemsix
ne se construit pas sur ce poste sans y exécuter du code téléchargé.

    python3 outils/construire-msix.py [dist/Monteur-VerdierCAM-<v>-windows-x64]   # défaut : la version de crates/appli

L'identité vient de l'Espace partenaires : le Monteur y est un produit à part, du même compte que VerdierCAM (la fiche
du produit › Gestion des produits › Identité du produit) :
    MONTEUR_MSIX_NOM       Package/Identity/Name         (par exemple 12345AtelierduVerdier.MonteurVerdierCAM)
    VCAM_MSIX_EDITEUR      Package/Identity/Publisher    (par exemple CN=1A2B3C4D-…, le même que pour VerdierCAM)
    VCAM_MSIX_EDITEUR_NOM  Package/Properties/PublisherDisplayName
Sans elles, c'est un paquet d'ESSAI, d'éditeur « CN=Atelier du Verdier Essai », que le Store refuserait.
UN PAQUET À INSTALLER À LA MAIN SE SIGNE (29/10/2026, PC de l'atelier, Windows 11 25H2) : non signé, Windows le
déballe, puis le refuse — « un package non signé ne peut pas contenir d'activations exécutables » (0x80073D2B), même
par « Add-AppxPackage -AllowUnsigned ». Le paquet envoyé au Store, lui, part non signé : le Store le signe.
    VCAM_MSIX_PFX=<fichier.pfx>  VCAM_MSIX_MOT_DE_PASSE_FICHIER=<fichier>    signe par osslsigncode 2.6 ou plus récent
Le sujet du certificat doit être, lettre pour lettre, l'éditeur du manifeste (VCAM_MSIX_EDITEUR, ou celui de l'essai) ;
pour un essai, un certificat fait pour l'occasion, que le Windows d'essai range dans Cert:\\LocalMachine\\TrustedPeople
(docs/windows.md, « Le Microsoft Store »). Un certificat acheté signerait de même un MSIX à télécharger hors du Store.
VCAM_MSIX_SCHEMAS : le dossier des schémas du manifeste du SDK (resources/AppxPackaging/Manifest/Schema) ; donné, le
manifeste y est validé par xmllint avant d'être empaqueté.
"""
import base64
import hashlib
import os
import re
import shutil
import struct
import subprocess
import sys
import tempfile
import zlib
from pathlib import Path

DEPOT = Path(__file__).resolve().parent.parent
EDITEUR_ESSAI = "CN=Atelier du Verdier Essai"
BLOC = 65536   # un bloc de la carte des blocs (AppxBlockMap.xml) : 64 Kio
A_LAISSER = {"LISEZMOI.txt"}   # le mode d'emploi du zip (déballer, SmartScreen) : rien à faire dans le paquet du Store


def sortir(message):
    print(message, file=sys.stderr)
    sys.exit(1)


def version_du_depot():
    m = re.search(r'^version = "([0-9.]+)"', (DEPOT / "crates" / "appli" / "Cargo.toml").read_text(), re.M)
    if not m:
        sortir("crates/appli/Cargo.toml : version introuvable")
    return m.group(1)


def vignettes(dossier):
    """Les images du manifeste, la carte de l'icône (sa ligne de temps) sur fond transparent (la tuile prend la couleur
    du manifeste)."""
    logo = DEPOT / "ressources" / "fr.atelierduverdier.Monteur2.svg"
    dossier.mkdir(parents=True, exist_ok=True)
    # nom, largeur, hauteur, taille de la carte
    for nom, l, h, carte in (("Square44x44Logo", 44, 44, 40), ("Square150x150Logo", 150, 150, 110),
                             ("Wide310x150Logo", 310, 150, 120), ("StoreLogo", 50, 50, 46)):
        with tempfile.NamedTemporaryFile(suffix=".png") as t:
            subprocess.run(["rsvg-convert", "-w", str(carte), "-h", str(carte), str(logo), "-o", t.name], check=True)
            subprocess.run(["magick", t.name, "-background", "none", "-gravity", "center", "-extent", f"{l}x{h}",
                            "-depth", "8", f"PNG32:{dossier / (nom + '.png')}"], check=True)


def manifeste(version, nom, editeur, editeur_nom):
    texte = (DEPOT / "outils" / "windows" / "AppxManifest.xml.in").read_text(encoding="utf-8")
    for cle, valeur in (("@NOM@", nom), ("@EDITEUR@", editeur), ("@EDITEUR_NOM@", editeur_nom),
                        ("@VERSION@", version + ".0")):
        texte = texte.replace(cle, valeur.replace("&", "&amp;").replace('"', "&quot;").replace("<", "&lt;"))
    return texte


def valider_manifeste(chemin, schemas):
    """xmllint contre les schémas du SDK. Ils sont écrits pour le lecteur de Microsoft : huit motifs y sortent de la
    norme XSD (\\x01, \\b, (?!…)) et le schéma des serveurs ne se compile pas — on travaille sur une copie, ces motifs
    élargis à « .* », le schéma des serveurs laissé de côté. La structure, les attributs exigés et leurs valeurs
    énumérées restent contrôlés."""
    with tempfile.TemporaryDirectory() as t:
        copie = Path(t) / "schemas"
        shutil.copytree(schemas, copie)
        espaces = {}
        for f in sorted(copie.rglob("*.xsd")):
            texte = f.read_text(encoding="utf-8")
            texte2 = re.sub(r'(<xs:pattern\s+value=")([^"]*)(")',
                            lambda m: m.group(1) + (".*" if re.search(r"\\x|\\b|\(\?", m.group(2)) else m.group(2))
                            + m.group(3), texte)
            if texte2 != texte:
                f.write_text(texte2, encoding="utf-8")
            cible = re.search(r'targetNamespace="([^"]+)"', texte)
            if cible and "Bundle" not in f.name and "ServerManifest" not in f.name:
                espaces.setdefault(cible.group(1), f)
        tous = Path(t) / "tous.xsd"
        tous.write_text('<?xml version="1.0"?>\n<xs:schema xmlns:xs="http://www.w3.org/2001/XMLSchema">\n'
                        + "".join(f'  <xs:import namespace="{ns}" schemaLocation="{f.as_uri()}"/>\n'
                                  for ns, f in espaces.items())
                        + "</xs:schema>\n", encoding="utf-8")
        r = subprocess.run(["xmllint", "--noout", "--schema", str(tous), str(chemin)], capture_output=True, text=True)
        if r.returncode != 0:
            sortir("Le manifeste ne passe pas les schémas de Microsoft :\n"
                   + "\n".join(l for l in r.stderr.splitlines() if "kipping" not in l and not l.startswith("warning")))


# LE FORMAT, tel que makemsix l'écrit (SDK MSIX, src/msix/pack et src/msix/common, relu le 29/10/2026) :
#   - les fichiers, puis AppxManifest.xml, AppxBlockMap.xml et [Content_Types].xml, puis le répertoire central et les
#     trois fins de zip (ZIP64 toujours : « Because we only use zip64 ») ;
#   - en-tête local de 30 octets sans champ extra, version 45, drapeau 8 (descripteur après les données), CRC et tailles
#     à zéro ; après les données, un descripteur de 24 octets aux tailles sur 64 bits ;
#   - chaque bloc de 64 Kio comprimé à part (deflate brut, niveau 9, Z_FULL_FLUSH : il se décomprime seul), puis
#     Z_FINISH — les deux octets de fin ne comptent dans la taille d'aucun bloc ; les PNG sont rangés sans compression ;
#   - le nom dans le zip encodé (« libc++.dll » y devient « libc%2B%2B.dll »), celui de la carte des blocs non, avec « \ ».
EXTENSIONS_RANGEES = set("appx cab docm docx dotm dotx gz potm potx ppam ppsm ppsx pptm pptx rar tar xap xlam xlsb xlsm "
                         "xlsx xltm xltx zip m4a mp3 wma gif jpg jpeg png avi mpeg mpg mov wmv".split())
TYPES = {"dll": "application/x-msdownload", "exe": "application/x-msdownload", "json": "application/json",
         "png": "image/png", "txt": "text/plain", "xml": "text/xml", "svg": "image/svg+xml", "js": "application/x-javascript",
         "css": "text/css", "htm": "text/html", "html": "text/html", "ico": "image/vnd.microsoft.icon", "bmp": "image/bmp",
         "gif": "image/gif", "jpg": "image/jpeg", "jpeg": "image/jpeg", "zip": "application/x-zip-compressed"}
TYPE_MANIFESTE = "application/vnd.ms-appx.manifest+xml"
TYPE_CARTE = "application/vnd.ms-appx.blockmap+xml"
A_ENCODER = set(" !#$%&'()+,;=@[]^`{}")
NOMS_DE_PERIPHERIQUES = {"con", "prn", "aux", "nul"} | {f"com{i}" for i in range(1, 10)} | {f"lpt{i}" for i in range(1, 10)}


def verifier_nom(nom):
    """Ce que makemsix refuse (FileNameValidation.cpp) ou garde pour le paquet lui-même."""
    bas = nom.lower()
    if bas in ("appxmanifest.xml", "appxblockmap.xml", "appxsignature.p7x", "[content_types].xml") \
            or bas.startswith(("appxmetadata", "microsoft.system.package.metadata")):
        sortir(f"{nom} : ce nom est réservé au paquet")
    for morceau in nom.split("/"):
        if not morceau or morceau[-1] in " ." or len(morceau) > 255 or morceau.lower().startswith("xn--") \
                or morceau.lower().split(".")[0] in NOMS_DE_PERIPHERIQUES \
                or any(ord(c) < 32 or c in '"*:<>?|\\' for c in morceau):
            sortir(f"{nom} : un nom de fichier que Windows refuserait dans un paquet")


def encoder(nom):
    return "".join("".join(f"%{o:02X}" for o in c.encode("utf-8")) if (c in A_ENCODER or ord(c) > 0x7F) else c
                   for c in nom)


def heure_dos():
    import time
    t = time.localtime()
    return (t.tm_hour << 11) | (t.tm_min << 5) | (t.tm_sec // 2), ((t.tm_year - 1980) << 9) | (t.tm_mon << 5) | t.tm_mday


def echapper(texte):
    return texte.replace("&", "&amp;").replace("<", "&lt;").replace(">", "&gt;").replace("\r", "&#xD;")


def ecrire_msix(chemin, fichiers, manifeste_octets):
    """Le paquet MSIX non signé (le Store le signe), comme makemsix pack : voir LE FORMAT ci-dessus."""
    heure, date = heure_dos()
    central = []
    carte = ['<?xml version="1.0" encoding="UTF-8" standalone="no"?><BlockMap '
             'xmlns="http://schemas.microsoft.com/appx/2010/blockmap" HashMethod="http://www.w3.org/2001/04/xmlenc#sha256">']
    types = ['<?xml version="1.0" encoding="UTF-8" standalone="yes"?><Types '
             'xmlns="http://schemas.openxmlformats.org/package/2006/content-types">']
    vus = {}   # extension en minuscules → type

    def type_de(nom_zip, type_impose=None):
        dernier = nom_zip.rsplit("/", 1)[-1]
        if "." not in dernier:
            types.append(f'<Override ContentType="{type_impose or "application/octet-stream"}" PartName="/{nom_zip}"/>')
            return
        ext = dernier.rsplit(".", 1)[1]
        contenu = type_impose or TYPES.get(ext.lower(), "application/octet-stream")
        if ext.lower() not in vus:
            vus[ext.lower()] = contenu
            types.append(f'<Default ContentType="{contenu}" Extension="{echapper(ext)}"/>')
        elif vus[ext.lower()] != contenu:
            types.append(f'<Override ContentType="{contenu}" PartName="/{nom_zip}"/>')

    with open(chemin, "wb") as sortie:
        def ajouter(nom, donnees, comprimer, dans_la_carte):
            nom_zip = nom if nom == "[Content_Types].xml" else encoder(nom)
            octets_nom = nom_zip.encode("ascii")
            decalage = sortie.tell()
            methode = 8 if comprimer else 0
            sortie.write(struct.pack("<IHHHHHIIIHH", 0x04034B50, 45, 0x0008, methode, heure, date, 0, 0, 0,
                                     len(octets_nom), 0) + octets_nom)
            blocs = []   # (empreinte, taille comprimée ou None)
            if comprimer:
                c = zlib.compressobj(9, zlib.DEFLATED, -15, 9)
                ecrit = 0
                for i in range(0, len(donnees), BLOC):
                    morceau = donnees[i:i + BLOC]
                    sortie_bloc = c.compress(morceau) + c.flush(zlib.Z_FULL_FLUSH)
                    sortie.write(sortie_bloc)
                    ecrit += len(sortie_bloc)
                    blocs.append((hashlib.sha256(morceau).digest(), len(sortie_bloc)))
                fin = c.flush(zlib.Z_FINISH)
                sortie.write(fin)
                taille_comprimee = ecrit + len(fin)
            else:
                sortie.write(donnees)
                taille_comprimee = len(donnees)
                blocs = [(hashlib.sha256(donnees[i:i + BLOC]).digest(), None) for i in range(0, len(donnees), BLOC)]
            crc = zlib.crc32(donnees) & 0xFFFFFFFF
            sortie.write(struct.pack("<IIQQ", 0x08074B50, crc, taille_comprimee, len(donnees)))
            if max(taille_comprimee, len(donnees), decalage) > 0xFFFFFFFE:
                sortir(f"{nom} : un paquet de plus de 4 Go demanderait les champs ZIP64 du répertoire central")
            central.append(struct.pack("<IHHHHHHIIIHHHHHII", 0x02014B50, 45, 45, 0x0008, methode, heure, date, crc,
                                       taille_comprimee, len(donnees), len(octets_nom), 0, 0, 0, 0, 0, decalage)
                           + octets_nom)
            if dans_la_carte:
                element = f'<File Name="{echapper(nom.replace("/", chr(92)))}" Size="{len(donnees)}" LfhSize="{30 + len(octets_nom)}"'
                if not blocs:
                    carte.append(element + "/>")
                else:
                    carte.append(element + ">" + "".join(
                        f'<Block Hash="{base64.b64encode(h).decode()}"' + (f' Size="{t}"' if t is not None else "") + "/>"
                        for h, t in blocs) + "</File>")
            return nom_zip

        for nom, disque in fichiers:
            ext = nom.rsplit(".", 1)[-1].lower() if "." in nom.rsplit("/", 1)[-1] else ""
            type_de(ajouter(nom, disque.read_bytes(), ext not in EXTENSIONS_RANGEES, True))
        type_de(ajouter("AppxManifest.xml", manifeste_octets, True, True), TYPE_MANIFESTE)
        carte.append("</BlockMap>")
        type_de(ajouter("AppxBlockMap.xml", "".join(carte).encode("utf-8"), True, False), TYPE_CARTE)
        types.append("</Types>")
        ajouter("[Content_Types].xml", "".join(types).encode("utf-8"), True, False)

        debut_central = sortie.tell()
        for entree in central:
            sortie.write(entree)
        taille_centrale = sortie.tell() - debut_central
        fin64 = sortie.tell()
        sortie.write(struct.pack("<IQHHIIQQQQ", 0x06064B50, 44, 45, 45, 0, 0, len(central), len(central),
                                 taille_centrale, debut_central))
        sortie.write(struct.pack("<IIQI", 0x07064B50, 0, fin64, 1))
        sortie.write(struct.pack("<IHHHHIIH", 0x06054B50, 0, 0, 0xFFFF, 0xFFFF, 0xFFFFFFFF, 0xFFFFFFFF, 0))


def relire(chemin):
    """Le paquet relu après coup, comme Windows le lira : chaque fichier de la carte des blocs est dans le zip, à sa
    taille, son en-tête local de la longueur annoncée, chaque bloc à son empreinte ; et rien dans le zip qui n'y soit pas,
    hormis la carte et la table des types."""
    import xml.etree.ElementTree as ET
    import zipfile
    ns = {"b": "http://schemas.microsoft.com/appx/2010/blockmap"}
    with zipfile.ZipFile(chemin) as z, open(chemin, "rb") as brut:
        if z.testzip() is not None:
            sortir(f"{chemin} : un fichier du zip ne se relit pas (CRC)")
        carte = ET.fromstring(z.read("AppxBlockMap.xml"))
        dans_zip = {i.filename: i for i in z.infolist()}
        vus = set()
        for f in carte.findall("b:File", ns):
            nom = encoder(f.get("Name").replace("\\", "/"))
            info = dans_zip.get(nom)
            if info is None:
                sortir(f"carte des blocs : {nom} n'est pas dans le zip")
            vus.add(nom)
            donnees = z.read(info)
            if len(donnees) != int(f.get("Size")):
                sortir(f"carte des blocs : {nom}, taille {f.get('Size')} annoncée, {len(donnees)} lue")
            brut.seek(info.header_offset)
            entete = brut.read(30)
            n_nom, n_extra = struct.unpack("<HH", entete[26:30])
            if 30 + n_nom + n_extra != int(f.get("LfhSize")):
                sortir(f"carte des blocs : {nom}, en-tête local de {30 + n_nom + n_extra} octets, {f.get('LfhSize')} annoncés")
            blocs = f.findall("b:Block", ns)
            if len(blocs) != max(1, -(-len(donnees) // BLOC)) and not (len(donnees) == 0 and not blocs):
                sortir(f"carte des blocs : {nom}, {len(blocs)} blocs pour {len(donnees)} octets")
            for i, b in enumerate(blocs):
                if base64.b64encode(hashlib.sha256(donnees[i * BLOC:(i + 1) * BLOC]).digest()).decode() != b.get("Hash"):
                    sortir(f"carte des blocs : {nom}, bloc {i} à la mauvaise empreinte")
            # Comprimé : chaque bloc se décomprime SEUL, à sa taille annoncée — Windows lit le paquet bloc par bloc.
            if info.compress_type == zipfile.ZIP_DEFLATED:
                brut.seek(info.header_offset + 30 + n_nom + n_extra)
                comprime = brut.read(info.compress_size)
                pos = 0
                for i, b in enumerate(blocs):
                    t = int(b.get("Size"))
                    try:
                        seul = zlib.decompressobj(-15).decompress(comprime[pos:pos + t])
                    except zlib.error:
                        seul = None
                    if seul != donnees[i * BLOC:(i + 1) * BLOC]:
                        sortir(f"carte des blocs : {nom}, le bloc {i} ne se décomprime pas seul")
                    pos += t
                if info.compress_size - pos > 16:
                    sortir(f"carte des blocs : {nom}, {info.compress_size - pos} octets comprimés hors des blocs")
            elif any(b.get("Size") is not None for b in blocs):
                sortir(f"carte des blocs : {nom} est rangé sans compression, ses blocs n'ont pas de taille")
        # Chaque programme et chaque image que le manifeste nomme est dans le paquet.
        for cite in re.findall(r'(?:Executable|Logo|Square150x150Logo|Square44x44Logo|Wide310x150Logo)="([^"]+)"|'
                               r"<(?:uap:)?Logo>([^<]+)</", z.read("AppxManifest.xml").decode("utf-8")):
            cite = "".join(cite).replace("\\", "/")
            if encoder(cite) not in dans_zip:
                sortir(f"le manifeste cite {cite}, absent du paquet")
        reste = set(dans_zip) - vus - {"AppxBlockMap.xml", "[Content_Types].xml", "AppxSignature.p7x"}
        if reste:
            sortir(f"dans le zip sans être dans la carte des blocs : {sorted(reste)[:5]}")
        types = ET.fromstring(z.read("[Content_Types].xml"))
        tns = "{http://schemas.openxmlformats.org/package/2006/content-types}"
        defauts = {d.get("Extension").lower() for d in types.iter(tns + "Default")}
        remplaces = {o.get("PartName").lower() for o in types.iter(tns + "Override")}
        for nom in dans_zip:
            if nom == "[Content_Types].xml":
                continue
            ext = nom.rsplit(".", 1)[-1].lower() if "." in nom.rsplit("/", 1)[-1] else ""
            if ext not in defauts and "/" + nom.lower() not in remplaces:
                sortir(f"[Content_Types].xml : aucun type pour {nom}")
    return len(vus)


def main():
    source = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 \
        else DEPOT / "dist" / f"Monteur-VerdierCAM-{version_du_depot()}-windows-x64"
    programme = source
    if not (programme / "Monteur VerdierCAM.exe").is_file():
        sortir(f"{programme}/Monteur VerdierCAM.exe manque : outils/construire-windows.sh d'abord")
    # La version du paquet est celle des programmes qu'il emporte : le nom du dossier assemblé.
    m = re.fullmatch(r"Monteur-VerdierCAM-([0-9]+\.[0-9]+\.[0-9]+)-windows-x64", source.name)
    if not m:
        sortir(f"{source.name} : le dossier attendu s'appelle Monteur-VerdierCAM-<version>-windows-x64")
    version = m.group(1)
    nom = os.environ.get("MONTEUR_MSIX_NOM", "")
    editeur = os.environ.get("VCAM_MSIX_EDITEUR", "")
    editeur_nom = os.environ.get("VCAM_MSIX_EDITEUR_NOM", "")
    essai = not (nom or editeur or editeur_nom)
    if essai:
        nom, editeur, editeur_nom = "AtelierDuVerdier.MonteurVerdierCAM", EDITEUR_ESSAI, "Atelier du Verdier"
    elif not (nom and editeur and editeur_nom):
        sortir("MONTEUR_MSIX_NOM, VCAM_MSIX_EDITEUR et VCAM_MSIX_EDITEUR_NOM vont ensemble (Espace partenaires › Identité du produit)")
    if not re.fullmatch(r"[-.A-Za-z0-9]{3,50}", nom):
        sortir(f"MONTEUR_MSIX_NOM « {nom} » : de 3 à 50 lettres, chiffres, points ou tirets")

    sortie = source.parent / f"{source.name}.msix"
    with tempfile.TemporaryDirectory(dir=source.parent) as t:
        disposition = Path(t)
        vignettes(disposition / "Assets")
        texte = manifeste(version, nom, editeur, editeur_nom)
        (disposition / "AppxManifest.xml").write_text(texte, encoding="utf-8")
        if os.environ.get("VCAM_MSIX_SCHEMAS"):
            valider_manifeste(disposition / "AppxManifest.xml", Path(os.environ["VCAM_MSIX_SCHEMAS"]))
            print("manifeste conforme aux schémas de Microsoft")
        fichiers = []   # (nom dans le paquet, chemin sur le disque)
        for racine in (programme, disposition / "Assets"):
            for f in sorted(p for p in racine.rglob("*") if p.is_file()):
                interne = f.relative_to(programme if racine == programme else disposition).as_posix()
                if racine == programme and interne in A_LAISSER:
                    continue
                verifier_nom(interne)
                fichiers.append((interne, f))
        partiel = sortie.with_suffix(".msix.partiel")
        pfx = os.environ.get("VCAM_MSIX_PFX", "")
        try:
            ecrire_msix(partiel, fichiers, (disposition / "AppxManifest.xml").read_bytes())
            n = relire(partiel)
            if pfx:
                signer(partiel, Path(pfx), Path(os.environ.get("VCAM_MSIX_MOT_DE_PASSE_FICHIER", "")))
                relire(partiel)
        except BaseException:
            partiel.unlink(missing_ok=True)   # un paquet qui ne se relit pas ne reste pas à côté des bons
            raise
        partiel.replace(sortie)
    taille = sortie.stat().st_size / 1e6
    print(f"{sortie} ({taille:.0f} Mo, {n} fichiers, version {version}.0, {'signé' if pfx else 'non signé'})"
          + (f" — ESSAI : {EDITEUR_ESSAI}, le Store le refuserait" if essai else f" — {nom}, {editeur}"))


def signer(chemin, pfx, mot_de_passe):
    """La signature d'un MSIX par osslsigncode (2.6 au moins : avant, il ne connaît pas les paquets APPX). Il ajoute
    AppxSignature.p7x et son type ; la carte des blocs et les fichiers ne bougent pas."""
    if not shutil.which("osslsigncode"):
        sortir("VCAM_MSIX_PFX donné, mais osslsigncode manque (paquet AUR osslsigncode, ou Debian trixie)")
    v = re.search(r"osslsigncode (\d+)\.(\d+)", subprocess.run(["osslsigncode", "--version"], capture_output=True,
                                                               text=True).stdout + " ")
    if not v or (int(v.group(1)), int(v.group(2))) < (2, 6):
        sortir("osslsigncode 2.6 ou plus récent : les versions d'avant ne signent pas un MSIX")
    if not mot_de_passe.is_file():
        sortir("VCAM_MSIX_MOT_DE_PASSE_FICHIER : fichier du mot de passe illisible")
    signe = chemin.with_suffix(".signe")
    # Le même serveur d'horodatage que construire-windows.sh, et la même variable pour s'en passer (vide : un essai
    # hors réseau, avec un certificat jetable ; la signature meurt alors avec le certificat).
    horodatage = os.environ.get("VCAM_SIGNATURE_HORODATAGE", "http://timestamp.sectigo.com")
    subprocess.run(["osslsigncode", "sign", "-pkcs12", str(pfx), "-readpass", str(mot_de_passe), "-h", "sha256"]
                   + (["-t", horodatage] if horodatage else []) + ["-in", str(chemin), "-out", str(signe)],
                   check=True, stdout=subprocess.DEVNULL)
    signe.replace(chemin)


if __name__ == "__main__":
    main()
