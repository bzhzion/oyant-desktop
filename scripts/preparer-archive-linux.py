"""Assemble l'archive tar.gz de Linux, pour qui ne passe ni par apt, ni par pacman, ni par rpm.

Contenu, sous un repertoire `oyant-<version>/` :
- `oyant`, le binaire, executable ;
- `oyant.png` et `oyant.desktop`, pour l'entree de menu ;
- `LICENSE.md` et `NOTICE.md`, qui voyagent avec tout ce qu'on redistribue ;
- `LISEZMOI.txt`, qui dit quoi installer et ou copier quoi.

⛔ **L'archive est RELUE apres ecriture, et le script echoue si elle ne contient pas exactement ce
qui est attendu, binaire executable compris.** Un controle qui relirait les fichiers SOURCES
dirait vert sur une archive incomplete : c'est l'effet qu'on verifie, pas l'intention.

⚠️ **Archive reproductible** : dates, proprietaires et ordre fixes. Deux constructions du meme tag
donnent le meme fichier, donc la meme empreinte, ce qui permet de verifier une copie.

⚠️ Le `.desktop` est aussi ecrit dans `packaging/pacman/oyant-bin/PKGBUILD`. Les deux doivent
dire la meme chose, a `Exec` pres (chemin absolu la-bas, nom seul ici, puisque l'utilisateur
choisit ou il copie le binaire).

Usage :
  python scripts/preparer-archive-linux.py --version 0.4.0 --binaire diffusion/oyant-linux-x64 \
      --destination diffusion
"""

import argparse
import gzip
import io
import sys
import tarfile
from pathlib import Path

RACINE = Path(__file__).resolve().parent.parent
ICONE = RACINE / "src-tauri" / "icons" / "128x128@2x.png"
LICENCE = RACINE / "LICENSE.md"
NOTICE = RACINE / "NOTICE.md"

BUREAU = """[Desktop Entry]
Name=Oyant
Comment=Dictée vocale locale
Exec=oyant
Icon=oyant
Type=Application
Categories=Utility;Accessibility;
"""

LISEZMOI = """Oyant {version} pour Linux (x86_64)
=====================================

Dictée vocale locale : aucun enregistrement ne quitte l'ordinateur.

Si votre distribution a apt, pacman ou rpm, préférez le paquet : il installe les dépendances et
se met à jour tout seul. Voir https://oyant.breizhzion.com.

Avec cette archive, installez d'abord les bibliothèques dont Oyant a besoin :
  Debian, Ubuntu : libwebkit2gtk-4.1-0 libgtk-3-0 libayatana-appindicator3-1 libasound2
  Fedora         : webkit2gtk4.1 gtk3 libayatana-appindicator-gtk3 alsa-lib
  Arch           : webkit2gtk-4.1 gtk3 libayatana-appindicator alsa-lib

Puis, pour votre seul compte :
  install -Dm755 oyant ~/.local/bin/oyant
  install -Dm644 oyant.png ~/.local/share/icons/hicolor/256x256/apps/oyant.png
  install -Dm644 oyant.desktop ~/.local/share/applications/oyant.desktop

~/.local/bin doit être dans votre PATH. Oyant apparaît ensuite dans le menu des applications.

Au premier usage, Oyant télécharge son programme de reconnaissance et le modèle choisi. Ensuite,
tout se fait hors ligne.

Licence : LICENSE.md. Composants tiers : NOTICE.md.
"""

# Date fixe des entrees de l'archive : celle du 1er janvier 2026, arbitraire mais stable.
DATE = 1767225600


def entree(nom: str, donnees: bytes, mode: int) -> tuple[tarfile.TarInfo, bytes]:
    info = tarfile.TarInfo(nom)
    info.size = len(donnees)
    info.mode = mode
    info.mtime = DATE
    info.uid = info.gid = 0
    info.uname = info.gname = "root"
    return info, donnees


def main() -> int:
    analyseur = argparse.ArgumentParser(description=__doc__)
    analyseur.add_argument("--version", required=True, help="version sans le v initial")
    analyseur.add_argument("--binaire", required=True, type=Path)
    analyseur.add_argument("--destination", required=True, type=Path)
    options = analyseur.parse_args()

    if options.version.startswith("v"):
        print("erreur : --version s'attend a 0.4.0 et pas a v0.4.0", file=sys.stderr)
        return 1
    for chemin in (options.binaire, ICONE, LICENCE, NOTICE):
        if not chemin.is_file():
            print(f"erreur : {chemin} est introuvable", file=sys.stderr)
            return 1

    dossier = f"oyant-{options.version}"
    contenu = [
        entree(f"{dossier}/oyant", options.binaire.read_bytes(), 0o755),
        entree(f"{dossier}/oyant.png", ICONE.read_bytes(), 0o644),
        entree(f"{dossier}/oyant.desktop", BUREAU.encode("utf-8"), 0o644),
        entree(f"{dossier}/LICENSE.md", LICENCE.read_bytes(), 0o644),
        entree(f"{dossier}/NOTICE.md", NOTICE.read_bytes(), 0o644),
        entree(
            f"{dossier}/LISEZMOI.txt",
            LISEZMOI.format(version=options.version).encode("utf-8"),
            0o644,
        ),
    ]

    tampon = io.BytesIO()
    # `format=GNU_FORMAT` et pas PAX : PAX ajoute des en-tetes de date a la seconde pres qui
    # rendraient l'archive non reproductible.
    with tarfile.open(fileobj=tampon, mode="w", format=tarfile.GNU_FORMAT) as archive:
        for info, donnees in contenu:
            archive.addfile(info, io.BytesIO(donnees))

    options.destination.mkdir(parents=True, exist_ok=True)
    cible = options.destination / f"oyant-{options.version}-linux-x64.tar.gz"
    # `mtime=0` dans l'en-tete gzip, pour la meme raison de reproductibilite.
    with open(cible, "wb") as sortie, gzip.GzipFile(
        filename="", mode="wb", fileobj=sortie, mtime=0
    ) as compresse:
        compresse.write(tampon.getvalue())

    # Relecture de ce qui a REELLEMENT ete ecrit.
    with tarfile.open(cible, "r:gz") as relue:
        trouves = {m.name: m for m in relue.getmembers()}
    attendus = {info.name: info for info, _ in contenu}
    if set(trouves) != set(attendus):
        print(f"erreur : l'archive contient {sorted(trouves)}", file=sys.stderr)
        return 1
    binaire = trouves[f"{dossier}/oyant"]
    if binaire.mode & 0o111 == 0 or binaire.size != options.binaire.stat().st_size:
        print("erreur : le binaire de l'archive n'est pas executable ou pas complet", file=sys.stderr)
        return 1

    print(f"{cible} ({cible.stat().st_size} octets, {len(trouves)} fichiers)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
