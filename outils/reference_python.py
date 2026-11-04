#!/usr/bin/env python3
"""Fait calculer au Monteur 1 (PySide6) ce que le noyau Rust doit retrouver à l'identique, sur
les fichiers de `crates/noyau/tests/donnees/`. Écrit `reference.json` à côté d'eux.

python3 outils/reference_python.py [~/Projets/logiciels/monteur]
Ne lit que les données de contrôle ; n'écrit que reference.json.
"""
import json
import sys
from dataclasses import asdict
from pathlib import Path

ICI = Path(__file__).resolve().parent.parent / "crates" / "noyau" / "tests" / "donnees"
sys.path.insert(0, str(Path(sys.argv[1] if len(sys.argv) > 1 else "~/Projets/logiciels/monteur").expanduser()))
import montage as mt  # noqa: E402
import noyau  # noqa: E402

ref = {"montages": {}, "journal": {}, "projet": {}, "fiches": {}, "description": {}}

for chemin in sorted(ICI.glob("* montage.json")):
    m = mt.lire(chemin)
    cles = mt._cles_zoom(m.zooms)
    instants = [m.duree * k / 997 for k in range(998)]
    ref["montages"][chemin.name] = {
        "reecrit": mt.en_dict(m),
        "vide": m.vide(),
        "coupes_actives": m.coupes_actives(),
        "accelerations": m.accelerations(),
        "duree_montee": m.duree_montee(),
        "sous_titres_a_part": bool(m.sous_titres_a_part()),
        "vitesses": [m.vitesse_a(t) for t in instants],
        "dans_une_coupe": [m.dans_une_coupe(t) for t in instants],
        "cles": cles,
        "etats": [mt.etat_zoom(m.zooms, t, cles) for t in instants],
        "rects": [mt.rect_source(*mt.etat_zoom(m.zooms, t, cles), m.largeur, m.hauteur) for t in instants],
        "temps_montes": [mt.temps_monte(m.coupes_actives(), t) for t in instants],
        "instants": instants,
    }

for chemin in sorted(ICI.glob("gestes *.jsonl")):
    j = mt.lire_journal(chemin)
    video = ICI / "2026-11-02 10-06-06.mp4"   # un nom d'OBS : le fichier n'a pas à exister
    ref["journal"][chemin.name] = {
        "debut": j.debut, "nb": len(j.evenements), "premiers": j.evenements[:5],
        "derniers": j.evenements[-3:], "taille_fenetre": list(j.taille_fenetre),
        "debut_video": mt.debut_de_la_video(video, 94.6),
        "decalage": mt.decalage_journal(j, video, 94.6),
        "trouve": mt.trouver_journal(video, 94.6, [ICI]).chemin.name,
    }

for chemin in sorted(ICI.glob("*.monteur")):
    p = noyau.lire_projet(chemin)
    ref["projet"][chemin.name] = {
        "video": str(p.video), "taille": p.taille, "enregistre_le": p.enregistre_le,
        "fiche": asdict(p.fiche), "description": asdict(p.description),
        "montage": mt.en_dict(mt.depuis_dict(p.montage)) if p.montage else None,
    }

r = noyau.Registre(ICI / "fiches.json")
ref["fiches"] = {"dossier": str(r.dossier), "rangement": str(r.rangement),
                 "projets": str(r.projets) if r.projets else "",
                 "fiches": {k: asdict(f) for k, f in r.fiches.items()}}

for chemin in sorted(ICI.glob("* description YouTube.txt")):
    ref["description"][chemin.name] = asdict(noyau.lire_description(chemin))

# La préparation entière, comme « Préparer le montage » du Monteur 1, sur la prise du 02/11 à
# 10:06 : le mouvement de l'image et les silences sont mesurés par ffmpeg sur la vraie vidéo (lue
# seulement) et gardés ici, pour que le contrôle Rust tourne sans elle.
import copy  # noqa: E402
import subprocess  # noqa: E402
PRISES = [("2026-11-02 10-06-06", "gestes 2026-11-02 10-05-56.jsonl"),
          ("2026-11-04 07-59-21", "gestes 2026-11-04 07-42-17.jsonl")]
ref["preparations"] = {}
for nom, nom_journal in PRISES:
    VIDEO = Path.home() / "Vidéos" / "Copies d'écran" / f"{nom}.mp4"
    if not VIDEO.exists():
        continue
    depart = mt.lire(ICI / f"{nom} montage.json")
    m = copy.deepcopy(depart)
    gris = subprocess.run(mt.commande_image_grise(VIDEO), capture_output=True, check=True).stdout
    mouvement = mt.mouvement_image(gris)
    silences_txt = subprocess.run(mt.commande_silences(VIDEO), capture_output=True, text=True).stderr
    silences_txt = "\n".join(l for l in silences_txt.splitlines() if "silence_" in l)
    j = mt.lire_journal(ICI / nom_journal)
    dec0 = mt.decalage_journal(j, VIDEO, m.duree)
    clics = mt.instants_actifs(j, dec0, gestes_seuls=True)
    trouve = mt.calage_automatique(mouvement, clics)
    if trouve is not None and m.reglages.calage_auto:
        m.reglages.decalage_journal = trouve
    mt.analyser_journal(j, m, VIDEO, mouvement)
    mt.caler_etiquettes_sur_la_voix(m)
    silences = mt.lire_silences(silences_txt, m.duree)
    dec = mt.decalage_journal(j, VIDEO, m.duree) + m.reglages.decalage_journal
    actifs = mt.instants_actifs(j, dec)
    nouvelles = mt.temps_morts(silences, actifs, mouvement)
    m.coupes = [c for c in m.coupes if c.origine != "temps mort"] + nouvelles
    m.trier()
    ref["preparations"][nom] = {
        "journal": nom_journal,
        "mouvement": mouvement, "silences_txt": silences_txt, "clics": clics,
        "calage": trouve, "silences": silences, "actifs": actifs,
        "sans_journal": [asdict(c) for c in mt.temps_morts(silences, None, mouvement)],
        "changements": mt.changements_d_ecran(mouvement),
        "montage": mt.en_dict(m),
        "sans_mouvement": mt.en_dict(mt.analyser_journal(j, copy.deepcopy(depart), VIDEO, None)),
    }

# Les sous-titres : découpage, .srt (incrusté ou décalé des 3 s de vignette), images gardées.
ref["srt"] = {}
for chemin in sorted(ICI.glob("* montage.json")):
    m = mt.lire(chemin)
    ref["srt"][chemin.name] = {
        "texte": mt.texte_srt(m), "decale": mt.texte_srt(m, 3.0),
        "decoupes": [mt.decouper_sous_titre(st.texte) for st in m.sous_titres],
        "fusion": [asdict(s) for s in mt.fusionner_sous_titres(m.sous_titres)],
        "son_retire": mt.son_retire(m),
        "gardees": [mt.image_gardee(m, k / 30, 30) for k in range(int(m.duree * 30))],
    }
# Une transcription de whisper-cli, faite à la main avec ses pièges.
whisper = {"transcription": [
    {"text": " Bon,  on passe", "offsets": {"from": 1000, "to": 2400}},
    {"text": " dans l'atelier.", "offsets": {"from": 2500, "to": 2900}},
    {"text": "[BLANK_AUDIO]", "offsets": {"from": 3000, "to": 4000}},
    {"text": "sans temps"},
    {"text": " Et là on règle la profondeur de chaque passe", "offsets": {"from": 5000, "to": 8000}},
    {"text": " finie.", "offsets": {"from": 8100, "to": 8500}},
    {"text": " 10 mm.", "offsets": {"from": 9800, "to": 10200}},
    {"text": " La suite tout de suite après", "offsets": {"from": 10300, "to": 12500}}]}
(ICI / "whisper-essai.json").write_text(json.dumps(whisper, ensure_ascii=False), encoding="utf-8")
ref["whisper"] = [asdict(s) for s in mt.lire_whisper(ICI / "whisper-essai.json")]

# La liste d'un dossier, l'état des vidéos et le rangement, sur un dossier jetable bâti d'après
# `arbre` (le contrôle Rust bâtit le même).
import os  # noqa: E402
import tempfile  # noqa: E402
arbre = [  # nom, âge en secondes (plus grand = plus vieux)
    ("2026-11-04 08-12-40.mp4", 10), ("2026-11-04 08-12-40.mkv", 5),
    ("2026-11-04 08-12-40 vignette YouTube.png", 9), ("2026-11-04 08-12-40 montage.json", 9),
    ("2026-11-04 08-12-40.monteur", 9), ("2026-11-04 08-12-40 description YouTube.txt", 9),
    ("2026-11-03 07-55-02.mp4", 100), ("2026-11-03 07-55-02 avec vignette.mp4", 90),
    ("2026-11-01 07-30-28 avec vignette.mp4", 400), ("2026-11-01 07-30-28 montage YouTube.fr.srt", 400),
    ("2026-11-02 19-04-24.MOV", 200), ("2026-11-02 19-04-24 montage YouTube.mp4", 150),
    ("essai.partiel.mp4", 1), ("notes.txt", 2), ("bug ignoré.mp4", 300),
]
with tempfile.TemporaryDirectory() as tmp:
    tmp = Path(tmp)
    dossier, rangement = tmp / "Copies d'écran", tmp / "YouTube"
    dossier.mkdir()
    maintenant = 1_793_800_000
    for nom, age in arbre:
        (dossier / nom).write_text(nom, encoding="utf-8")
        os.utime(dossier / nom, (maintenant - age, maintenant - age))
    r = noyau.Registre(tmp / "fiches.json")
    r.fiche(dossier / "2026-11-01 07-30-28 avec vignette.mp4").youtube_id = "kO3T6NuRUxk"
    r.fiche(dossier / "bug ignoré.mp4").ignoree = True
    r.fiche(dossier / "2026-11-04 08-12-40.mp4").titre = "GRAVER UNE IMAGE"
    r.enregistrer()
    liste = noyau.lister_videos(dossier)
    rel = lambda p: str(Path(p).relative_to(tmp))  # noqa: E731
    scenario = {
        "arbre": arbre, "maintenant": maintenant,
        "liste": [p.name for p in liste],
        "etats": {p.name: noyau.etat(p, r.connue(p)) for p in liste},
        "lies": {p.name: [f.name for f in noyau.fichiers_lies(p)] for p in liste},
        "derivees": {p.name: noyau.est_derivee(p) for p in liste},
        "sources": {p.name: noyau.nom_source(p) for p in liste},
    }
    nouvelle = noyau.deplacer(r, dossier / "2026-11-04 08-12-40.mp4", rangement)
    scenario["range"] = rel(nouvelle)
    scenario["apres"] = {"dossier": sorted(p.name for p in dossier.iterdir()),
                         "rangement": sorted(p.name for p in rangement.iterdir())}
    scenario["fiches_apres"] = sorted(rel(k) for k in json.loads((tmp / "fiches.json").read_text())["fiches"])
    (dossier / "2026-11-03 07-55-02 vignette YouTube.png").write_text("x")
    (rangement / "2026-11-03 07-55-02 vignette YouTube.png").write_text("y")
    try:
        noyau.deplacer(r, dossier / "2026-11-03 07-55-02.mp4", rangement)
        scenario["refus"] = None
    except FileExistsError as e:
        scenario["refus"] = str(e).replace(str(tmp), "<tmp>")
    scenario["apres_refus"] = sorted(p.name for p in dossier.iterdir())
ref["videos"] = scenario

# ffprobe sur la vraie prise, et des sorties faites à la main.
sondes = {}
if VIDEO.exists():
    sondes["vraie"] = subprocess.run(["ffprobe", "-v", "error", "-print_format", "json", "-show_format",
                                      "-show_streams", str(VIDEO)], capture_output=True, text=True).stdout
sondes["muette"] = json.dumps({"format": {"duration": "12.5"}, "streams": [
    {"codec_type": "video", "width": 641, "height": 361, "r_frame_rate": "60/1"}]})
sondes["mono"] = json.dumps({"format": {}, "streams": [
    {"codec_type": "video", "width": 1920, "height": 1200, "duration": "7.0"},
    {"codec_type": "audio", "channels": 1, "sample_rate": "44100"}]})
ref["sondes"] = {}
for nom, texte in sondes.items():
    d = json.loads(texte)
    image = next(s for s in d["streams"] if s["codec_type"] == "video")
    son = next((s for s in d["streams"] if s["codec_type"] == "audio"), None)
    infos = noyau.Infos(duree=float(d["format"].get("duration") or image.get("duration") or 0),
                        largeur=int(image["width"]), hauteur=int(image["height"]),
                        images_par_s=image.get("r_frame_rate") or "30/1", son=son is not None)
    if son:
        infos.frequence = int(son.get("sample_rate") or 48000)
        canaux = int(son.get("channels") or 2)
        infos.disposition = son.get("channel_layout") or ("mono" if canaux == 1 else "stereo")
    ref["sondes"][nom] = {"texte": texte, "infos": asdict(infos),
                          "montage": noyau.commande_montage(Path("/v/x %d.mp4"), Path("/t/vignette.png"),
                                                            Path("/v/x avec vignette.partiel.mp4"), infos),
                          "avancement": [noyau.avancement_montage(l, infos) for l in
                                         ("out_time_us=1500000", "out_time_us=999999999999", "frame=12", " out_time_us=42")]}

tailles = [(1920, 1200), (1920, 1080), (1280, 720), (641, 361), (600, 900), (1600, 900), (3840, 2160), (500, 1000)]
ref["cadres"] = {f"{l}x{h}": {"defaut": noyau.cadre_defaut(l, h),
                              "bornes": [noyau.borner_cadre(c, l, h) for c in
                                         ([-50, -50, 100, 0], [1e5, 1e5, 1e5, 0], [10.5, 20.5, 400.5, 0],
                                          [300, 100, 646, 720], [0, 0, 159.4, 0])]}
                 for l, h in tailles}
ref["images"] = [noyau.commande_image(Path("/v/x.mp4"), t, d) for t, d in ((12.3456, None), (99, 10.0), (-3, 5.0), (4, 0))]
descriptions = [noyau.Description(), noyau.Description("t" * 101, "é" * 2501, ",".join(["mot"] * 130)),
                noyau.Description("Graver <une> image", "a > b", "VerdierCAM, CNC\nFAO")]
ref["problemes"] = [noyau.problemes_description(d) for d in descriptions]
os.environ["MONTEUR_YOUTUBE_ENVOI"] = "/faux/youtube-envoi"
ref["envois"] = [noyau.commande_envoi(Path("/v/x avec vignette.mp4"), d, Path("/v/x description YouTube.txt"), v)
                 for d, v in zip(descriptions, (None, Path("/v/x vignette YouTube.jpg"), None))]
ref["ids"] = [noyau.lire_id_youtube(s) for s in ("Envoyée : https://youtu.be/kO3T6NuRUxk (privée)",
                                                 "https://youtu.be/abc", "rien", "https://youtu.be/a_b-c9Zé!")]
ref["pourcentages"] = [noyau.lire_pourcentage(s) for s in ("Envoi : 42,5 %", "12%", "x 7.25  % y", "100 %", "aucun", "3 4 %")]
ref["partiels"] = [str(noyau.chemin_partiel(Path(p))) for p in ("/v/x avec vignette.mp4", "/v/x.vignette.png", "/v/sans")]

(ICI / "reference.json").write_text(json.dumps(ref, ensure_ascii=False, indent=1), encoding="utf-8")
print("reference.json :", sum(len(v) for v in ref.values() if isinstance(v, dict)), "entrées")
