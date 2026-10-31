import { invoke } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';

/** Un appareil autorise, tel que le cœur le retient. */
type Appareil = {
  /** Empreinte de sa clé publique : la seule chose qui l'autorise. */
  empreinte: string;
  /** ⚠️ Déclaré par l'appareil lui-même, donc à afficher et jamais à croire. */
  nom: string;
  appaire_le: string;
};

/** Une demande en attente, telle qu'elle arrive du cœur. */
type Demande = { id: string; nom: string; code: string };

/**
 * L'écran d'appairage : la demande à accepter, et les téléphones déjà autorisés.
 *
 * ⛔ **C'est ICI que se donne l'autorisation, sur la machine qui recevra les frappes.** Un
 * appairage validé seulement côté téléphone ne vaudrait rien : celui qui accepte doit être celui
 * qui subit les conséquences.
 */
export function installerAppairage(racine: HTMLElement): void {
  racine.innerHTML = '';

  const groupe = document.createElement('fieldset');
  groupe.className = 'groupe';
  const legende = document.createElement('legend');
  legende.className = 'section';
  legende.textContent = 'Téléphones autorisés';
  groupe.append(legende);

  const zoneDemande = document.createElement('div');
  zoneDemande.id = 'appairage-demande';
  groupe.append(zoneDemande);

  const liste = document.createElement('div');
  liste.id = 'appairage-liste';
  groupe.append(liste);

  const message = document.createElement('p');
  message.className = 'message';
  message.setAttribute('role', 'status');
  groupe.append(message);

  racine.append(groupe);

  void rafraichir(liste, message);

  // ⛔ Une demande arrive quand elle arrive : le téléphone n'attend pas que cet écran soit ouvert.
  // L'écouteur est donc posé au chargement, pas à l'affichage de la section.
  void listen<Demande>('appairage-demande', (evenement) => {
    afficherDemande(zoneDemande, liste, message, evenement.payload);
  });
}

/**
 * Affiche la demande, avec les quatre chiffres à retrouver sur le téléphone.
 *
 * ⛔ **Le code n'est pas décoratif** : sans lui, un voisin sur le même wifi qui lance une demande
 * au même moment se ferait accepter à la place du bon téléphone. L'utilisateur attend une demande,
 * il en voit une, il dit oui. Le nombre est ce qui lie ce qu'il voit à ce qu'il tient en main.
 */
function afficherDemande(
  zone: HTMLElement,
  liste: HTMLElement,
  message: HTMLElement,
  demande: Demande,
): void {
  zone.innerHTML = '';

  const carte = document.createElement('div');
  carte.className = 'reglage';

  const texte = document.createElement('div');
  texte.style.flex = '1';
  const titre = document.createElement('span');
  titre.className = 'reglage-titre';
  titre.textContent = `« ${demande.nom} » demande à se connecter`;
  const aide = document.createElement('span');
  aide.className = 'reglage-aide';
  // ⚠️ La consigne dit quoi COMPARER, pas seulement quoi faire : « acceptez » seul ferait cliquer
  // sans regarder, ce qui annulerait l'intérêt du code.
  aide.textContent =
    'N’acceptez que si votre téléphone affiche exactement le même nombre. S’il en affiche un autre, refusez : ce n’est pas lui.';
  texte.append(titre, aide);

  const code = document.createElement('strong');
  code.textContent = demande.code;
  code.style.fontSize = '2rem';
  code.style.letterSpacing = '0.25em';
  code.setAttribute('aria-label', `Code à comparer : ${demande.code.split('').join(' ')}`);

  const accepter = document.createElement('button');
  accepter.type = 'button';
  accepter.textContent = 'Accepter';
  const refuser = document.createElement('button');
  refuser.type = 'button';
  refuser.textContent = 'Refuser';

  const repondre = async (accepte: boolean) => {
    accepter.disabled = true;
    refuser.disabled = true;
    const prise = await invoke<boolean>('reseau_repondre', { id: demande.id, accepte });
    zone.innerHTML = '';
    if (!prise) {
      // ⛔ Ne jamais laisser croire qu'un appairage a réussi quand le téléphone a déjà renoncé.
      message.textContent = 'La demande avait expiré. Relancez la connexion depuis le téléphone.';
    } else {
      message.textContent = accepte ? 'Téléphone autorisé.' : 'Demande refusée.';
    }
    await rafraichir(liste, message);
  };

  accepter.addEventListener('click', () => void repondre(true));
  refuser.addEventListener('click', () => void repondre(false));

  carte.append(texte, code, accepter, refuser);
  zone.append(carte);
}

/** Recharge la liste des appareils autorisés. */
async function rafraichir(liste: HTMLElement, message: HTMLElement): Promise<void> {
  liste.innerHTML = '';
  let appareils: Appareil[] = [];
  try {
    appareils = await invoke<Appareil[]>('reseau_appareils');
  } catch (erreur) {
    message.textContent = `Liste illisible : ${String(erreur)}`;
    return;
  }

  if (appareils.length === 0) {
    const vide = document.createElement('p');
    vide.className = 'reglage-aide';
    vide.textContent = 'Aucun téléphone autorisé pour l’instant.';
    liste.append(vide);
    return;
  }

  for (const appareil of appareils) {
    const ligne = document.createElement('div');
    // `reglage-valeur` comme les autres champs : texte a gauche, bouton a droite. Avec `reglage`
    // seul, le bouton s'etirait sur toute la colonne restante et collait au nom.
    ligne.className = 'reglage reglage-valeur';

    // Pas de `label` : il n'y a aucun champ a nommer, seulement le nom et l'empreinte empiles.
    const texte = document.createElement('div');
    texte.style.display = 'grid';
    texte.style.gap = '2px';
    const nom = document.createElement('span');
    nom.className = 'reglage-titre';
    nom.textContent = appareil.nom;
    const detail = document.createElement('span');
    detail.className = 'reglage-aide';
    // Les huit premiers caractères suffisent à distinguer deux appareils, et une empreinte
    // entière de 64 caractères serait illisible dans une liste.
    detail.textContent = `Empreinte ${appareil.empreinte.slice(0, 8)}…`;
    texte.append(nom, detail);

    const revoquer = document.createElement('button');
    revoquer.type = 'button';
    revoquer.className = 'action';
    revoquer.textContent = 'Révoquer';
    revoquer.addEventListener('click', () => {
      void (async () => {
        revoquer.disabled = true;
        try {
          await invoke<boolean>('reseau_revoquer', { empreinte: appareil.empreinte });
          message.textContent = `« ${appareil.nom} » ne peut plus se connecter.`;
        } catch (erreur) {
          message.textContent = `Révocation impossible : ${String(erreur)}`;
        }
        await rafraichir(liste, message);
      })();
    });

    ligne.append(texte, revoquer);
    liste.append(ligne);
  }
}
