import { invoke } from '@tauri-apps/api/core';
import { getCurrentWebview } from '@tauri-apps/api/webview';

/** Ce que le cœur rend d'un fichier transcrit. */
type FichierTranscrit = { texte: string; secondes: number; modele: string };

/**
 * Transcrire un fichier audio en le déposant dans la fenêtre.
 *
 * ⛔ **Les fichiers passent UN PAR UN**, dans l'ordre du dépôt : en lancer plusieurs à la fois
 * ferait tourner plusieurs moteurs en parallèle sur le même processeur, donc chacun plus lentement
 * que s'ils se suivaient, et une dictée lancée pendant ce temps attendrait derrière eux.
 *
 * ⚠️ Le texte n'est ni écrit au curseur ni ajouté à l'historique : la fenêtre active est Oyant
 * lui-même, puisqu'on vient d'y déposer le fichier. Il s'affiche, se copie, ou s'enregistre à côté
 * du fichier audio, sans jamais écraser un texte existant (le cœur numérote).
 */
export function installerDepot(): void {
  const voile = document.createElement('div');
  voile.className = 'depot-voile';
  voile.hidden = true;
  voile.setAttribute('aria-hidden', 'true');
  voile.textContent = 'Déposez un fichier audio pour le transcrire';

  const panneau = document.createElement('section');
  panneau.className = 'depot-panneau';
  panneau.hidden = true;
  panneau.setAttribute('aria-label', 'Fichiers transcrits');

  const entete = document.createElement('div');
  entete.className = 'depot-entete';
  const titre = document.createElement('h2');
  titre.textContent = 'Fichiers transcrits';
  const fermer = document.createElement('button');
  fermer.type = 'button';
  fermer.className = 'action';
  fermer.textContent = 'Fermer';
  entete.append(titre, fermer);

  const liste = document.createElement('div');
  liste.className = 'depot-liste';
  panneau.append(entete, liste);

  document.body.append(voile, panneau);

  const file: Array<{ chemin: string; carte: HTMLElement }> = [];
  let enCours = false;

  fermer.addEventListener('click', () => {
    // Ce qui attend encore continue : fermer le panneau ne doit pas faire perdre un travail.
    if (!enCours) {
      liste.innerHTML = '';
    }
    panneau.hidden = true;
  });

  const suivant = async () => {
    if (enCours) return;
    const prochain = file.shift();
    if (!prochain) return;
    enCours = true;
    await traiter(prochain.chemin, prochain.carte);
    enCours = false;
    void suivant();
  };

  // ⚠️ Protégé : si la webview n'est pas joignable, `getCurrentWebview` lève une exception, et
  // elle couperait tout le démarrage de l'interface qui suit cet appel dans `main.ts`.
  let webview: ReturnType<typeof getCurrentWebview>;
  try {
    webview = getCurrentWebview();
  } catch (erreur) {
    console.error('depot de fichiers indisponible', erreur);
    return;
  }
  void webview.onDragDropEvent((evenement) => {
    const charge = evenement.payload;
    if (charge.type === 'enter' || charge.type === 'over') {
      voile.hidden = false;
    } else if (charge.type === 'leave') {
      voile.hidden = true;
    } else if (charge.type === 'drop') {
      voile.hidden = true;
      if (charge.paths.length === 0) return;
      panneau.hidden = false;
      for (const chemin of charge.paths) {
        const carte = creerCarte(chemin);
        liste.append(carte);
        file.push({ chemin, carte });
      }
      void suivant();
    }
  });
}

function nomDuFichier(chemin: string): string {
  return chemin.split(/[\\/]/).pop() ?? chemin;
}

function creerCarte(chemin: string): HTMLElement {
  const carte = document.createElement('article');
  carte.className = 'depot-carte';
  const nom = document.createElement('span');
  nom.className = 'reglage-titre';
  nom.textContent = nomDuFichier(chemin);
  const etat = document.createElement('p');
  etat.className = 'reglage-aide';
  etat.setAttribute('role', 'status');
  etat.textContent = 'En attente…';
  carte.append(nom, etat);
  return carte;
}

async function traiter(chemin: string, carte: HTMLElement): Promise<void> {
  const etat = carte.querySelector('[role=status]') as HTMLElement;
  etat.textContent = 'Transcription en cours…';
  let resultat: FichierTranscrit;
  try {
    resultat = await invoke<FichierTranscrit>('transcrire_fichier', { chemin });
  } catch (erreur) {
    etat.textContent = String(erreur);
    etat.classList.add('depot-erreur');
    return;
  }

  etat.textContent = `Transcrit en ${resultat.secondes.toFixed(1).replace('.', ',')} s avec ${resultat.modele}.`;

  const texte = document.createElement('textarea');
  texte.className = 'depot-texte';
  texte.readOnly = true;
  texte.rows = Math.min(8, Math.max(3, Math.ceil(resultat.texte.length / 60)));
  texte.value = resultat.texte;
  texte.setAttribute('aria-label', `Texte de ${nomDuFichier(chemin)}`);

  const actions = document.createElement('div');
  actions.className = 'modele-actions';
  const copier = document.createElement('button');
  copier.type = 'button';
  copier.className = 'action action-principale';
  copier.textContent = 'Copier';
  const enregistrer = document.createElement('button');
  enregistrer.type = 'button';
  enregistrer.className = 'action';
  enregistrer.textContent = 'Enregistrer à côté du fichier';
  actions.append(copier, enregistrer);

  const retour = document.createElement('p');
  retour.className = 'reglage-aide';
  retour.setAttribute('role', 'status');

  copier.addEventListener('click', () => {
    void navigator.clipboard
      .writeText(resultat.texte)
      .then(() => (retour.textContent = 'Texte copié.'))
      .catch((e: unknown) => (retour.textContent = `Copie impossible : ${String(e)}`));
  });
  enregistrer.addEventListener('click', () => {
    enregistrer.disabled = true;
    void invoke<string>('enregistrer_texte_a_cote', { audio: chemin, texte: resultat.texte })
      .then((ecrit) => (retour.textContent = `Enregistré dans ${ecrit}.`))
      .catch((e: unknown) => {
        retour.textContent = String(e);
        enregistrer.disabled = false;
      });
  });

  carte.append(texte, actions, retour);
}
