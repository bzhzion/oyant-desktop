"""Mesure la vitesse et le taux d'erreurs de chaque modele, et ecrit src-tauri/mesures-modeles.json.

Pourquoi un script et pas des chiffres dans le code : un nombre ecrit a la main a cote du code
qu'il decrit finit par mentir. L'ecran de comparaison des modeles lit CE fichier, et ce fichier ne
s'ecrit que par ce script, qui dit sur quelle machine et sur quel corpus il a mesure.

Le corpus : trois lectures LibriVox en francais (domaine public), trente secondes chacune, et le
texte des livres comme reference. Trois voix et pas une : Parakeet est meilleur que Whisper Small
sur deux d'entre elles et rend un texte inutilisable sur la troisieme. Une seule voix aurait donne
l'une ou l'autre conclusion, et les deux auraient ete fausses.

Le moteur : celui qu'Oyant embarque (processeur seul, src-tauri/moteur/), donc le cas de quelqu'un
qui vient d'installer. Whisper tourne avec la detection de parole, comme dans le produit.

Prerequis :
  - python scripts/preparer-moteur-embarque.py   (le moteur, avec parakeet-cli)
  - ffmpeg dans le PATH
  - les modeles telecharges, par Oyant ou dans --modeles

Usage :
  python scripts/mesurer-modeles.py
  python scripts/mesurer-modeles.py --modeles D:/ailleurs --passages 3
"""

import argparse
import datetime
import html
import json
import os
import re
import statistics
import subprocess
import sys
import time
import urllib.request
import winreg
from pathlib import Path

RACINE = Path(__file__).resolve().parent.parent
MOTEUR = RACINE / "src-tauri" / "moteur"
VAD = RACINE / "src-tauri" / "vad" / "ggml-silero-v6.2.0.bin"
SORTIE = RACINE / "src-tauri" / "mesures-modeles.json"
TRAVAIL = RACINE / ".scratch" / "mesures"

DUREE = 30
FILS = 6

# Les fichiers doivent rester ceux du catalogue (`src-tauri/src/modeles.rs`) : un test Rust
# verifie que chaque modele du catalogue a sa mesure, donc un oubli ici se voit.
MODELES = {
    "small": ("whisper", "ggml-small.bin"),
    "medium": ("whisper", "ggml-medium.bin"),
    "large-v3": ("whisper", "ggml-large-v3.bin"),
    "parakeet-v3": ("parakeet", "ggml-parakeet-tdt-0.6b-v3-q8_0.bin"),
}

# (identifiant, audio LibriVox, debut en secondes, source du texte)
VOIX = [
    (
        "anciens-canadiens",
        "https://archive.org/download/les_anciens_canadiens_2203_librivox/ancienscanadiens_01_gaspe_64kb.mp3",
        60,
        "wikisource:Les_Anciens_Canadiens/1",
    ),
    (
        "boule-de-suif",
        "https://archive.org/download/bouledesuif_0904_librivox/bouledesuif_01_maupassant_64kb.mp3",
        90,
        "https://www.gutenberg.org/ebooks/10746.txt.utf-8",
    ),
    (
        "voyage-centre-terre",
        "https://archive.org/download/voyageaucentredelaterre_1210_librivox/voyageaucentredelaterre_01_verne_64kb.mp3",
        90,
        "https://www.gutenberg.org/ebooks/4791.txt.utf-8",
    ),
]


def telecharger(url: str, cible: Path) -> Path:
    if not cible.exists():
        cible.parent.mkdir(parents=True, exist_ok=True)
        requete = urllib.request.Request(url, headers={"User-Agent": "oyant-mesures"})
        with urllib.request.urlopen(requete, timeout=300) as reponse:
            cible.write_bytes(reponse.read())
    return cible


def texte_source(source: str) -> str:
    if source.startswith("wikisource:"):
        page = source.split(":", 1)[1]
        url = (
            "https://fr.wikisource.org/w/api.php?action=parse&prop=text&format=json"
            f"&formatversion=2&page={page}"
        )
        brut = json.loads(telecharger(url, TRAVAIL / f"{page.replace('/', '_')}.json").read_text("utf-8"))
        return html.unescape(re.sub(r"<[^>]+>", " ", brut["parse"]["text"]))
    return telecharger(source, TRAVAIL / source.rsplit("/", 1)[1]).read_text("utf-8")


def mots(texte: str) -> list[str]:
    """Minuscules, ponctuation retiree, apostrophes et traits d'union comme separateurs.

    ⚠️ Les ACCENTS sont gardes : « prudent » pour « prudemment » ou « préparais » sans accent sont
    de vraies fautes dans un texte dicte en francais, et les effacer flatterait tous les modeles.
    """
    texte = texte.lower().replace("’", "'").replace("œ", "oe")
    return re.findall(r"[0-9a-zàâäçéèêëîïôöùûüÿæ]+", texte)


def distance(a: list[str], b: list[str]) -> int:
    precedente = list(range(len(b) + 1))
    for i, x in enumerate(a, 1):
        courante = [i]
        for j, y in enumerate(b, 1):
            courante.append(min(precedente[j] + 1, courante[j - 1] + 1, precedente[j - 1] + (x != y)))
        precedente = courante
    return precedente[-1]


def reference(livre: list[str], sorties: list[list[str]]) -> list[str]:
    """Le passage du livre que la lecture couvre.

    ⛔ **Choisi pour minimiser la SOMME des distances de tous les modeles**, jamais celle d'un
    seul : caler la reference sur la sortie d'un modele la rendrait plus indulgente pour lui. Les
    bords restent approximatifs (une coupure tombe au milieu d'un mot), et ce bruit est le meme pour
    tous les modeles.
    """
    longueur = round(statistics.median(len(s) for s in sorties if s))

    # ⚠️ Ancrage sur des suites de TROIS mots, presque uniques dans un livre : ancrer sur un mot
    # seul ferait essayer chaque « de » et chaque « la » du texte, des heures de calcul.
    positions: dict[tuple[str, ...], list[int]] = {}
    for i in range(len(livre) - 2):
        positions.setdefault(tuple(livre[i : i + 3]), []).append(i)
    departs = set()
    for s in sorties:
        for decalage in range(min(20, len(s) - 2)):
            for position in positions.get(tuple(s[decalage : decalage + 3]), []):
                departs.add(max(0, position - decalage))
    if not departs:
        sys.exit("passage introuvable dans le texte du livre : verifier le debut de l'extrait")

    meilleur, retenu = None, []
    for debut in departs:
        for decalage in range(-6, 7):
            for ecart in (-6, -3, 0, 3, 6):
                d = max(0, debut + decalage)
                candidat = livre[d : d + longueur + ecart]
                if not candidat:
                    continue
                total = sum(distance(s, candidat) for s in sorties)
                if meilleur is None or total < meilleur:
                    meilleur, retenu = total, candidat
    return retenu


def transcrire(famille: str, modele: Path, audio: Path) -> tuple[str, float]:
    if famille == "whisper":
        commande = [MOTEUR / "whisper-cli.exe", "-m", modele, "-l", "fr", "-t", str(FILS),
                    "--temperature", "0", "--no-timestamps", "--no-prints",
                    "--vad", "-vm", VAD, "-f", audio]
    else:
        commande = [MOTEUR / "parakeet-cli.exe", "-m", modele, "-t", str(FILS), "--no-prints", "-f", audio]
    debut = time.perf_counter()
    resultat = subprocess.run([str(c) for c in commande], capture_output=True)
    duree = time.perf_counter() - debut
    if resultat.returncode != 0:
        sys.exit(f"echec de {modele.name} sur {audio.name} : {resultat.stderr.decode('utf-8', 'replace')[-400:]}")
    return resultat.stdout.decode("utf-8", "replace"), duree


def nom_lisible(brut: str) -> str:
    """« Intel(R) Core(TM) i5-9400F CPU @ 2.90GHz » devient « Intel Core i5-9400F ».

    Le nom brut est ecrit pour des techniciens ; il s'affiche dans une interface destinee a des
    gens qui ne le sont pas, et la frequence n'y apprend rien a personne.
    """
    nom = re.sub(r"\((R|TM|C)\)", "", brut, flags=re.I)
    nom = re.sub(r"\s+CPU\b|\s*@.*$|\s+\d+-Core Processor", "", nom)
    return re.sub(r"\s+", " ", nom).strip()


def processeur() -> str:
    try:
        cle = winreg.OpenKey(winreg.HKEY_LOCAL_MACHINE, r"HARDWARE\DESCRIPTION\System\CentralProcessor\0")
        return nom_lisible(winreg.QueryValueEx(cle, "ProcessorNameString")[0].strip())
    except OSError:
        return "processeur inconnu"


def main() -> None:
    analyseur = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    analyseur.add_argument("--modeles", type=Path, action="append", default=[],
                           help="repertoire supplementaire ou chercher les modeles")
    analyseur.add_argument("--passages", type=int, default=3)
    arguments = analyseur.parse_args()

    for requis in [MOTEUR / "whisper-cli.exe", MOTEUR / "parakeet-cli.exe", VAD]:
        if not requis.exists():
            sys.exit(f"manquant : {requis}\nLancer python scripts/preparer-moteur-embarque.py")

    repertoires = arguments.modeles + [
        Path(os.environ["LOCALAPPDATA"]) / "org.breizhzion.oyant.desktop" / "modeles"
    ]
    chemins = {}
    for identifiant, (famille, fichier) in MODELES.items():
        trouve = next((r / fichier for r in repertoires if (r / fichier).is_file()), None)
        if trouve is None:
            sys.exit(f"modele absent : {fichier}\nLe telecharger dans Oyant, ou passer --modeles.")
        chemins[identifiant] = (famille, trouve)

    sorties: dict[str, dict[str, list[str]]] = {i: {} for i in MODELES}
    durees: dict[str, list[float]] = {i: [] for i in MODELES}
    for voix, url, debut, source in VOIX:
        mp3 = telecharger(url, TRAVAIL / f"{voix}.mp3")
        wav = TRAVAIL / f"{voix}-{debut}-{DUREE}.wav"
        if not wav.exists():
            subprocess.run(["ffmpeg", "-loglevel", "error", "-y", "-ss", str(debut), "-t", str(DUREE),
                            "-i", str(mp3), "-ar", "16000", "-ac", "1", "-c:a", "pcm_s16le", str(wav)], check=True)
        for identifiant, (famille, modele) in chemins.items():
            for _ in range(arguments.passages):
                texte, duree = transcrire(famille, modele, wav)
                durees[identifiant].append(duree)
            sorties[identifiant][voix] = mots(texte)
            print(f"  {voix:22} {identifiant:12} {duree:6.1f} s")

    erreurs: dict[str, dict[str, float]] = {i: {} for i in MODELES}
    for voix, _, _, source in VOIX:
        livre = mots(texte_source(source))
        ref = reference(livre, [sorties[i][voix] for i in MODELES])
        print(f"  reference {voix} : {len(ref)} mots, « {' '.join(ref[:8])} … {' '.join(ref[-5:])} »")
        for identifiant in MODELES:
            erreurs[identifiant][voix] = 100 * distance(sorties[identifiant][voix], ref) / len(ref)

    resultat = {
        "mesure": {
            "date": datetime.date.today().isoformat(),
            "machine": f"un {processeur()} ({FILS} cœurs), sans carte graphique",
            "corpus": f"{len(VOIX)} lectures LibriVox en français, {DUREE} s chacune",
            "script": "scripts/mesurer-modeles.py",
        },
        "modeles": {
            identifiant: {
                # Combien de fois plus vite que la parole elle-meme : 10 veut dire 3 s pour 30 s.
                "fois_plus_rapide_que_la_parole": round(DUREE / statistics.median(durees[identifiant]), 1),
                "erreurs_pour_cent": round(statistics.mean(erreurs[identifiant].values()), 1),
                "erreurs_pour_cent_pire_voix": round(max(erreurs[identifiant].values()), 1),
            }
            for identifiant in MODELES
        },
    }
    SORTIE.write_text(json.dumps(resultat, ensure_ascii=False, indent=2) + "\n", encoding="utf-8")
    print(json.dumps(resultat, ensure_ascii=False, indent=2))


if __name__ == "__main__":
    main()
