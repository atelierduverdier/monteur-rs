#!/usr/bin/env python3
"""LE MODE D'EMPLOI EN UNE PAGE HTML (22/11/2026), pour le livrer avec le Monteur : Aide › Mode d'emploi l'ouvre
dans le navigateur, sans réseau.

    python3 outils/mode-emploi-html.py docs/mode-emploi.md sortie.html

python-markdown (paquet python-markdown d'Arch). Une feuille de style tient dans la page, claire ou sombre selon le
système, aux couleurs de VerdierCAM.
"""
import html
import pathlib
import sys

import markdown

STYLE = """
:root { --fond: #ffffff; --texte: #1e1e1e; --doux: #5a5a5a; --filet: #e2e2e2; --orange: #c75400; --code: #f3f3f3; }
@media (prefers-color-scheme: dark) {
  :root { --fond: #121214; --texte: #e6e6e6; --doux: #a0a0a0; --filet: #2c2c30; --orange: #ff6d00; --code: #1e1e22; }
}
body { background: var(--fond); color: var(--texte); font: 16px/1.6 system-ui, sans-serif; margin: 0; }
main { max-width: 760px; margin: 0 auto; padding: 32px 16px 64px; }
h1 { font-size: 2em; border-left: 6px solid var(--orange); padding-left: 14px; }
h2 { margin-top: 2em; color: var(--orange); }
code, pre { background: var(--code); border-radius: 4px; font-family: ui-monospace, monospace; }
code { padding: 1px 5px; }
pre { padding: 12px; overflow-x: auto; }
table { border-collapse: collapse; }
td, th { border-bottom: 1px solid var(--filet); padding: 6px 12px; text-align: left; vertical-align: top; }
a { color: var(--orange); }
"""


def main():
    source, sortie = pathlib.Path(sys.argv[1]), pathlib.Path(sys.argv[2])
    texte = source.read_text(encoding="utf-8")
    titre = texte.splitlines()[0].lstrip("# ").strip()
    corps = markdown.markdown(texte, extensions=["tables", "fenced_code"])
    page = (
        f'<!doctype html>\n<html lang="fr">\n<head>\n<meta charset="utf-8">\n'
        f'<meta name="viewport" content="width=device-width, initial-scale=1">\n'
        f"<title>{html.escape(titre)}</title>\n<style>{STYLE}</style>\n</head>\n<body>\n<main>\n{corps}\n</main>\n</body>\n</html>\n"
    )
    sortie.parent.mkdir(parents=True, exist_ok=True)
    sortie.write_text(page, encoding="utf-8")


if __name__ == "__main__":
    main()
