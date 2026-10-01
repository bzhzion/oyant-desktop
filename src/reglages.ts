// Ecran de reglages : l'ecran sur lequel on tombe au premier lancement.
//
// ⚠️ Les champs sont **generes depuis une description declarative** et pas ecrits un par un en
// HTML. Le motif n'est pas l'economie de lignes : c'est que l'etiquette liee, le texte d'aide
// rattache par `aria-describedby` et la taille de cible sont alors **identiques partout par
// construction**. Trente blocs recopies a la main, c'est trente occasions d'oublier un `for`, et
// un champ sans etiquette ne se voit pas a l'ecran.
//
// ⚠️ Les cles sont celles du FICHIER, donc en anglais (voir `src-tauri/src/reglages.rs`). Les
// libelles, eux, sont en francais : c'est l'interface.

import { invoke } from '@tauri-apps/api/core';

import { brancherHistorique, brancherSubstitutions, brancherVocabulaire } from './texte';

/**
 * Un remplacement automatique.
 *
 * ⚠️ Les clés sont celles du fichier de réglages, en anglais, jamais traduites : le fichier est lu
 * par des gens et recopié d'une machine à l'autre.
 */
export type Substitution = { from: string; to: string; case_sensitive: boolean };

export type Reglages = {
  hotkey: string;
  microphone: string;
  min_duration_ms: number;
  max_duration_s: number;
  silence_threshold: number;
  start_beep: boolean;
  end_beep: boolean;
  beep_frequency_hz: number;
  beep_duration_ms: number;
  pause_media: boolean;

  model: string;
  language: string;
  threads: number;
  temperature: number;
  live_transcription: boolean;

  injection_delay_ms: number;
  auto_enter: boolean;
  leading_space: boolean;
  auto_capitalize: boolean;
  french_typography: boolean;
  copy_to_clipboard: boolean;
  substitutions: Substitution[];
  vocabulary: string[];

  notifications: boolean;
  history_size: number;

  auto_update: boolean;
  log_level: string;

  show_advanced: boolean;

  /** Accepter qu'un téléphone se connecte. ⛔ Faux par défaut : tant qu'il l'est, aucun port
   * n'est lié côté Rust. */
  network_enabled: boolean;
  /** Écouter au-delà de la boucle locale. ⚠️ Distinct du précédent à dessein : l'un ouvre le
   * service, l'autre le rend joignable. */
  network_all_interfaces: boolean;
};

type Base = { cle: keyof Reglages; titre: string; aide?: string; avance?: boolean };
type Champ =
  | (Base & { type: 'bool' })
  | (Base & { type: 'nombre'; min: number; max: number; pas?: number; unite?: string })
  | (Base & { type: 'choix'; options: Array<[string, string]> })
  | (Base & { type: 'texte' });

/** `conteneur` est l'ID du `div` de l'onglet ou ce groupe doit s'afficher. ⚠️ Un groupe par
 * conteneur aujourd'hui, mais rien n'empeche plusieurs groupes de partager le meme onglet : c'est
 * le cas d'aucun aujourd'hui, mais la forme le permettrait sans y retoucher. */
type Groupe = { titre: string; conteneur: string; champs: Champ[] };

// ⚠️ Cette liste EST l'inventaire des reglages : elle fait autorite, et le test
// `reglages_utilises` echoue si un reglage y apparait sans rien piloter. Ce qui demande
// d'enumerer le systeme (liste des microphones, modeles reellement telecharges) n'y entre qu'une
// fois le code capable de le faire : afficher une liste vide en attendant serait pire que ne rien
// afficher.
const GROUPES: Groupe[] = [
  {
    titre: 'Capture',
    conteneur: 'reglages-capture',
    champs: [
      {
        cle: 'hotkey',
        type: 'texte',
        titre: 'Raccourci global',
        aide: 'Les touches à maintenir enfoncées pour dicter. Oyant écoute tant que vous les gardez appuyées.',
      },
      {
        cle: 'microphone',
        type: 'choix',
        titre: 'Microphone',
        // ⚠️ Rempli a l'execution en interrogeant le systeme, jamais ecrit ici : une liste de
        // materiel recopiee dans le code serait fausse sur toutes les machines sauf une.
        options: [],
        aide: 'Celui qu’Oyant écoute. « Celui du système » suit automatiquement votre casque quand vous le branchez.',
      },
      {
        cle: 'min_duration_ms',
        avance: true,
        type: 'nombre',
        titre: 'Durée minimale',
        unite: 'ms',
        min: 0,
        max: 5000,
        pas: 50,
        aide: 'Un appui plus court que cette durée est ignoré, pour ne pas déclencher la dictée par erreur.',
      },
      {
        cle: 'max_duration_s',
        type: 'nombre',
        titre: 'Durée maximale',
        unite: 's',
        min: 5,
        max: 3600,
        pas: 5,
        aide: 'Au-delà, l’enregistrement s’arrête tout seul.',
      },
      {
        cle: 'silence_threshold',
        avance: true,
        type: 'nombre',
        titre: 'Seuil de silence',
        min: 0,
        max: 1,
        pas: 0.005,
        aide: 'À partir de quel niveau sonore Oyant considère que vous ne parlez plus. Plus la valeur est basse, plus il est sensible.',
      },
      {
        cle: 'start_beep',
        type: 'bool',
        titre: 'Bip au début',
        aide: 'Un son court confirme qu’Oyant s’est mis à écouter.',
      },
      {
        cle: 'end_beep',
        type: 'bool',
        titre: 'Bip à la fin',
        aide: 'Un son court confirme que l’écoute est terminée.',
      },
      {
        cle: 'beep_frequency_hz',
        avance: true,
        type: 'nombre',
        titre: 'Hauteur du bip',
        unite: 'Hz',
        min: 100,
        max: 8000,
        pas: 10,
        aide: 'Plus le nombre est grand, plus le bip est aigu.',
      },
      {
        cle: 'beep_duration_ms',
        avance: true,
        type: 'nombre',
        titre: 'Durée du bip',
        unite: 'ms',
        min: 10,
        max: 1000,
        pas: 10,
        aide: 'Combien de temps le bip dure.',
      },
    ],
  },
  // ⚠️ « Mettre les médias en pause » et « Transcription en direct » ont été RETIRÉS de l'écran :
  // ils ne pilotaient rien. Ils restent dans le fichier de réglages, donc aucune configuration
  // n'est cassée, et ils reviendront le jour où quelque chose les lit.
  {
    titre: 'Transcription',
    conteneur: 'reglages-transcription',
    champs: [
      {
        cle: 'language',
        type: 'choix',
        titre: 'Langue',
        options: [
          ['auto', 'Détection automatique'],
          ['fr', 'Français'],
          ['en', 'Anglais'],
          ['ko', 'Coréen'],
          ['pt', 'Portugais'],
          ['es', 'Espagnol'],
          ['de', 'Allemand'],
        ],
        aide: 'La langue que vous allez parler. En détection automatique, Oyant la devine au début de chaque dictée.',
      },
      {
        cle: 'threads',
        avance: true,
        type: 'nombre',
        titre: 'Cœurs utilisés',
        min: 1,
        max: 64,
        pas: 1,
        aide: 'Combien de cœurs du processeur travaillent à la transcription. Plus il y en a, plus c’est rapide, mais l’ordinateur est plus chargé.',
      },
      {
        cle: 'temperature',
        avance: true,
        type: 'nombre',
        titre: 'Marge d’interprétation',
        min: 0,
        max: 1,
        pas: 0.05,
        aide: 'À 0, Oyant reste au plus près de ce qu’il a entendu. Plus haut, il s’autorise à deviner quand il hésite.',
      },
    ],
  },
  {
    titre: 'Sortie',
    conteneur: 'reglages-sortie',
    champs: [
      {
        cle: 'injection_delay_ms',
        avance: true,
        type: 'nombre',
        titre: 'Pause entre les touches',
        unite: 'ms',
        min: 0,
        max: 2000,
        pas: 10,
        // ⛔ Le libellé disait « délai AVANT écriture » alors que le code attend après CHAQUE
        // caractère. Un réglage qui décrit autre chose que ce qu'il fait se tourne dans le
        // mauvais sens en croyant bien faire.
        aide: 'Le temps qu’Oyant attend entre chaque touche. Laissez 0 sauf si une application perd des caractères quand le texte arrive trop vite, comme certains terminaux ou machines virtuelles. Attention : 50 ms sur une phrase de cent caractères, c’est cinq secondes.',
      },
      {
        cle: 'auto_enter',
        type: 'bool',
        titre: 'Valider par Entrée',
        aide: 'Appuie sur Entrée après le texte, comme pour envoyer un message.',
      },
      {
        cle: 'leading_space',
        avance: true,
        type: 'bool',
        titre: 'Espace avant le texte',
        aide: 'Ajoute une espace devant, utile quand le curseur est collé au mot précédent.',
      },
      {
        cle: 'auto_capitalize',
        type: 'bool',
        titre: 'Majuscule en début de phrase',
        aide: 'Met une majuscule au premier mot de chaque phrase.',
      },
      {
        cle: 'french_typography',
        type: 'bool',
        titre: 'Typographie française',
        aide: 'Ajoute les espaces insécables que le français demande avant ? ! : et ;',
      },
      {
        cle: 'copy_to_clipboard',
        type: 'bool',
        titre: 'Copier dans le presse-papiers',
        aide: 'Place aussi le texte dans le presse-papiers, pour pouvoir le recoller ailleurs.',
      },
    ],
  },
  {
    titre: 'Interface',
    conteneur: 'reglages-application',
    champs: [
      {
        cle: 'notifications',
        type: 'bool',
        titre: 'Notifications',
        // ⛔ Le libellé promettait un message « quand une transcription se termine », ce que rien
        // ne faisait. Il dit maintenant ce que le réglage commande vraiment : le SEUL canal qui
        // atteint l'utilisateur quand la fenêtre est fermée, c'est-à-dire presque toujours.
        aide: 'Prévient par une notification du système quand une dictée échoue. C’est le seul moyen d’être averti quand cette fenêtre est fermée.',
      },
      {
        cle: 'history_size',
        type: 'nombre',
        titre: 'Dictées conservées',
        // ⛔ La borne basse est ZÉRO, qui veut dire « n'en garde aucune ». Elle valait 1, donc
        // l'historique ne pouvait pas être désactivé : le réglage acceptait 0 et le cœur le
        // ramenait à 1 en silence. Et mettre 0 EFFACE ce qui était gardé.
        min: 0,
        max: 100,
        pas: 1,
        aide: 'Combien de dictées récentes Oyant garde pour que vous puissiez les relire. 0 pour ne rien garder, ce qui efface aussi l’historique existant.',
      },
    ],
  },
  {
    titre: 'Téléphone',
    conteneur: 'reglages-telephone',
    champs: [
      {
        cle: 'network_enabled',
        type: 'bool',
        titre: 'Accepter les connexions d’un téléphone',
        // ⛔ Le libellé dit ce que ça OUVRE, pas ce que ça apporte. Une case qui promet un
        // confort sans nommer ce qu'elle autorise se coche sans y penser.
        aide: 'Permet à l’application Oyant sur votre téléphone de se connecter à cet ordinateur. Tant que c’est décoché, aucun port n’est ouvert. Chaque téléphone doit être accepté une fois, ici, avec un code à quatre chiffres. Cochez aussi la case suivante pour que votre téléphone puisse joindre cet ordinateur.',
      },
      {
        cle: 'network_all_interfaces',
        type: 'bool',
        titre: 'Autoriser depuis le réseau local',
        // ⚠️ Deux réglages et pas un, à dessein : le premier ouvre le service, le second le rend
        // joignable. Les fondre aurait fait qu'accepter les téléphones expose au réseau du même
        // geste, sans que personne ne l'ait demandé.
        //
        // ⛔ Mais PAS `avance: true`, bien qu'il porte un avertissement de sécurité : sans lui
        // AUCUN téléphone ne peut se connecter, et son absence ne produit pas d'erreur, seulement
        // une recherche qui ne trouve rien. Le cacher derrière la bascule des réglages avancés
        // rendait la fonctionnalité muette pour qui ne l'ouvre jamais, c'est-à-dire le cas normal.
        aide: 'Sans ceci, Oyant n’écoute que sur cet ordinateur et aucun téléphone ne peut le joindre. Ne l’activez que sur un réseau de confiance : jamais sur un wifi d’hôtel ou public.',
      },
    ],
  },
  // ⛔ **Le groupe « Maintenance » a été RETIRÉ de l'écran, pas supprimé du produit.**
  //
  // Il portait « Mises à jour automatiques », cochée par défaut, et « Niveau de journal ». Ni
  // l'une ni l'autre ne pilotait quoi que ce soit : il n'y a ni mise à jour ni journal. Une case
  // cochée qui promet de vous tenir à jour alors que rien n'existe derrière est le pire endroit
  // où laisser un réglage qui ment, puisque la promesse touche à la sécurité.
  //
  // Les deux champs restent dans le fichier de réglages, donc une configuration existante n'est
  // pas cassée, et ils reviendront ici le jour où quelque chose les lit. `reglages_utilises.rs`
  // les tient en attente avec leur étape.
];

let courants: Reglages | null = null;
let defauts: Reglages | null = null;
let minuterie: number | undefined;

/** Toutes les clés marquées « avancé », à plat. */
function clesAvancees(): Array<keyof Reglages> {
  return GROUPES.flatMap((g) => g.champs.filter((c) => c.avance).map((c) => c.cle));
}

/**
 * Applique le repli des réglages avancés, et **dit combien d'entre eux ne sont plus à leur
 * valeur par défaut**.
 *
 * ⛔ C'est le point qui rend ce repli acceptable. Cacher un réglage qu'on a modifié le rend
 * introuvable : l'application se comporterait autrement sans qu'aucun écran ne l'explique, et on
 * chercherait la cause partout sauf dans un champ replié. Le compte est donc affiché en clair
 * quand les avancés sont masqués.
 *
 * ⚠️ `hidden` et non `display: none` en CSS : l'attribut retire aussi l'élément de l'arbre
 * d'accessibilité, donc un champ replié n'est pas lu ni atteint au clavier. Masqué à l'oeil mais
 * navigable au clavier serait le pire des deux.
 */
function appliquerRepli(): void {
  if (!courants) return;
  const visible = courants.show_advanced;

  for (const cle of clesAvancees()) {
    document.getElementById(`ligne-${cle}`)?.toggleAttribute('hidden', !visible);
  }

  // Un groupe dont toutes les lignes sont repliées ne doit pas laisser sa légende orpheline.
  for (const groupe of GROUPES) {
    const bloc = document.getElementById(`groupe-${groupe.titre}`);
    if (!bloc) continue;
    const reste = groupe.champs.some((c) => !c.avance) || visible;
    bloc.toggleAttribute('hidden', !reste);
  }

  const note = document.getElementById('note-avances');
  if (!note) return;

  if (visible || !defauts) {
    note.textContent = '';
    note.toggleAttribute('hidden', true);
    return;
  }

  const modifies = clesAvancees().filter((cle) => courants![cle] !== defauts![cle]);
  if (modifies.length === 0) {
    note.textContent = '';
    note.toggleAttribute('hidden', true);
  } else {
    note.textContent =
      modifies.length === 1
        ? '1 réglage avancé n’est plus à sa valeur par défaut.'
        : `${modifies.length} réglages avancés ne sont plus à leur valeur par défaut.`;
    note.toggleAttribute('hidden', false);
  }
}

function dire(texte: string, erreur = false): void {
  const zone = document.getElementById('message-reglages');
  if (!zone) return;
  zone.textContent = texte;
  zone.classList.toggle('message-erreur', erreur);
}

/**
 * Applique des valeurs aux champs.
 *
 * ⚠️ Le champ qui a le focus est laissé tranquille. Le coeur renvoie les valeurs NORMALISEES
 * après écriture, et les réappliquer aveuglément pendant qu'on tape replacerait le curseur en fin
 * de champ à chaque frappe, ce qui rend la saisie inutilisable sans que la cause soit devinable.
 */
function peupler(valeurs: Reglages): void {
  for (const groupe of GROUPES) {
    for (const champ of groupe.champs) {
      const element = document.getElementById(`r-${champ.cle}`) as
        | HTMLInputElement
        | HTMLSelectElement
        | null;
      if (!element || element === document.activeElement) continue;

      const valeur = valeurs[champ.cle];
      if (champ.type === 'bool') {
        (element as HTMLInputElement).checked = Boolean(valeur);
      } else {
        element.value = String(valeur);
      }
    }
  }
}

async function enregistrer(): Promise<void> {
  if (!courants) return;
  try {
    // ⛔ On relit le fichier AVANT d'écrire, et on ne remplace que ce que CET écran pilote.
    // Sans ça, l'instantané chargé au démarrage écrase en silence tout réglage modifié depuis par
    // un autre écran : choisir « carte graphique NVIDIA » puis toucher n'importe quel réglage
    // remettait le calcul sur le processeur, sans un message. Constaté le 2026-09-17.
    const surDisque = await invoke<Reglages>('lire_reglages');
    const pilotes = new Set(GROUPES.flatMap((g) => g.champs).map((c) => c.cle));
    pilotes.add('show_advanced');
    for (const cle of Object.keys(surDisque) as Array<keyof Reglages>) {
      if (!pilotes.has(cle)) (courants[cle] as unknown) = surDisque[cle];
    }

    const ecrits = await invoke<Reglages>('ecrire_reglages', { reglages: courants });
    courants = ecrits;
    // Les valeurs rendues sont celles du DISQUE : si une borne a été appliquée, l'écran doit le
    // montrer plutôt que d'afficher une valeur que le fichier ne porte pas.
    peupler(ecrits);
    // Le compte des réglages avancés modifiés doit suivre chaque écriture, sinon il annoncerait
    // l'état d'avant.
    appliquerRepli();

    // ⚠️ Le raccourci se réenregistre TOUT DE SUITE auprès du système. Sans ça il ne changerait
    // qu'au prochain lancement, ce qui se lit comme « le réglage ne marche pas ». Et un raccourci
    // refusé (déjà pris par une autre application) doit le dire ici, au moment où on le choisit,
    // pas rester silencieusement inerte jusqu'à la première tentative de dictée.
    try {
      await invoke('appliquer_raccourci', { libelle: ecrits.hotkey });
    } catch (erreur) {
      dire(`Réglages enregistrés, mais le raccourci n'a pas pu être posé : ${String(erreur)}`, true);
      return;
    }

    // ⚠️ L'historique se redessine : passer le plafond a 0 l'efface cote coeur, et l'ecran
    // doit le montrer tout de suite plutot qu'au prochain lancement.
    await brancherHistorique();

    dire('Réglages enregistrés.');
    window.setTimeout(() => dire(''), 1600);
  } catch (erreur) {
    // ⛔ Jamais en silence : un réglage qu'on croit enregistré et qui ne l'est pas est la pire
    // des deux situations.
    dire(`Enregistrement impossible : ${String(erreur)}`, true);
    console.error('ecrire_reglages', erreur);
  }
}

function programmerEnregistrement(): void {
  window.clearTimeout(minuterie);
  // On attend la fin de la saisie : écrire le fichier à chaque frappe le réécrirait des dizaines
  // de fois pour un seul changement.
  minuterie = window.setTimeout(() => void enregistrer(), 400);
}

function champVersDom(champ: Champ): HTMLElement {
  const ligne = document.createElement('div');
  ligne.className = champ.type === 'bool' ? 'reglage' : 'reglage reglage-valeur';
  ligne.id = `ligne-${champ.cle}`;

  const id = `r-${champ.cle}`;
  const idAide = `${id}-aide`;

  const etiquette = document.createElement('label');
  etiquette.htmlFor = id;

  const titre = document.createElement('span');
  titre.className = 'reglage-titre';
  titre.textContent = champ.titre;
  etiquette.append(titre);

  if (champ.aide) {
    const aide = document.createElement('span');
    aide.className = 'reglage-aide';
    aide.id = idAide;
    aide.textContent = champ.aide;
    etiquette.append(aide);
  }

  let controle: HTMLInputElement | HTMLSelectElement;
  if (champ.type === 'choix') {
    const select = document.createElement('select');
    for (const [valeur, libelle] of champ.options) {
      const option = document.createElement('option');
      option.value = valeur;
      option.textContent = libelle;
      select.append(option);
    }
    controle = select;
  } else {
    const input = document.createElement('input');
    if (champ.type === 'bool') {
      input.type = 'checkbox';
      // Redessinée en interrupteur, mais c'est bien une case à cocher : voir le commentaire de
      // `.interrupteur` dans la feuille de style.
      input.className = 'interrupteur';
    } else if (champ.type === 'nombre') {
      input.type = 'number';
      input.min = String(champ.min);
      input.max = String(champ.max);
      input.step = String(champ.pas ?? 1);
      input.inputMode = 'decimal';
    } else {
      input.type = 'text';
      input.spellcheck = false;
    }
    controle = input;
  }

  controle.id = id;
  if (champ.aide) controle.setAttribute('aria-describedby', idAide);

  controle.addEventListener('input', () => {
    if (!courants) return;
    if (champ.type === 'bool') {
      (courants[champ.cle] as boolean) = (controle as HTMLInputElement).checked;
    } else if (champ.type === 'nombre') {
      const lu = Number((controle as HTMLInputElement).value);
      // Un champ numérique vidé donne NaN : on ignore la frappe intermédiaire plutôt que
      // d'écrire une valeur absurde dans le fichier.
      if (Number.isNaN(lu)) return;
      (courants[champ.cle] as number) = lu;
    } else {
      (courants[champ.cle] as string) = controle.value;
    }
    programmerEnregistrement();
  });

  if (champ.type === 'bool') {
    // La case vient avant son étiquette, comme pour tout interrupteur.
    ligne.append(controle, etiquette);
  } else {
    const bloc = document.createElement('div');
    bloc.className = 'reglage-controle';

    const suffixe = document.createElement('span');
    suffixe.className = 'reglage-unite';
    // ⚠️ La colonne d'unité est TOUJOURS posée, même vide. Sans elle, un champ sans unité
    // s'étendait plus à droite que ses voisins et la colonne de saisie partait en escalier :
    // défaut invisible en relisant le code, évident sur une capture.
    suffixe.textContent = champ.type === 'nombre' ? (champ.unite ?? '') : '';
    // `aria-hidden` : l'unité est déjà dans le libellé pour qui écoute l'écran, la répéter
    // ferait « Durée minimale ms ms ».
    suffixe.setAttribute('aria-hidden', 'true');

    bloc.append(controle, suffixe);
    ligne.append(etiquette, bloc);
  }

  return ligne;
}

function construire(): void {
  for (const groupe of GROUPES) {
    // ⚠️ Chaque groupe cherche SON PROPRE conteneur, celui de l'onglet ou il doit s'afficher :
    // ce n'est plus un unique `#reglages` qui recevait tout le monde avant la refonte en onglets.
    const hote = document.getElementById(groupe.conteneur);
    if (!hote) {
      console.error(`Groupe « ${groupe.titre} » sans conteneur : #${groupe.conteneur} introuvable.`);
      continue;
    }

    // `fieldset` et `legend` : le groupe est annoncé avant ses champs par les lecteurs d'écran,
    // ce qu'un simple titre visuel ne fait pas.
    const bloc = document.createElement('fieldset');
    bloc.className = 'groupe';
    bloc.id = `groupe-${groupe.titre}`;

    const legende = document.createElement('legend');
    legende.className = 'section';
    legende.textContent = groupe.titre;
    bloc.append(legende);

    for (const champ of groupe.champs) bloc.append(champVersDom(champ));
    hote.append(bloc);
  }
}

/** L'interrupteur qui déplie les réglages avancés. */
function brancherInterrupteurAvances(): void {
  const bascule = document.getElementById('avances') as HTMLInputElement | null;
  if (!bascule) return;

  bascule.addEventListener('change', () => {
    if (!courants) return;
    courants.show_advanced = bascule.checked;
    appliquerRepli();
    programmerEnregistrement();
  });
}

export async function brancherReglages(): Promise<void> {
  // Le catalogue est lu AVANT de construire l'ecran : un `select` construit vide
  // puis rempli apres coup afficherait brievement un choix inexistant.
  // ⚠️ Chargé séparément du catalogue des modèles, et dans son propre `try`. Un microphone
  // illisible ne doit pas vider la liste des modèles : ces deux écrans ont déjà été vidés une
  // fois par un chargement partagé qui échouait.
  try {
    const micros = await invoke<string[]>('microphones');
    const champ = GROUPES.flatMap((g) => g.champs).find((c) => c.cle === 'microphone');
    if (champ && champ.type === 'choix') {
      // La chaîne vide est le défaut et veut dire « celui du système ».
      champ.options = [['', 'Celui du système'], ...micros.map((nom): [string, string] => [nom, nom])];
    }
  } catch (erreur) {
    console.error('microphones illisibles', erreur);
  }

  construire();
  brancherInterrupteurAvances();

  try {
    courants = await invoke<Reglages>('lire_reglages');
    // ⚠️ Les défauts viennent du COEUR, jamais recopiés ici : deux listes de défauts finiraient
    // par diverger, et l'écran annoncerait alors des écarts qui n'existent pas, ou en tairait.
    defauts = await invoke<Reglages>('reglages_par_defaut');
    peupler(courants);

    const bascule = document.getElementById('avances') as HTMLInputElement | null;
    if (bascule) bascule.checked = courants.show_advanced;
    appliquerRepli();
  } catch (erreur) {
    dire('Les réglages n’ont pas pu être lus.', true);
    console.error('lire_reglages', erreur);
    return;
  }

  try {
    const chemin = await invoke<string>('chemin_reglages');
    const zone = document.getElementById('chemin-reglages');
    // Savoir OU vivent ses réglages est ce qui permet de les sauvegarder, de les comparer, ou de
    // nous les envoyer quand quelque chose ne va pas.
    if (zone) zone.textContent = chemin;
  } catch (erreur) {
    console.error('chemin_reglages', erreur);
  }

  // Les listes, après les champs simples : elles lisent l'état déjà chargé.
  brancherVocabulaire(
    () => courants?.vocabulary ?? [],
    async (liste) => {
      if (!courants) return;
      courants.vocabulary = liste;
      await enregistrer();
    },
  );
  brancherSubstitutions(
    () => courants?.substitutions ?? [],
    async (liste) => {
      if (!courants) return;
      courants.substitutions = liste;
      // ⚠️ On repasse par `enregistrer`, qui relit le disque avant d'écrire : écrire directement
      // ici rouvrirait le défaut du 2026-09-17, où un écran écrasait en silence un réglage
      // modifié par un autre.
      await enregistrer();
    },
  );
  await brancherHistorique();
}
