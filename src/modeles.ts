// Ecran des modeles : telecharger, verifier, supprimer.
//
// ⛔ **Present et verifie ne sont pas la meme chose, et l'ecran ne doit jamais les confondre.** Un
// modele tronque par une coupure reseau porte un nom parfaitement normal et fait une taille
// plausible. S'il s'affichait « pret », la panne se manifesterait bien plus tard, a la premiere
// dictee, par un message du moteur qui ne parlerait pas de telechargement.
//
// ⚠️ L'etat est ecrit en toutes lettres et pas seulement colore : WCAG 1.4.1 interdit de faire
// porter une information a la seule couleur, et « vert contre ambre » serait illisible pour une
// partie des gens.

import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

export type EtatModele = {
  identifiant: string;
  nom: string;
  description: string;
  taille_attendue: number;
  octets_locaux: number;
  complet: boolean;
  verifie: boolean;
  reprise_possible: boolean;
  /** C'est ce modèle qui sert aux transcriptions. */
  actif: boolean;
  langues: string;
  mesure: Mesure | null;
};

type Mesure = {
  fois_plus_rapide_que_la_parole: number;
  erreurs_pour_cent: number;
  erreurs_pour_cent_pire_voix: number;
};

type ContexteMesures = { date: string; machine: string; corpus: string; script: string };

let contexte: ContexteMesures | null = null;

/** Nombre a la francaise : virgule decimale, et pas de « ,0 » inutile. */
function nombre(valeur: number): string {
  return valeur.toLocaleString('fr-FR', { maximumFractionDigits: 1 });
}

/**
 * Le temps de transcription d'une minute dictee, tire de la vitesse mesuree.
 *
 * ⚠️ Affiche en duree et pas en « × la parole » : « 0,7 × » ne dit a personne que Large v3 met
 * plus d'une minute a transcrire une minute, alors que « 1 min 26 s » le dit tout de suite.
 */
function pourUneMinute(foisPlusRapide: number): string {
  const secondes = Math.round(60 / foisPlusRapide);
  if (secondes < 60) return `${secondes} s`;
  const reste = secondes % 60;
  return reste === 0 ? `${secondes / 60} min` : `${Math.floor(secondes / 60)} min ${reste} s`;
}

/**
 * Le tableau qui compare les modeles cote a cote : taille, langues, vitesse, erreurs.
 *
 * ⛔ **Les chiffres viennent de `src-tauri/mesures-modeles.json`, ecrit par
 * `scripts/mesurer-modeles.py`**, jamais tapes ici. La legende dit sur quelle machine et quel
 * corpus ils ont ete mesures : une vitesse sans sa machine ne veut rien dire.
 *
 * ⚠️ **La colonne « pire voix » n'est pas un detail.** Parakeet est le plus juste en moyenne sur
 * deux voix et rend un texte inutilisable sur la troisieme : une moyenne seule cacherait
 * exactement le risque que quelqu'un doit connaitre avant de choisir.
 *
 * ⚠️ Un vrai `<table>` avec `scope` et `<caption>`, pas des `div` alignes : un lecteur d'ecran
 * annonce alors l'en-tete de colonne a chaque cellule. Le conteneur defile horizontalement si la
 * fenetre est etroite, et il est atteignable au clavier pour pouvoir le faire defiler.
 */
function tableauComparatif(): HTMLElement {
  const conteneur = document.createElement('div');
  conteneur.className = 'comparatif';
  conteneur.tabIndex = 0;
  conteneur.setAttribute('role', 'region');
  conteneur.setAttribute('aria-label', 'Comparaison des modèles');

  const tableau = document.createElement('table');
  const legende = document.createElement('caption');
  legende.textContent = 'Comparer les modèles';
  tableau.append(legende);

  const tete = tableau.createTHead().insertRow();
  for (const titre of ['Modèle', 'Taille', 'Langues', '1 min dictée', 'Erreurs', 'Pire voix']) {
    const th = document.createElement('th');
    th.scope = 'col';
    th.textContent = titre;
    tete.append(th);
  }

  const corps = tableau.createTBody();
  for (const etat of etats) {
    const ligne = corps.insertRow();
    if (etat.actif) ligne.className = 'comparatif-actif';
    const nom = document.createElement('th');
    nom.scope = 'row';
    nom.textContent = etat.actif ? `${etat.nom} (utilisé)` : etat.nom;
    ligne.append(nom);
    const m = etat.mesure;
    for (const valeur of [
      taille(etat.taille_attendue),
      etat.langues,
      m ? pourUneMinute(m.fois_plus_rapide_que_la_parole) : 'non mesuré',
      m ? `${nombre(m.erreurs_pour_cent)} %` : 'non mesuré',
      m ? `${nombre(m.erreurs_pour_cent_pire_voix)} %` : 'non mesuré',
    ]) {
      ligne.insertCell().textContent = valeur;
    }
  }
  conteneur.append(tableau);

  if (contexte) {
    const note = document.createElement('p');
    note.className = 'comparatif-note';
    // ⚠️ La vitesse est celle du PROCESSEUR seul : avec une carte graphique, tout va plus vite,
    // et le dire evite de croire qu'un modele lent le restera.
    note.textContent =
      `« 1 min dictée » : temps de transcription d’une minute de parole. ` +
      `Erreurs : part des mots faux, manquants ou en trop, comparés au texte lu, en moyenne puis ` +
      `sur la voix la plus difficile. ` +
      `Mesuré le ${new Date(contexte.date).toLocaleDateString('fr-FR')} sur ${contexte.machine}, ` +
      `${contexte.corpus}. Avec une carte graphique, tout va plus vite.`;
    conteneur.append(note);
  }
  return conteneur;
}

type Progression = { identifiant: string; recus: number; total: number };

let etats: EtatModele[] = [];
const enCours = new Set<string>();

/** Taille lisible. Les modeles pesent des centaines de Mo, l'octet ne dit rien a personne. */
function taille(octets: number): string {
  const mo = octets / 1_000_000;
  // Virgule decimale : « 1.53 Go » est une ecriture anglaise dans une interface francaise.
  if (mo >= 1000) return `${(mo / 1000).toLocaleString('fr-FR', { minimumFractionDigits: 2, maximumFractionDigits: 2 })} Go`;
  return `${Math.round(mo)} Mo`;
}

function dire(texte: string, erreur = false): void {
  const zone = document.getElementById('message-modeles');
  if (!zone) return;
  zone.textContent = texte;
  zone.classList.toggle('message-erreur', erreur);
}

/**
 * Le mot qui qualifie l'etat du modele.
 *
 * ⚠️ « Téléchargé » et « Vérifié » sont deux choses differentes et le restent a l'ecran. Les
 * fondre en un seul « Prêt » ferait exactement la promesse qu'on ne peut pas tenir.
 */
function libelleEtat(etat: EtatModele): string {
  if (enCours.has(etat.identifiant)) return 'Téléchargement…';
  if (etat.verifie) return 'Vérifié';
  if (etat.complet) return 'Téléchargé, non vérifié';
  if (etat.reprise_possible) {
    const part = Math.floor((etat.octets_locaux / etat.taille_attendue) * 100);
    return `Interrompu à ${part} %`;
  }
  return 'Absent';
}

function bouton(libelle: string, action: () => void, principal = false): HTMLButtonElement {
  const b = document.createElement('button');
  b.type = 'button';
  b.className = principal ? 'action action-principale' : 'action';
  b.textContent = libelle;
  b.addEventListener('click', action);
  return b;
}

async function rafraichir(): Promise<void> {
  etats = await invoke<EtatModele[]>('etat_modeles');
  dessiner();
}

async function agir(action: () => Promise<unknown>, quoi: string): Promise<void> {
  dire('');
  try {
    await action();
  } catch (erreur) {
    // ⛔ Jamais en silence. Une empreinte qui ne correspond pas est précisément le message le
    // plus utile que ce module puisse produire.
    dire(`${quoi} : ${String(erreur)}`, true);
    console.error(quoi, erreur);
  }
}

function dessiner(): void {
  const hote = document.getElementById('modeles');
  if (!hote) return;
  hote.textContent = '';
  hote.append(tableauComparatif());

  // ⛔ **Le modele se choisit ICI et nulle part ailleurs** (arbitre par painteau le 2026-09-18).
  // Il y avait en plus une liste deroulante dans les reglages : painteau a dicte sans savoir avec
  // quel modele, parce que la selection n'etait pas la ou on telecharge. Meme motif que les
  // moteurs, et meme mise en avant, pour qu'une liste ne se lise pas autrement que sa voisine.
  const groupe = document.createElement('fieldset');
  groupe.className = 'choix';
  hote.append(groupe);

  for (const etat of etats) {
    const bloc = document.createElement('div');
    bloc.className = 'modele choix-option';
    bloc.id = `modele-${etat.identifiant}`;

    const radio = document.createElement('input');
    radio.type = 'radio';
    radio.name = 'modele';
    radio.id = `modele-choix-${etat.identifiant}`;
    radio.value = etat.identifiant;
    radio.checked = etat.actif;
    radio.disabled = enCours.size > 0;
    radio.addEventListener('change', () => {
      if (!radio.checked) return;
      void agir(async () => {
        // ⚠️ Cocher un modele absent le TELECHARGE, comme pour les moteurs : la question posee
        // est « avec quoi je transcris », pas « quel fichier je gere ».
        dire(`${etat.nom} : préparation…`);
        etats = await invoke<EtatModele[]>('choisir_modele', { identifiant: etat.identifiant });
        dessiner();
        dire(`${etat.nom} est maintenant utilisé.`);
      }, 'Choix du modèle');
    });
    bloc.append(radio);

    // L'etiquette enveloppe tout le contenu : la cible cliquable est la carte entiere.
    const etiquette = document.createElement('label');
    etiquette.htmlFor = radio.id;

    const entete = document.createElement('div');
    entete.className = 'modele-entete';

    const nom = document.createElement('span');
    nom.className = 'modele-nom';
    nom.textContent = etat.nom;

    const marque = document.createElement('span');
    marque.className = 'modele-etat';
    marque.dataset.etat = etat.verifie
      ? 'verifie'
      : etat.complet
        ? 'present'
        : etat.reprise_possible
          ? 'partiel'
          : 'absent';
    // ⚠️ « Utilisé » passe DEVANT l'état de téléchargement : c'est la question qu'on se pose en
    // regardant la liste, et « Vérifié » sur trois lignes ne dit pas laquelle sert.
    marque.textContent = etat.actif ? 'Utilisé' : libelleEtat(etat);
    entete.append(nom, marque);

    const description = document.createElement('p');
    description.className = 'modele-description';
    description.textContent = `${etat.description} ${taille(etat.taille_attendue)}.`;

    etiquette.append(entete, description);
    bloc.append(etiquette);

    // Barre de progression : un element natif, donc annonce et compris sans rien reconstruire.
    const barre = document.createElement('progress');
    barre.className = 'progression';
    barre.id = `progression-${etat.identifiant}`;
    barre.max = etat.taille_attendue;
    barre.value = enCours.has(etat.identifiant) ? etat.octets_locaux : 0;
    barre.hidden = !enCours.has(etat.identifiant);
    barre.setAttribute('aria-label', `Téléchargement de ${etat.nom}`);
    bloc.append(barre);  // hors de l'etiquette : cliquer une action ne doit pas cocher le radio

    const actions = document.createElement('div');
    actions.className = 'modele-actions';

    if (enCours.has(etat.identifiant)) {
      actions.append(
        bouton('Arrêter', () => {
          void agir(
            () => invoke('interrompre_modele', { identifiant: etat.identifiant }),
            'Arrêt',
          );
        }),
      );
    } else {
      if (!etat.complet) {
        actions.append(
          bouton(
            etat.reprise_possible ? 'Reprendre' : 'Télécharger',
            () => void telecharger(etat.identifiant),
            true,
          ),
        );
      }
      if (etat.complet) {
        actions.append(
          bouton(
            'Vérifier',
            () =>
              void agir(async () => {
                dire(`Vérification de ${etat.nom} en cours…`);
                try {
                  await invoke('verifier_modele', { identifiant: etat.identifiant });
                  dire(`${etat.nom} : empreinte conforme.`);
                } finally {
                  // ⛔ `finally` : sur ECHEC aussi. Le coeur retire la marque `.verifie` quand
                  // l'empreinte ne correspond pas, donc sans cette relecture l'écran continuerait
                  // d'afficher « Vérifié » juste au-dessus du message disant que le fichier est
                  // abîmé. Défaut constaté le 2026-09-17 sur une capture, invisible au typage.
                  await rafraichir();
                }
              }, 'Vérification'),
          ),
        );
      }
      if (etat.complet || etat.reprise_possible) {
        actions.append(
          bouton(
            'Supprimer',
            () =>
              void agir(async () => {
                await invoke('supprimer_modele', { identifiant: etat.identifiant });
                await rafraichir();
              }, 'Suppression'),
          ),
        );
      }
    }

    bloc.append(actions);
    groupe.append(bloc);
  }
}

async function telecharger(identifiant: string): Promise<void> {
  enCours.add(identifiant);
  dessiner();
  await agir(async () => {
    try {
      await invoke('telecharger_modele', { identifiant });
    } finally {
      // ⚠️ `finally` : sur interruption ou sur erreur, l'écran doit cesser d'annoncer un
      // téléchargement en cours. Sans ça il resterait bloqué sur « Téléchargement… » et le seul
      // recours serait de relancer l'application.
      enCours.delete(identifiant);
      await rafraichir();
    }
  }, 'Téléchargement');
}

/**
 * Remplit la liste des modeles de l'ecran de reglages depuis le CATALOGUE.
 *
 * ⚠️ Recopier cette liste cote interface la ferait diverger du catalogue : on proposerait alors
 * un modele que rien ne sait telecharger, exactement ce que faisait l'ancienne version en
 * offrant Parakeet qui n'a jamais existe sur le bureau.
 */
export function optionsDeModeles(): Array<[string, string]> {
  return etats.map((e) => [e.identifiant, e.nom]);
}

export async function chargerModeles(): Promise<void> {
  etats = await invoke<EtatModele[]>('etat_modeles');
}

// ── Sur quoi l'ordinateur calcule ──────────────────────────────────────────────────────────────
//
// ⛔ **C'est un CHOIX, pas une liste a gerer** (arbitre par painteau le 2026-09-17). On coche
// une option, Oyant installe ce qu'il faut et s'en sert. La question posee a l'utilisateur est
// « sur quoi votre ordinateur calcule-t-il », pas « quel binaire voulez-vous administrer ».
//
// ⚠️ Une option indisponible est montree quand meme, avec sa raison, contrairement au menu
// de l'icone : quelqu'un avec une carte AMD a besoin de savoir si elle est prise en charge, et
// le silence le ferait chercher une option qui n'existe pas.

type EtatMoteur = {
  identifiant: string;
  nom: string;
  description: string;
  taille_attendue: number;
  installe: boolean;
  embarque: boolean;
  actif: boolean;
  acceleration_chargee: string[];
  acceleration_confirmee: boolean | null;
  disponible: boolean;
  indisponible_parce_que: string;
  chemin: string;
};

let moteurs: EtatMoteur[] = [];
let installationEnCours: string | null = null;

function libelleMoteur(m: EtatMoteur): string {
  if (installationEnCours === m.identifiant) return 'Installation…';
  if (!m.disponible) return 'Indisponible';
  // ⛔ Une accélération promise qui ne se charge pas doit se voir AVANT tout le reste : c'est le
  // seul cas où Oyant fonctionne tout en faisant le contraire de ce que l'écran annonce.
  if (m.installe && m.acceleration_confirmee === false) return 'Ne démarre pas';
  if (m.actif) return 'Utilisé';
  if (m.installe) return m.embarque ? 'Fourni' : 'Installé';
  // ⚠️ Un moteur embarque n'annonce AUCUNE taille : il n'y a rien a telecharger, et « 0 Mo »
  // serait un chiffre faux affiche a la place d'une information utile.
  return m.embarque ? 'Fourni' : `À télécharger, ${taille(m.taille_attendue)}`;
}

/**
 * Ce que la MESURE dit, à afficher sous le choix actif.
 *
 * ⛔ Jamais une déclaration. Le 2026-09-17, une archive CUDA parfaitement téléchargée et vérifiée
 * par son empreinte était incapable de charger son accélération, faute d'une bibliothèque qu'elle
 * ne contenait pas : whisper.cpp retombait sur le processeur sans un mot, et le seul symptôme
 * était un temps de transcription qui ne descendait pas.
 */
function mesureMoteur(m: EtatMoteur): string {
  if (!m.installe) return '';
  const charges = m.acceleration_chargee.join(', ') || 'aucun';
  if (m.acceleration_chargee.length === 0) {
    // ⛔ Zero dos d'execution charge veut dire que le moteur n'a pas demarre du tout. La cause
    // la plus frequente est une bibliotheque systeme absente (`vcomp140.dll`, livree par le
    // redistribuable Visual C++), que whisper.cpp n'embarque pas et que Windows ne nomme pas
    // dans son message d'erreur.
    return 'Le programme de transcription ne démarre pas sur cette machine. Il manque sans doute le composant « Visual C++ Redistributable » de Microsoft.';
  }
  if (m.acceleration_confirmee === false) {
    return `L’accélération ne se charge pas sur cette machine : le calcul retombe sur le processeur. Mesuré, chargé : ${charges}.`;
  }
  return `Mesuré à l’instant, chargé : ${charges}.`;
}

function dessinerMoteurs(): void {
  const hote = document.getElementById('moteurs');
  if (!hote) return;
  hote.textContent = '';

  if (moteurs.length === 0) {
    const vide = document.createElement('p');
    vide.className = 'modele-description';
    vide.textContent =
      'Aucun programme de transcription n’est encore disponible pour cette plateforme.';
    hote.append(vide);
    return;
  }

  // `fieldset` + boutons radio natifs : le groupe est annonce, les fleches naviguent, l'etat
  // coche est porte par le navigateur. Rien a reconstruire.
  const groupe = document.createElement('fieldset');
  groupe.className = 'choix';

  for (const m of moteurs) {
    const ligne = document.createElement('div');
    ligne.className = 'choix-option';
    ligne.id = `moteur-${m.identifiant}`;

    const radio = document.createElement('input');
    radio.type = 'radio';
    radio.name = 'calcul';
    radio.id = `calcul-${m.identifiant}`;
    radio.value = m.identifiant;
    radio.checked = m.actif;
    radio.disabled = !m.disponible || installationEnCours !== null;
    radio.setAttribute('aria-describedby', `calcul-${m.identifiant}-aide`);

    const etiquette = document.createElement('label');
    etiquette.htmlFor = radio.id;

    const entete = document.createElement('span');
    entete.className = 'choix-entete';
    const nom = document.createElement('span');
    nom.className = 'reglage-titre';
    nom.textContent = m.nom;
    const etat = document.createElement('span');
    etat.className = 'modele-etat';
    etat.dataset.etat = m.actif ? 'verifie' : m.disponible ? 'present' : 'absent';
    etat.textContent = libelleMoteur(m);
    entete.append(nom, etat);

    const aide = document.createElement('span');
    aide.className = 'reglage-aide';
    aide.id = `calcul-${m.identifiant}-aide`;
    // La raison d'indisponibilite REMPLACE la description : c'est elle qu'il faut lire.
    aide.textContent = m.disponible ? m.description : m.indisponible_parce_que;

    etiquette.append(entete, aide);

    const mesure = mesureMoteur(m);
    if (mesure) {
      const ligne = document.createElement('span');
      ligne.className =
        m.acceleration_confirmee === false ? 'reglage-aide mesure-alerte' : 'reglage-aide';
      ligne.textContent = mesure;
      etiquette.append(ligne);
    }

    const barre = document.createElement('progress');
    barre.className = 'progression';
    barre.id = `progression-moteur-${m.identifiant}`;
    barre.max = m.taille_attendue || 1;
    barre.value = 0;
    barre.hidden = installationEnCours !== m.identifiant;
    barre.setAttribute('aria-label', `Installation de ${m.nom}`);
    etiquette.append(barre);

    radio.addEventListener('change', () => {
      if (!radio.checked) return;
      void choisirMoteur(m);
    });

    ligne.append(radio, etiquette);
    groupe.append(ligne);
  }

  hote.append(groupe);
}

async function choisirMoteur(m: EtatMoteur): Promise<void> {
  installationEnCours = m.installe ? null : m.identifiant;
  dessinerMoteurs();
  await agir(async () => {
    try {
      if (!m.installe) dire(`Téléchargement de ${taille(m.taille_attendue)} en cours…`);
      moteurs = await invoke<EtatMoteur[]>('choisir_moteur', { identifiant: m.identifiant });

      // ⛔ On fait compiler ses noyaux a l'acceleration MAINTENANT. Sans ca, ce cout unique
      // (17,7 s mesurees sur une GTX 1060 le 2026-09-17, contre 1,8 s ensuite) tomberait sur la
      // premiere dictee, celle qui donne son impression du produit. Ici l'utilisateur vient
      // d'attendre un telechargement, une poignee de secondes de plus ne le surprend pas.
      dire('Préparation de la carte graphique, une seule fois…');
      const prechauffe = await invoke<boolean>('prechauffer', { identifiant: m.identifiant });

      dire(
        prechauffe
          ? `Oyant calcule maintenant sur : ${m.nom.toLowerCase()}. La carte est prête.`
          : `Oyant calcule maintenant sur : ${m.nom.toLowerCase()}.`,
      );
    } finally {
      // ⚠️ `finally` : sur echec aussi, l'ecran doit revenir a l'etat REEL. Sinon la
      // radio resterait cochee sur un choix qui n'a pas ete enregistre, et la barre de
      // progression resterait a l'ecran.
      installationEnCours = null;
      moteurs = await invoke<EtatMoteur[]>('etat_moteurs');
      dessinerMoteurs();
    }
  }, 'Choix du calcul');
}

export async function brancherMoteurs(): Promise<void> {
  try {
    moteurs = await invoke<EtatMoteur[]>('etat_moteurs');
    dessinerMoteurs();
  } catch (erreur) {
    dire('L’état du programme de transcription n’a pas pu être lu.', true);
    console.error('etat_moteurs', erreur);
    return;
  }

  await listen<Progression>('moteur-progression', (evenement) => {
    const { identifiant, recus, total } = evenement.payload;
    const barre = document.getElementById(
      `progression-moteur-${identifiant}`,
    ) as HTMLProgressElement | null;
    if (barre) {
      barre.hidden = false;
      barre.max = total;
      barre.value = recus;
    }
    const etat = document
      .getElementById(`moteur-${identifiant}`)
      ?.querySelector('.modele-etat') as HTMLElement | null;
    if (etat) etat.textContent = `${taille(recus)} sur ${taille(total)}`;
  });
}

export async function brancherModeles(): Promise<void> {
  // ⚠️ On relit ICI plutôt que de compter sur le chargement fait par l'écran de réglages : les
  // deux branchements partent en parallèle, donc s'appuyer sur l'ordre laissait la liste VIDE une
  // fois sur deux. Défaut invisible au typage et sans message, vu seulement sur une capture.
  try {
    // Avant la liste : le tableau se dessine avec elle, et sa légende en dépend.
    contexte = await invoke<ContexteMesures | null>('contexte_mesures');
    await rafraichir();
  } catch (erreur) {
    dire('La liste des modèles n’a pas pu être lue.', true);
    console.error('etat_modeles', erreur);
    return;
  }

  await listen<Progression>('modele-progression', (evenement) => {
    const { identifiant, recus } = evenement.payload;
    const barre = document.getElementById(`progression-${identifiant}`) as HTMLProgressElement | null;
    if (barre) {
      barre.hidden = false;
      barre.value = recus;
    }
    const marque = document
      .getElementById(`modele-${identifiant}`)
      ?.querySelector('.modele-etat') as HTMLElement | null;
    if (marque) {
      const etat = etats.find((e) => e.identifiant === identifiant);
      const total = etat?.taille_attendue ?? evenement.payload.total;
      marque.textContent = `${taille(recus)} sur ${taille(total)}`;
    }
  });
}
