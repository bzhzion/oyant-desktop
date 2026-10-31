// Les deux réglages qui sont des LISTES : les substitutions et l'historique.
//
// ⚠️ Ils ne passent pas par la génération déclarative de `reglages.ts`, qui ne sait produire que
// des booléens, des nombres, des choix et du texte. Les forcer dans ce moule aurait demandé un
// cinquième type de champ utilisé une seule fois, donc plus de code pour moins de clarté.

import { invoke } from '@tauri-apps/api/core';

import type { Reglages, Substitution } from './reglages';

type Entree = { at: number; text: string };

function dire(texte: string, erreur = false): void {
  const zone = document.getElementById('message-reglages');
  if (!zone) return;
  zone.textContent = texte;
  zone.classList.toggle('message-erreur', erreur);
}

/**
 * Les substitutions, éditables ligne par ligne.
 *
 * ⚠️ On enregistre à chaque modification plutôt que derrière un bouton « Appliquer » : le reste
 * de cet écran se comporte déjà comme ça, et deux façons d'enregistrer dans un même écran est
 * exactement le genre d'incohérence qui fait perdre une saisie.
 */
export function brancherSubstitutions(
  lire: () => Substitution[],
  ecrire: (liste: Substitution[]) => Promise<void>,
): void {
  const hote = document.getElementById('substitutions');
  if (!hote) return;

  const dessiner = (): void => {
    const liste = lire();
    hote.replaceChildren();

    const groupe = document.createElement('fieldset');
    groupe.className = 'groupe groupe-texte';
    const titre = document.createElement('legend');
    titre.textContent = 'Substitutions';
    groupe.append(titre);

    const aide = document.createElement('p');
    aide.className = 'reglage-aide';
    aide.textContent =
      'Remplacements appliqués automatiquement au texte dicté : abréviations, corrections, termes techniques. La casse est ignorée sauf si vous cochez la case.';
    groupe.append(aide);

    liste.forEach((regle, index) => {
      const ligne = document.createElement('div');
      ligne.className = 'substitution';

      const cherche = document.createElement('input');
      cherche.type = 'text';
      cherche.value = regle.from;
      cherche.placeholder = 'ce que je dis';
      cherche.setAttribute('aria-label', `Texte recherché, règle ${index + 1}`);
      cherche.addEventListener('change', () => {
        const suivante = lire().slice();
        suivante[index] = { ...suivante[index], from: cherche.value };
        void appliquer(suivante);
      });

      const vers = document.createElement('span');
      vers.className = 'substitution-fleche';
      vers.textContent = '→';
      vers.setAttribute('aria-hidden', 'true');

      const remplace = document.createElement('input');
      remplace.type = 'text';
      remplace.value = regle.to;
      remplace.placeholder = 'ce qui est écrit';
      remplace.setAttribute('aria-label', `Texte de remplacement, règle ${index + 1}`);
      remplace.addEventListener('change', () => {
        const suivante = lire().slice();
        suivante[index] = { ...suivante[index], to: remplace.value };
        void appliquer(suivante);
      });

      const casse = document.createElement('input');
      casse.type = 'checkbox';
      casse.className = 'interrupteur';
      casse.checked = regle.case_sensitive;
      casse.id = `casse-${index}`;
      casse.addEventListener('change', () => {
        const suivante = lire().slice();
        suivante[index] = { ...suivante[index], case_sensitive: casse.checked };
        void appliquer(suivante);
      });
      const casseEtiquette = document.createElement('label');
      casseEtiquette.htmlFor = casse.id;
      casseEtiquette.textContent = 'Aa';
      casseEtiquette.title = 'La casse compte';

      const retirer = document.createElement('button');
      retirer.type = 'button';
      retirer.className = 'action';
      retirer.textContent = 'Retirer';
      retirer.setAttribute('aria-label', `Retirer la règle ${index + 1}`);
      retirer.addEventListener('click', () => {
        void appliquer(lire().filter((_, autre) => autre !== index));
      });

      ligne.append(cherche, vers, remplace, casse, casseEtiquette, retirer);
      groupe.append(ligne);
    });

    const ajouter = document.createElement('button');
    ajouter.type = 'button';
    ajouter.className = 'action';
    ajouter.textContent = 'Ajouter une substitution';
    ajouter.addEventListener('click', () => {
      void appliquer([...lire(), { from: '', to: '', case_sensitive: false }]);
    });
    groupe.append(ajouter);

    hote.append(groupe);
  };

  const appliquer = async (liste: Substitution[]): Promise<void> => {
    try {
      await ecrire(liste);
      dessiner();
    } catch (erreur) {
      // ⛔ Jamais en silence : une règle qu'on croit enregistrée et qui ne l'est pas se découvre
      // en dictant, donc bien trop tard.
      dire(`Substitution non enregistrée : ${String(erreur)}`, true);
    }
  };

  dessiner();
}

/**
 * L'historique des dictées.
 *
 * ⚠️ Il n'apparaît QUE s'il contient quelque chose. Une section vide sur un écran déjà long
 * n'apprend rien, et l'historique est désactivé par défaut : elle serait donc vide chez la
 * plupart des gens.
 */
export async function brancherHistorique(): Promise<void> {
  const hote = document.getElementById('historique');
  if (!hote) return;

  const dessiner = async (): Promise<void> => {
    let entrees: Entree[] = [];
    try {
      entrees = await invoke<Entree[]>('etat_historique');
    } catch (erreur) {
      console.error('etat_historique', erreur);
    }

    hote.replaceChildren();
    if (entrees.length === 0) return;

    const groupe = document.createElement('fieldset');
    groupe.className = 'groupe groupe-texte';
    const titre = document.createElement('legend');
    titre.textContent = `Dictées récentes (${entrees.length})`;
    groupe.append(titre);

    const liste = document.createElement('ul');
    liste.className = 'historique';
    for (const entree of entrees) {
      const element = document.createElement('li');

      const quand = document.createElement('time');
      // L'horodatage est stocké en secondes Unix, donc indépendant du fuseau : c'est ici qu'on
      // le rend lisible, avec le fuseau de la machine qui regarde.
      const date = new Date(entree.at * 1000);
      quand.dateTime = date.toISOString();
      quand.textContent = date.toLocaleString();

      const contenu = document.createElement('span');
      contenu.className = 'historique-texte';
      contenu.textContent = entree.text;

      const copier = document.createElement('button');
      copier.type = 'button';
      copier.className = 'action';
      copier.textContent = 'Copier';
      copier.addEventListener('click', () => {
        void navigator.clipboard.writeText(entree.text).then(
          () => dire('Copié.'),
          (erreur) => dire(`Copie impossible : ${String(erreur)}`, true),
        );
      });

      element.append(quand, contenu, copier);
      liste.append(element);
    }
    groupe.append(liste);

    const vider = document.createElement('button');
    vider.type = 'button';
    vider.className = 'action action-danger';
    vider.textContent = 'Effacer l’historique';
    vider.addEventListener('click', () => {
      // ⚠️ Confirmation ET bouton désactivé pendant l'action : règle d'interface du parc sur
      // toute action destructive, le second empêchant un double clic d'en lancer deux.
      if (!window.confirm('Effacer toutes les dictées conservées ? Cette action est définitive.')) {
        return;
      }
      vider.disabled = true;
      void invoke('effacer_historique')
        .then(() => {
          dire('Historique effacé.');
          return dessiner();
        })
        .catch((erreur) => dire(`Effacement impossible : ${String(erreur)}`, true))
        .finally(() => {
          vider.disabled = false;
        });
    });
    groupe.append(vider);

    hote.append(groupe);
  };

  await dessiner();
}

/** Ce que l'écran de réglages appelle après chaque enregistrement. */
export async function rafraichirApresEnregistrement(_courants: Reglages): Promise<void> {
  await brancherHistorique();
}

/**
 * Le vocabulaire, une zone de texte a raison d'un terme par ligne.
 *
 * ⛔ **Ce n'est pas une liste de substitutions, et l'ecran doit le dire.** Une substitution
 * corrige le texte APRES coup, de facon exacte. Le vocabulaire est donne au moteur AVANT la
 * transcription pour qu'il se trompe moins : c'est un biais, donc rien ne peut etre remplace a
 * tort. Les deux blocs sont presentes dans l'ordre du traitement pour que la difference se voie.
 *
 * ⚠️ Une zone de texte plutot qu'une ligne par entree comme les substitutions : une entree de
 * vocabulaire est un seul mot, donc un champ par mot ferait vingt champs a remplir la ou un
 * copier-coller d'une liste suffit.
 */
export function brancherVocabulaire(
  lire: () => string[],
  ecrire: (liste: string[]) => Promise<void>,
): void {
  const hote = document.getElementById('vocabulaire');
  if (!hote) return;

  hote.replaceChildren();

  const groupe = document.createElement('fieldset');
  groupe.className = 'groupe groupe-texte';
  const titre = document.createElement('legend');
  titre.textContent = 'Vocabulaire';
  groupe.append(titre);

  const aide = document.createElement('p');
  aide.className = 'reglage-aide';
  aide.textContent =
    // ⚠️ Cette aide a dit « rien n'est remplace apres coup » jusqu'a la correction approchee du
    // 2026-10-31 : un texte d'aide decrit le comportement, il doit changer avec lui.
    'Un terme par ligne : noms propres, termes de votre métier, acronymes. Ils sont donnés à la reconnaissance vocale avant qu’elle transcrive, pour qu’elle les écrive correctement du premier coup. Si un mot transcrit se prononce et s’écrit presque comme l’un d’eux, il est aussi remplacé par votre terme (réglage « Corriger les mots proches de mon vocabulaire », onglet Sortie).';
  groupe.append(aide);

  // ⚠️ Cet ajout est AUTOMATIQUE et invisible autrement : sans cette phrase, l'utilisateur ne
  // saurait pas que ses substitutions alimentent aussi le vocabulaire, ni pourquoi le budget se
  // remplit plus vite que ce qu'il a tape.
  const deduits = document.createElement('p');
  deduits.className = 'reglage-aide';
  deduits.textContent =
    'Vos substitutions y contribuent aussi : quand leur remplacement ressemble à un terme (un nom, un acronyme), il est ajouté à cette liste avant la transcription, sans que vous ayez à le ressaisir. Les corrections de tournure, elles, ne le sont pas. Ces termes partagent le budget ci-dessous.';
  groupe.append(deduits);

  const zone = document.createElement('textarea');
  zone.id = 'vocabulaire-liste';
  zone.rows = 6;
  zone.spellcheck = false;
  zone.value = lire().join('\n');
  zone.placeholder = 'Kowalczyk\nLévothyrox\nVilleurbanne';
  zone.setAttribute('aria-label', 'Vocabulaire, un terme par ligne');

  const compte = document.createElement('p');
  compte.className = 'reglage-aide';
  const majCompte = (liste: string[]): void => {
    // ⚠️ On dit le PLAFOND et pas seulement le compte : le coeur ecarte les termes au-dela d'une
    // longueur totale, et l'apprendre par une transcription qui n'a pas marche serait pire.
    //
    // ⛔ **Le compte porte sur CETTE liste seule, et le libelle le dit.** Les termes deduits des
    // substitutions consomment le meme budget, mais leur filtre vit dans le coeur : le recopier
    // ici ferait deux implementations de la meme regle, qui finiraient par diverger sans que
    // rien ne le signale. Un libelle exact vaut mieux qu'un nombre faussement precis.
    const caracteres = liste.join(', ').length;
    compte.textContent =
      liste.length === 0
        ? 'Aucun terme saisi. Vos substitutions peuvent quand même en fournir.'
        : `${liste.length} terme${liste.length > 1 ? 's' : ''} saisi${liste.length > 1 ? 's' : ''}, ${caracteres} caractères sur les 600 du budget.` +
          (caracteres > 600 ? ' Les termes au-delà seront ignorés.' : '');
  };
  majCompte(lire());

  const decouper = (brut: string): string[] =>
    brut
      .split('\n')
      .map((ligne) => ligne.trim())
      .filter((ligne) => ligne.length > 0);

  zone.addEventListener('input', () => majCompte(decouper(zone.value)));
  zone.addEventListener('change', () => {
    const liste = decouper(zone.value);
    majCompte(liste);
    // ⚠️ Comme le reste de cet ecran, on enregistre a la perte du focus et pas derriere un bouton
    // « Appliquer » : deux facons d'enregistrer dans un meme ecran font perdre une saisie.
    void ecrire(liste).then(() => {
      // On reecrit la zone depuis la liste retenue, pour que l'utilisateur VOIE ce qui a ete
      // garde : les lignes vides et les espaces disparaissent, et le silence serait trompeur.
      zone.value = liste.join('\n');
    });
  });

  groupe.append(zone, compte);
  hote.append(groupe);
}
